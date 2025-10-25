pub mod settings;

use std::ops::Deref;
use std::path::Path;
use std::str::FromStr;
use std::sync::PoisonError;

use aws_sdk_cognitoidentity::primitives::DateTimeFormat;
use aws_sdk_cognitoidentity::types::Credentials;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::FromSql;
use rusqlite::{
    Connection,
    Error,
    ToSql,
    params,
};
use serde::de::DeserializeOwned;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    Map,
    Value,
};
use settings::Settings;
use thiserror::Error;
use tracing::{
    error,
    info,
    trace,
};
use uuid::Uuid;

use crate::cli::ConversationState;
use crate::util::directories::{
    DirectoryError,
    database_path,
};

macro_rules! migrations {
    ($($name:expr),*) => {{
        &[
            $(
                Migration {
                    name: $name,
                    sql: include_str!(concat!("sqlite_migrations/", $name, ".sql")),
                }
            ),*
        ]
    }};
}

const CREDENTIALS_KEY: &str = "telemetry-cognito-credentials";
const CLIENT_ID_KEY: &str = "telemetryClientId";
const CODEWHISPERER_PROFILE_KEY: &str = "api.codewhisperer.profile";
const START_URL_KEY: &str = "auth.idc.start-url";
const IDC_REGION_KEY: &str = "auth.idc.region";
const LOGIN_METHOD_KEY: &str = "auth.login-method";
// We include this key to remove for backwards compatibility
const CUSTOMIZATION_STATE_KEY: &str = "api.selectedCustomization";
const PROFILE_MIGRATION_KEY: &str = "profile.Migrated";
const HEARTBEAT_DATE_KEY: &str = "telemetry.lastHeartbeatDate";

