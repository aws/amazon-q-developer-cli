use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::FromSql;
use rusqlite::{
    Error,
    ToSql,
    params,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::trace;

use crate::agent::util::directories::database_path;
use crate::agent::util::error::{
    ErrorContext,
    UtilError,
};
use crate::agent::util::is_integ_test;

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

#[derive(Debug)]
pub enum Table {
    /// The auth table contains SSO and Builder ID credentials.
    Auth,
}

impl std::fmt::Display for Table {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Table::Auth => write!(f, "auth_kv"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    pub async fn new() -> Result<Self, UtilError> {
        let path = match cfg!(test) && !is_integ_test() {
            true => {
                return Ok(Self {
                    pool: Pool::builder().build(SqliteConnectionManager::memory()).unwrap(),
                });
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

        Ok(Self { pool })
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

    fn delete_entry(&self, table: Table, key: impl AsRef<str>) -> Result<(), UtilError> {
        self.pool
            .get()?
            .execute(&format!("DELETE FROM {table} WHERE key = ?1"), [key.as_ref()])?;
        Ok(())
    }
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
