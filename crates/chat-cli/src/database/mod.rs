pub mod secret_store;
pub mod settings;
pub mod state;

use std::ops::Deref;
use std::sync::PoisonError;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::FromSql;
use rusqlite::{
    Connection,
    Error,
    ToSql,
    params,
};
use secret_store::SecretStore;
use serde_json::{
    Map,
    Value,
};
use settings::Settings;
use thiserror::Error;
use tracing::info;

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

const MIGRATIONS: &[Migration] = migrations![
    "000_migration_table",
    "001_history_table",
    "002_drop_history_in_ssh_docker",
    "003_improved_history_timing",
    "004_state_table",
    "005_auth_table"
];

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
    #[cfg(target_os = "macos")]
    #[error("Security error: {}", .0)]
    Security(String),
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
    /// The state table contains persistant application state.
    State,
    /// The conversations tables contains user chat conversations.
    #[allow(dead_code)]
    Conversations,
    #[cfg(not(target_os = "macos"))]
    /// The auth table contains
    Auth,
}

impl std::fmt::Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Table::State => write!(f, "state"),
            Table::Conversations => write!(f, "conversations"),
            #[cfg(not(target_os = "macos"))]
            Table::Auth => write!(f, "auth_kv"),
        }
    }
}

#[derive(Debug)]
struct Migration {
    name: &'static str,
    sql: &'static str,
}

#[derive(Debug)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
    pub settings: Settings,
    pub secret_store: SecretStore,
}

impl Database {
    pub async fn new() -> Result<Self, DatabaseError> {
        let path = match cfg!(test) {
            true => {
                return Ok(Self {
                    pool: Pool::builder().build(SqliteConnectionManager::memory()).unwrap(),
                    settings: Settings::new().await?,
                    secret_store: SecretStore::new().await?,
                });
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
            secret_store: SecretStore::new().await?,
        }
        .migrate()
        .map_err(|e| DbOpenError(e.to_string()))?)
    }

    fn migrate(self) -> Result<Self, DatabaseError> {
        let mut conn = self.pool.get()?;
        let transaction = conn.transaction()?;

        // select the max migration id
        let max_id = max_migration(&transaction);

        for (version, migration) in MIGRATIONS.iter().enumerate() {
            // skip migrations that already exist
            match max_id {
                Some(max_id) if max_id >= version as i64 => continue,
                _ => (),
            };

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

    fn set_entry<T: ToSql>(&self, table: Table, key: impl AsRef<str>, value: T) -> Result<(), DatabaseError> {
        self.pool.get()?.execute(
            &format!("INSERT OR REPLACE INTO {table} (key, value) VALUES (?1, ?2)"),
            params![key.as_ref(), value],
        )?;
        Ok(())
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
            let value = row.get(1)?;
            Ok((key, value))
        })?;

        let mut map = Map::new();
        for row in rows {
            let (key, value) = row?;
            map.insert(key, value);
        }

        Ok(map)
    }

    /// Return a json encoded string entry
    fn get_string(&self, table: Table, key: impl AsRef<str>) -> Result<Option<String>, DatabaseError> {
        Ok(self.get_entry(table, key)?.and_then(|value| match value {
            Value::String(s) => Some(s),
            _ => None,
        }))
    }
}

fn max_migration<C: Deref<Target = Connection>>(conn: &C) -> Option<i64> {
    let mut stmt = conn.prepare("SELECT MAX(id) FROM migrations").ok()?;
    stmt.query_row([], |row| row.get(0)).ok()
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
        let max_migration = max_migration(&&*db.pool.get().unwrap());
        assert_eq!(max_migration, Some(MIGRATIONS.len() as i64));
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
        let migration_folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sqlite/migrations");
        let migration_count = std::fs::read_dir(migration_folder).unwrap().count();
        assert_eq!(MIGRATIONS.len(), migration_count);
    }
}