const MIGRATIONS: &[Migration] = migrations![
    "000_migration_table",
    "001_history_table",
    "002_drop_history_in_ssh_docker",
    "003_improved_history_timing",
    "004_state_table",
    "005_auth_table",
    "006_make_state_blob",
    "007_conversations_table"
];

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CredentialsJson {
    pub access_key_id: Option<String>,
    pub secret_key: Option<String>,
    pub session_token: Option<String>,
    pub expiration: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthProfile {
    pub arn: String,
    pub profile_name: String,
}

impl From<amzn_codewhisperer_client::types::Profile> for AuthProfile {
    fn from(profile: amzn_codewhisperer_client::types::Profile) -> Self {
        Self {
            arn: profile.arn,
            profile_name: profile.profile_name,
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret").finish()
    }
}

impl<T> From<T> for Secret
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

// A cloneable error
#[derive(Debug, Clone, thiserror::Error)]
#[error("Failed to open database: {}", .0)]
pub struct DbOpenError(pub(crate) String);

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
    #[error(transparent)]
    FigUtilError(#[from] crate::util::UtilError),
    #[error(transparent)]
    DirectoryError(#[from] DirectoryError),
    #[error(transparent)]
    Rusqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    R2d2(#[from] r2d2::Error),
    #[error(transparent)]
    DbOpenError(#[from] DbOpenError),
    #[error("{}", .0)]
    PoisonError(String),
    #[error(transparent)]
    StringFromUtf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    StrFromUtf8(#[from] std::str::Utf8Error),
    #[error("`{}` is not a valid setting", .0)]
    InvalidSetting(String),
}

impl<T> From<PoisonError<T>> for DatabaseError {
    fn from(value: PoisonError<T>) -> Self {
        Self::PoisonError(value.to_string())
    }
}

#[derive(Debug)]
pub enum Table {
    /// The state table contains persistent application state.
    State,
    /// The conversations tables contains user chat conversations.
    Conversations,
    /// The auth table contains SSO and Builder ID credentials.
    Auth,
}

impl std::fmt::Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Table::State => write!(f, "state"),
            Table::Conversations => write!(f, "conversations"),
            Table::Auth => write!(f, "auth_kv"),
        }
    }
}

#[derive(Debug)]
struct Migration {
    name: &'static str,
    sql: &'static str,
}

#[derive(Clone, Debug)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
    pub settings: Settings,
}

impl Database {
    pub async fn new() -> Result<Self, DatabaseError> {
        let path = match cfg!(test) {
            true => {
                return Self {
                    pool: Pool::builder().build(SqliteConnectionManager::memory()).unwrap(),
                    settings: Settings::new().await?,
                }
                .migrate();
            },
            false => database_path()?,
        };

        // make the parent dir if it doesnt exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let conn = SqliteConnectionManager::file(&path);
        let pool = Pool::builder().build(conn)?;

        // Check the unix permissions of the database file, set them to 0600 if they are not
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&path)?;
            let mut permissions = metadata.permissions();
            if permissions.mode() & 0o777 != 0o600 {
                tracing::debug!(?path, "Setting database file permissions to 0600");
                permissions.set_mode(0o600);
                std::fs::set_permissions(path, permissions)?;
            }
        }

        Ok(Self {
            pool,
            settings: Settings::new().await?,
        }
        .migrate()
        .map_err(|e| DbOpenError(e.to_string()))?)
    }

    /// Get all entries for dumping the persistent application state.
    pub fn get_all_entries(&self) -> Result<Map<String, Value>, DatabaseError> {
        self.all_entries(Table::State)
    }

    /// Get cognito credentials used by toolkit telemetry.
    pub fn get_credentials_entry(&mut self) -> Result<Option<CredentialsJson>, DatabaseError> {
        self.get_json_entry::<CredentialsJson>(Table::State, CREDENTIALS_KEY)
    }

    /// Set cognito credentials used by toolkit telemetry.
    pub fn set_credentials_entry(&mut self, credentials: &Credentials) -> Result<usize, DatabaseError> {
        self.set_json_entry(Table::State, CREDENTIALS_KEY, CredentialsJson {
            access_key_id: credentials.access_key_id.clone(),
            secret_key: credentials.secret_key.clone(),
            session_token: credentials.session_token.clone(),
            expiration: credentials
                .expiration
                .and_then(|t| t.fmt(DateTimeFormat::DateTime).ok()),
        })
    }

    /// Get the current user profile used to determine API endpoints.
    pub fn get_auth_profile(&self) -> Result<Option<AuthProfile>, DatabaseError> {
        self.get_json_entry(Table::State, CODEWHISPERER_PROFILE_KEY)
    }

    /// Set the current user profile used to determine API endpoints.
    pub fn set_auth_profile(&mut self, profile: &AuthProfile) -> Result<(), DatabaseError> {
        self.set_json_entry(Table::State, CODEWHISPERER_PROFILE_KEY, profile)?;
        self.delete_entry(Table::State, CUSTOMIZATION_STATE_KEY)
    }

    /// Unset the current user profile used to determine API endpoints.
    pub fn unset_auth_profile(&mut self) -> Result<(), DatabaseError> {
        self.delete_entry(Table::State, CODEWHISPERER_PROFILE_KEY)?;
        self.delete_entry(Table::State, CUSTOMIZATION_STATE_KEY)
    }

    /// Get the client ID used for telemetry requests.
    pub fn get_client_id(&mut self) -> Result<Option<Uuid>, DatabaseError> {
        Ok(self
            .get_json_entry::<String>(Table::State, CLIENT_ID_KEY)?
            .and_then(|s| Uuid::from_str(&s).ok()))
    }

    /// Get changelog last version from state table
    pub fn get_changelog_last_version(&self) -> Result<Option<String>, DatabaseError> {
        self.get_entry::<String>(Table::State, "changelog.lastVersion")
    }

    /// Set changelog last version in state table
    pub fn set_changelog_last_version(&self, version: &str) -> Result<(), DatabaseError> {
        self.set_entry(Table::State, "changelog.lastVersion", version)?;
        Ok(())
    }

    /// Get changelog show count from state table
    pub fn get_changelog_show_count(&self) -> Result<Option<i64>, DatabaseError> {
        self.get_entry::<i64>(Table::State, "changelog.showCount")
    }

    /// Set changelog show count in state table
    pub fn set_changelog_show_count(&self, count: i64) -> Result<(), DatabaseError> {
        self.set_entry(Table::State, "changelog.showCount", count)?;
        Ok(())
    }

    /// Set the client ID used for telemetry requests.
    pub fn set_client_id(&mut self, client_id: Uuid) -> Result<usize, DatabaseError> {
        self.set_json_entry(Table::State, CLIENT_ID_KEY, client_id.to_string())
    }

    /// Get the start URL used for IdC login.
    pub fn get_start_url(&self) -> Result<Option<String>, DatabaseError> {
        self.get_json_entry::<String>(Table::State, START_URL_KEY)
    }

    /// Set the start URL used for IdC login.
    pub fn set_start_url(&mut self, start_url: String) -> Result<usize, DatabaseError> {
        self.set_json_entry(Table::State, START_URL_KEY, start_url)
    }

    /// Get the region used for IdC login.
    pub fn get_idc_region(&self) -> Result<Option<String>, DatabaseError> {
        // Annoyingly, this is encoded as a JSON string on older clients
        self.get_json_entry::<String>(Table::State, IDC_REGION_KEY)
    }

    /// Set the region used for IdC login.
    pub fn set_idc_region(&mut self, region: String) -> Result<usize, DatabaseError> {
        // Annoyingly, this is encoded as a JSON string on older clients
        self.set_json_entry(Table::State, IDC_REGION_KEY, region)
    }

    /// Get the last used login method.
    pub fn get_login_method(&self) -> Result<Option<String>, DatabaseError> {
        self.get_json_entry::<String>(Table::State, LOGIN_METHOD_KEY)
    }

    /// Set the last used login method.
    pub fn set_login_method(&mut self, method: String) -> Result<usize, DatabaseError> {
        self.set_json_entry(Table::State, LOGIN_METHOD_KEY, method)
    }

    /// Get if user has already completed a migration
    pub fn get_has_migrated(&self) -> Result<Option<bool>, DatabaseError> {
        self.get_entry::<bool>(Table::State, PROFILE_MIGRATION_KEY)
    }

    /// Set if user has already completed a migration
    pub fn set_has_migrated(&self) -> Result<usize, DatabaseError> {
        self.set_entry(Table::State, PROFILE_MIGRATION_KEY, true)
    }

    /// Check if daily heartbeat should be sent
    pub fn should_send_heartbeat(&self) -> bool {
        use chrono::Utc;
        let today = Utc::now().format("%Y-%m-%d").to_string();

        match self.get_entry::<String>(Table::State, HEARTBEAT_DATE_KEY) {
            Ok(Some(last_date)) => last_date != today,
            Ok(None) => true, // First time - definitely send
            Err(_) => false,  // Database error - don't send (might have already sent)
        }
    }

    /// Record that heartbeat was sent today
    pub fn record_heartbeat_sent(&self) -> Result<(), DatabaseError> {
        use chrono::Utc;
        let today = Utc::now().format("%Y-%m-%d").to_string();
        self.set_entry(Table::State, HEARTBEAT_DATE_KEY, today)?;
        Ok(())
    }

    // /// Get the model id used for last conversation state.
    // pub fn get_last_used_model_id(&self) -> Result<Option<String>, DatabaseError> {
    //     self.get_json_entry::<String>(Table::State, LAST_USED_MODEL_ID)
    // }

    // /// Set the model id used for last conversation state.
    // pub fn set_last_used_model_id(&mut self, last_used_model_id: String) -> Result<usize,
    // DatabaseError> {     self.set_json_entry(Table::State, LAST_USED_MODEL_ID,
    // last_used_model_id) }

    // /// UnsSet the model id used for last conversation state.
    // pub fn unset_last_used_model_id(&mut self) -> Result<(), DatabaseError> {
    //     self.delete_entry(Table::State, LAST_USED_MODEL_ID)
    // }

    /// Get a chat conversation given a path to the conversation.
    pub fn get_conversation_by_path(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<Option<ConversationState>, DatabaseError> {
        // We would need to encode this to support non utf8 paths.
        let path = match path.as_ref().to_str() {
            Some(path) => path,
            None => return Ok(None),
        };

        self.get_json_entry(Table::Conversations, path)
    }

    /// Set a chat conversation given a path to the conversation.
    pub fn set_conversation_by_path(
        &mut self,
        path: impl AsRef<Path>,
        state: &ConversationState,
    ) -> Result<usize, DatabaseError> {
        // We would need to encode this to support non utf8 paths.
        let path = match path.as_ref().to_str() {
            Some(path) => path,
            None => return Ok(0),
        };

        self.set_json_entry(Table::Conversations, path, state)
    }

    pub async fn get_secret(&self, key: &str) -> Result<Option<Secret>, DatabaseError> {
        trace!(key, "getting secret");
        Ok(self.get_entry::<String>(Table::Auth, key)?.map(Into::into))
    }

    pub async fn set_secret(&self, key: &str, value: &str) -> Result<(), DatabaseError> {
        trace!(key, "setting secret");
        self.set_entry(Table::Auth, key, value)?;
        Ok(())
    }

    pub async fn delete_secret(&self, key: &str) -> Result<(), DatabaseError> {
        trace!(key, "deleting secret");
        self.delete_entry(Table::Auth, key)
    }

    // Private functions. Do not expose.

    fn migrate(self) -> Result<Self, DatabaseError> {
        let mut conn = self.pool.get()?;
        let transaction = conn.transaction()?;

        let max_version = max_migration_version(&transaction);

        for (version, migration) in MIGRATIONS.iter().enumerate() {
            if has_migration(&transaction, version, max_version)? {
                continue;
            }

            // execute the migration
            transaction.execute_batch(migration.sql)?;

            info!(%version, name =% migration.name, "Applying migration");

            // insert the migration entry
            transaction.execute(
                "INSERT INTO migrations (version, migration_time) VALUES (?1, strftime('%s', 'now'));",
                params![version],
            )?;
        }

        // commit the transaction
        transaction.commit()?;

        Ok(self)
    }

    fn get_entry<T: FromSql>(&self, table: Table, key: impl AsRef<str>) -> Result<Option<T>, DatabaseError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!("SELECT value FROM {table} WHERE key = ?1"))?;
        match stmt.query_row([key.as_ref()], |row| row.get(0)) {
            Ok(data) => Ok(Some(data)),
            Err(Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn set_entry(&self, table: Table, key: impl AsRef<str>, value: impl ToSql) -> Result<usize, DatabaseError> {
        Ok(self.pool.get()?.execute(
            &format!("INSERT OR REPLACE INTO {table} (key, value) VALUES (?1, ?2)"),
            params![key.as_ref(), value],
        )?)
    }

    fn get_json_entry<T: DeserializeOwned>(
        &self,
        table: Table,
        key: impl AsRef<str>,
    ) -> Result<Option<T>, DatabaseError> {
        Ok(match self.get_entry::<String>(table, key.as_ref())? {
            Some(value) => serde_json::from_str(&value)?,
            None => None,
        })
    }

    fn set_json_entry(
        &self,
        table: Table,
        key: impl AsRef<str>,
        value: impl Serialize,
    ) -> Result<usize, DatabaseError> {
        self.set_entry(table, key, serde_json::to_string(&value)?)
    }

    fn delete_entry(&self, table: Table, key: impl AsRef<str>) -> Result<(), DatabaseError> {
        self.pool
            .get()?
            .execute(&format!("DELETE FROM {table} WHERE key = ?1"), [key.as_ref()])?;
        Ok(())
    }

    fn all_entries(&self, table: Table) -> Result<Map<String, serde_json::Value>, DatabaseError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!("SELECT key, value FROM {table}"))?;
        let rows = stmt.query_map([], |row| {
            let key = row.get(0)?;
            let value = Value::String(row.get(1)?);
            Ok((key, value))
        })?;

        let mut map = Map::new();
        for row in rows {
            let (key, value) = row?;
            map.insert(key, value);
        }

        Ok(map)
    }
}

