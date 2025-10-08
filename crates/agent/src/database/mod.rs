use std::ops::Deref;
use std::str::FromStr;

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
use tracing::{
    info,
    trace,
};
use uuid::Uuid;

use crate::agent::util::directories::database_path;
use crate::agent::util::error::{
    ErrorContext,
    UtilError,
};
use crate::agent::util::is_integ_test;

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

// No migrations yet.
const MIGRATIONS: &[Migration] = migrations!["000_create_migration_auth_state_tables"];

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

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

// #[derive(Debug, Error)]
// pub enum DatabaseError {
//     #[error(transparent)]
//     IoError(#[from] std::io::Error),
//     #[error(transparent)]
//     DirectoryError(#[from] DirectoryError),
//     #[error(transparent)]
//     JsonError(#[from] serde_json::Error),
//     #[error(transparent)]
//     Rusqlite(#[from] rusqlite::Error),
//     #[error(transparent)]
//     R2d2(#[from] r2d2::Error),
//     #[error(transparent)]
//     DbOpenError(#[from] DbOpenError),
//     #[error("{}", .0)]
//     PoisonError(String),
//     #[error(transparent)]
//     StringFromUtf8(#[from] std::string::FromUtf8Error),
//     #[error(transparent)]
//     StrFromUtf8(#[from] std::str::Utf8Error),
// }
//
// impl<T> From<PoisonError<T>> for DatabaseError {
//     fn from(value: PoisonError<T>) -> Self {
//         Self::PoisonError(value.to_string())
//     }
// }

#[derive(Debug)]
pub enum Table {
    /// The auth table contains SSO and Builder ID credentials.
    Auth,
    /// The state table contains persistent application state.
    State,
}

impl std::fmt::Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Table::Auth => write!(f, "auth_kv"),
            Table::State => write!(f, "state"),
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
}