fn max_migration_version<C: Deref<Target = Connection>>(conn: &C) -> Option<i64> {
    let mut stmt = conn.prepare("SELECT MAX(version) FROM migrations").ok()?;
    stmt.query_row([], |row| row.get(0)).ok()
}

fn has_migration<C: Deref<Target = Connection>>(
    conn: &C,
    version: usize,
    max_version: Option<i64>,
) -> Result<bool, DatabaseError> {
    // IMPORTANT: Due to a bug with the first 7 migrations, we have to check manually
    //
    // Background: the migrations table stores two identifying keys: the sqlite auto-generated
    // auto-incrementing key `id`, and the `version` which is the index of the `MIGRATIONS`
    // constant.
    //
    // Checking whether a migration exists would compare id with version, but since id is 1-indexed
    // and version is 0-indexed, we would actually skip the last migration! Therefore, it's
    // possible users are missing a critical migration (namely, auth_kv table creation) when
    // upgrading to the qchat build (which includes two new migrations). Hence, we have to check
    // all migrations until version 7 to make sure that nothing is missed.
    if version <= 7 {
        let mut stmt = match conn.prepare("SELECT COUNT(*) FROM migrations WHERE version = ?1") {
            Ok(stmt) => stmt,
            // If the migrations table does not exist, then we can reasonably say no migrations
            // will exist.
            Err(Error::SqliteFailure(_, Some(msg))) if msg.contains("no such table") => {
                return Ok(false);
            },
            Err(err) => return Err(err.into()),
        };
        let count: i32 = stmt.query_row([version], |row| row.get(0))?;
        return Ok(count >= 1);
    }

    // Continuing from the previously implemented logic - any migrations after the 7th can have a simple
    // maximum version check, since we can reasonably assume if any version >=7 will have all
    // migrations prior to it.
    #[allow(clippy::match_like_matches_macro)]
    Ok(match max_version {
        Some(max_version) if max_version >= version as i64 => true,
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_errors() -> Vec<DatabaseError> {
        vec![
            std::io::Error::new(std::io::ErrorKind::InvalidData, "oops").into(),
            serde_json::from_str::<()>("oops").unwrap_err().into(),
            crate::util::directories::DirectoryError::NoHomeDirectory.into(),
            rusqlite::Error::SqliteSingleThreadedMode.into(),
            // r2d2::Error
            DbOpenError("oops".into()).into(),
            PoisonError::<()>::new(()).into(),
        ]
    }

    #[test]
    fn test_error_display_debug() {
        for error in all_errors() {
            eprintln!("{}", error);
            eprintln!("{:?}", error);
        }
    }

    #[tokio::test]
    async fn test_migrate() {
        let db = Database::new().await.unwrap();

        // assert migration count is correct
        let max_migration = max_migration_version(&&*db.pool.get().unwrap());
        assert_eq!(max_migration, Some(MIGRATIONS.len() as i64 - 1));
    }

    #[test]
    fn list_migrations() {
        // Assert the migrations are in order
        assert!(MIGRATIONS.windows(2).all(|w| w[0].name <= w[1].name));

        // Assert the migrations start with their index
        assert!(
            MIGRATIONS
                .iter()
                .enumerate()
                .all(|(i, m)| m.name.starts_with(&format!("{:03}_", i)))
        );

        // Assert all the files in migrations/ are in the list
        let migration_folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/database/sqlite_migrations");
        let migration_count = std::fs::read_dir(migration_folder).unwrap().count();
        assert_eq!(MIGRATIONS.len(), migration_count);
    }

    #[tokio::test]
    async fn state_table_tests() {
        let db = Database::new().await.unwrap();

        // set
        db.set_entry(Table::State, "test", "test").unwrap();
        db.set_entry(Table::State, "int", 1).unwrap();
        db.set_entry(Table::State, "float", 1.0).unwrap();
        db.set_entry(Table::State, "bool", true).unwrap();
        db.set_entry(Table::State, "array", vec![1, 2, 3]).unwrap();
        db.set_entry(Table::State, "object", serde_json::json!({ "test": "test" }))
            .unwrap();
        db.set_entry(Table::State, "binary", b"test".to_vec()).unwrap();

        // unset
        db.delete_entry(Table::State, "test").unwrap();
        db.delete_entry(Table::State, "int").unwrap();

        // is some
        assert!(db.get_entry::<String>(Table::State, "test").unwrap().is_none());
        assert!(db.get_entry::<i32>(Table::State, "int").unwrap().is_none());
        assert!(db.get_entry::<f32>(Table::State, "float").unwrap().is_some());
        assert!(db.get_entry::<bool>(Table::State, "bool").unwrap().is_some());
    }

    #[tokio::test]
    #[ignore = "not on ci"]
    async fn test_set_password() {
        let key = "test_set_password";
        let store = Database::new().await.unwrap();
        store.set_secret(key, "test").await.unwrap();
        assert_eq!(store.get_secret(key).await.unwrap().unwrap().0, "test");
        store.delete_secret(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "not on ci"]
    async fn secret_get_time() {
        let key = "test_secret_get_time";
        let store = Database::new().await.unwrap();
        store.set_secret(key, "1234").await.unwrap();

        let now = std::time::Instant::now();
        for _ in 0..100 {
            store.get_secret(key).await.unwrap();
        }

        println!("duration: {:?}", now.elapsed() / 100);

        store.delete_secret(key).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "not on ci"]
    async fn secret_delete() {
        let key = "test_secret_delete";

        let store = Database::new().await.unwrap();
        store.set_secret(key, "1234").await.unwrap();
        assert_eq!(store.get_secret(key).await.unwrap().unwrap().0, "1234");
        store.delete_secret(key).await.unwrap();
        assert_eq!(store.get_secret(key).await.unwrap(), None);
    }
}