impl Database {
    pub async fn new() -> Result<Self, UtilError> {
        let path = match cfg!(test) && !is_integ_test() {
            true => {
                return Self {
                    pool: Pool::builder().build(SqliteConnectionManager::memory()).unwrap(),
                }
                .migrate();
            },
            false => database_path()?,
        };

        // make the parent dir if it doesnt exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .context(format!("failed to create parent directory {:?} for database", parent))?;
            }
        }

        let conn = SqliteConnectionManager::file(&path);
        let pool = Pool::builder().build(conn)?;

        // Check the unix permissions of the database file, set them to 0600 if they are not
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&path).context(format!("failed to get metadata for file {:?}", path))?;
            let mut permissions = metadata.permissions();
            if permissions.mode() & 0o777 != 0o600 {
                tracing::debug!(?path, "Setting database file permissions to 0600");
                permissions.set_mode(0o600);
                std::fs::set_permissions(&path, permissions)
                    .context(format!("failed to set file permissions for file {:?}", path))?;
            }
        }

        Self { pool }
            .migrate()
            .map_err(|e| UtilError::DbOpenError(e.to_string()))
    }

    /// Get all entries for dumping the persistent application state.
    pub fn get_all_entries(&self) -> Result<Map<String, Value>, UtilError> {
        self.all_entries(Table::State)
    }

    /// Get the current user profile used to determine API endpoints.
    pub fn get_auth_profile(&self) -> Result<Option<AuthProfile>, UtilError> {
        self.get_json_entry(Table::State, CODEWHISPERER_PROFILE_KEY)
    }

    /// Set the current user profile used to determine API endpoints.
    pub fn set_auth_profile(&mut self, profile: &AuthProfile) -> Result<(), UtilError> {
        self.set_json_entry(Table::State, CODEWHISPERER_PROFILE_KEY, profile);
        Ok(())
    }

    /// Unset the current user profile used to determine API endpoints.
    pub fn unset_auth_profile(&mut self) -> Result<(), UtilError> {
        self.delete_entry(Table::State, CODEWHISPERER_PROFILE_KEY);
        Ok(())
    }

    /// Get the client ID used for telemetry requests.
    pub fn get_client_id(&mut self) -> Result<Option<Uuid>, UtilError> {
        Ok(self
            .get_json_entry::<String>(Table::State, CLIENT_ID_KEY)?
            .and_then(|s| Uuid::from_str(&s).ok()))
    }

    /// Set the client ID used for telemetry requests.
    pub fn set_client_id(&mut self, client_id: Uuid) -> Result<usize, UtilError> {
        self.set_json_entry(Table::State, CLIENT_ID_KEY, client_id.to_string())
    }

    /// Get the start URL used for IdC login.
    pub fn get_start_url(&self) -> Result<Option<String>, UtilError> {
        self.get_json_entry::<String>(Table::State, START_URL_KEY)
    }

    /// Set the start URL used for IdC login.
    pub fn set_start_url(&mut self, start_url: String) -> Result<usize, UtilError> {
        self.set_json_entry(Table::State, START_URL_KEY, start_url)
    }

    /// Get the region used for IdC login.
    pub fn get_idc_region(&self) -> Result<Option<String>, UtilError> {
        // Annoyingly, this is encoded as a JSON string on older clients
        self.get_json_entry::<String>(Table::State, IDC_REGION_KEY)
    }

    /// Set the region used for IdC login.
    pub fn set_idc_region(&mut self, region: String) -> Result<usize, UtilError> {
        // Annoyingly, this is encoded as a JSON string on older clients
        self.set_json_entry(Table::State, IDC_REGION_KEY, region)
    }

    pub async fn get_secret(&self, key: &str) -> Result<Option<Secret>, UtilError> {
        trace!(key, "getting secret");
        Ok(self.get_entry::<String>(Table::Auth, key)?.map(Into::into))
    }

    pub async fn set_secret(&self, key: &str, value: &str) -> Result<(), UtilError> {
        trace!(key, "setting secret");
        self.set_entry(Table::Auth, key, value)?;
        Ok(())
    }

    pub async fn delete_secret(&self, key: &str) -> Result<(), UtilError> {
        trace!(key, "deleting secret");
        self.delete_entry(Table::Auth, key)
    }

    fn migrate(self) -> Result<Self, UtilError> {
        let mut conn = self.pool.get()?;
        let transaction = conn.transaction()?;

        let max_version = max_migration_version(&transaction);

        for (version, migration) in MIGRATIONS.iter().enumerate() {
            if max_version.is_some_and(|max| version as i64 <= max) {
                continue;
            }

            info!(%version, name =% migration.name, "Applying migration");
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                // Migration time is inserted as a Unix timestamp (number of seconds since Unix Epoch).
                "INSERT INTO migrations (version, migration_time) VALUES (?1, strftime('%s', 'now'));",
                params![version],
            )?;
        }

        transaction.commit()?;

        Ok(self)
    }

    fn get_entry<T: FromSql>(&self, table: Table, key: impl AsRef<str>) -> Result<Option<T>, UtilError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!("SELECT value FROM {table} WHERE key = ?1"))?;
        match stmt.query_row([key.as_ref()], |row| row.get(0)) {
            Ok(data) => Ok(Some(data)),
            Err(Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn set_entry(&self, table: Table, key: impl AsRef<str>, value: impl ToSql) -> Result<usize, UtilError> {
        Ok(self.pool.get()?.execute(
            &format!("INSERT OR REPLACE INTO {table} (key, value) VALUES (?1, ?2)"),
            params![key.as_ref(), value],
        )?)
    }

    fn get_json_entry<T: DeserializeOwned>(&self, table: Table, key: impl AsRef<str>) -> Result<Option<T>, UtilError> {
        Ok(match self.get_entry::<String>(table, key.as_ref())? {
            Some(value) => serde_json::from_str(&value)?,
            None => None,
        })
    }

    fn set_json_entry(&self, table: Table, key: impl AsRef<str>, value: impl Serialize) -> Result<usize, UtilError> {
        self.set_entry(table, key, serde_json::to_string(&value)?)
    }

    fn delete_entry(&self, table: Table, key: impl AsRef<str>) -> Result<(), UtilError> {
        self.pool
            .get()?
            .execute(&format!("DELETE FROM {table} WHERE key = ?1"), [key.as_ref()])?;
        Ok(())
    }

    fn all_entries(&self, table: Table) -> Result<Map<String, serde_json::Value>, UtilError> {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::PoisonError;

    use super::*;

    fn all_errors() -> Vec<UtilError> {
        vec![
            Err::<(), std::io::Error>(std::io::Error::new(std::io::ErrorKind::InvalidData, "oops"))
                .context(format!("made an oopsy at file {:?}", PathBuf::from("oopsy_path")))
                .unwrap_err(),
            serde_json::from_str::<()>("oops").unwrap_err().into(),
            UtilError::MissingDataLocalDir,
            rusqlite::Error::SqliteSingleThreadedMode.into(),
            UtilError::DbOpenError("oops".into()),
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
