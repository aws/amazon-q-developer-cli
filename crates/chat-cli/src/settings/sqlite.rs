use std::ops::Deref;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::LazyLock;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::FromSql;
use rusqlite::{
    Connection,
    Error,
    ToSql,
    params,
};
use serde_json::Map;
use tracing::info;

use super::error::DbOpenError;
use crate::settings::Result;
use crate::util::directories::fig_data_dir;

const STATE_TABLE_NAME: &str = "state";
const AUTH_TABLE_NAME: &str = "auth_kv";

pub static DATABASE: LazyLock<Result<Db, DbOpenError>> = LazyLock::new(|| {
    let db = Db::new().map_err(|e| DbOpenError(e.to_string()))?;
    db.migrate().map_err(|e| DbOpenError(e.to_string()))?;
    Ok(db)
});

pub fn database() -> Result<&'static Db, DbOpenError> {
    match DATABASE.as_ref() {
        Ok(db) => Ok(db),
        Err(err) => Err(err.clone()),
    }
}

#[derive(Debug)]
struct Migration {
    name: &'static str,
    sql: &'static str,
}

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

#[derive(Debug, Clone)]
pub struct Db {
    pub(crate) pool: Pool<SqliteConnectionManager>,
}

impl Db {
    fn path() -> Result<PathBuf> {
        Ok(fig_data_dir()?.join("data.sqlite3"))
    }

    pub fn new() -> Result<Self> {
        Self::open(&Self::path()?)
    }

    fn open(path: &Path) -> Result<Self> {
        // make the parent dir if it doesnt exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let conn = SqliteConnectionManager::file(path);
        let pool = Pool::builder().build(conn)?;

        // Check the unix permissions of the database file, set them to 0600 if they are not
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(path)?;
            let mut permissions = metadata.permissions();
            if permissions.mode() & 0o777 != 0o600 {
                tracing::debug!(?path, "Setting database file permissions to 0600");
                permissions.set_mode(0o600);
                std::fs::set_permissions(path, permissions)?;
            }
        }

        Ok(Self { pool })
    }

    pub(crate) fn mock() -> Self {
        let conn = SqliteConnectionManager::memory();
        let pool = Pool::builder().build(conn).unwrap();
        Self { pool }
    }

    pub fn migrate(&self) -> Result<()> {
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

        Ok(())
    }

    fn get_value<T: FromSql>(&self, table: &'static str, key: impl AsRef<str>) -> Result<Option<T>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!("SELECT value FROM {table} WHERE key = ?1"))?;
        match stmt.query_row([key.as_ref()], |row| row.get(0)) {
            Ok(data) => Ok(Some(data)),
            Err(Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn get_state_value(&self, key: impl AsRef<str>) -> Result<Option<serde_json::Value>> {
        self.get_value(STATE_TABLE_NAME, key)
    }

    pub fn get_auth_value(&self, key: impl AsRef<str>) -> Result<Option<String>> {
        self.get_value(AUTH_TABLE_NAME, key)
    }

    fn set_value<T: ToSql>(&self, table: &'static str, key: impl AsRef<str>, value: T) -> Result<()> {
        self.pool.get()?.execute(
            &format!("INSERT OR REPLACE INTO {table} (key, value) VALUES (?1, ?2)"),
            params![key.as_ref(), value],
        )?;
        Ok(())
    }

    pub fn set_state_value(&self, key: impl AsRef<str>, value: impl Into<serde_json::Value>) -> Result<()> {
        self.set_value(STATE_TABLE_NAME, key, value.into())
    }

    pub fn set_auth_value(&self, key: impl AsRef<str>, value: impl Into<String>) -> Result<()> {
        self.set_value(AUTH_TABLE_NAME, key, value.into())
    }

    fn unset_value(&self, table: &'static str, key: impl AsRef<str>) -> Result<()> {
        self.pool
            .get()?
            .execute(&format!("DELETE FROM {table} WHERE key = ?1"), [key.as_ref()])?;
        Ok(())
    }

    pub fn unset_state_value(&self, key: impl AsRef<str>) -> Result<()> {
        self.unset_value(STATE_TABLE_NAME, key)
    }

    pub fn unset_auth_value(&self, key: impl AsRef<str>) -> Result<()> {
        self.unset_value(AUTH_TABLE_NAME, key)
    }

    fn is_value_set(&self, table: &'static str, key: impl AsRef<str>) -> Result<bool> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!("SELECT value FROM {table} WHERE key = ?1"))?;
        match stmt.query_row([key.as_ref()], |_| Ok(())) {
            Ok(()) => Ok(true),
            Err(Error::QueryReturnedNoRows) => Ok(false),
            Err(err) => Err(err.into()),
        }
    }

    #[allow(dead_code)]
    pub fn is_state_value_set(&self, key: impl AsRef<str>) -> Result<bool> {
        self.is_value_set(STATE_TABLE_NAME, key)
    }

    #[allow(dead_code)]
    pub fn is_auth_value_set(&self, key: impl AsRef<str>) -> Result<bool> {
        self.is_value_set(AUTH_TABLE_NAME, key)
    }

    fn all_values(&self, table: &'static str) -> Result<Map<String, serde_json::Value>> {
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

    pub fn all_state_values(&self) -> Result<Map<String, serde_json::Value>> {
        self.all_values(STATE_TABLE_NAME)
    }

    // atomic style operations

    fn atomic_op<T: FromSql + ToSql>(
        &self,
        key: impl AsRef<str>,
        op: impl FnOnce(&Option<T>) -> Option<T>,
    ) -> Result<Option<T>> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        let value = tx.query_row::<Option<T>, _, _>(
            &format!("SELECT value FROM {STATE_TABLE_NAME} WHERE key = ?1"),
            [key.as_ref()],
            |row| row.get(0),
        );

        let value_0: Option<T> = match value {
            Ok(value) => value,
            Err(Error::QueryReturnedNoRows) => None,
            Err(err) => return Err(err.into()),
        };

        let value_1 = op(&value_0);

        if let Some(value) = value_1 {
            tx.execute(
                &format!("INSERT OR REPLACE INTO {STATE_TABLE_NAME} (key, value) VALUES (?1, ?2)"),
                params![key.as_ref(), value],
            )?;
        } else {
            tx.execute(
                &format!("DELETE FROM {STATE_TABLE_NAME} WHERE key = ?1"),
                [key.as_ref()],
            )?;
        }

        tx.commit()?;

        Ok(value_0)
    }

    /// Atomically get the value of a key, then perform an or operation on it
    /// and set the new value. If the key does not exist, set it to the or value.
    pub fn atomic_bool_or(&self, key: impl AsRef<str>, or: bool) -> Result<bool> {
        self.atomic_op::<serde_json::Value>(key, |val| match val {
            // Some(val) => Some(serde_json::Value::Bool( || or)),
            Some(serde_json::Value::Bool(b)) => Some(serde_json::Value::Bool(*b || or)),
            Some(_) | None => Some(serde_json::Value::Bool(or)),
        })
        .map(|val| val.and_then(|val| val.as_bool()).unwrap_or(false))
    }
}

fn max_migration<C: Deref<Target = Connection>>(conn: &C) -> Option<i64> {
    let mut stmt = conn.prepare("SELECT MAX(id) FROM migrations").ok()?;
    stmt.query_row([], |row| row.get(0)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock() -> Db {
        let db = Db::mock();
        db.migrate().unwrap();
        db
    }

    #[test]
    fn test_migrate() {
        let db = mock();

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

    #[test]
    fn state_table_tests() {
        let db = mock();

        // set
        db.set_state_value("test", "test").unwrap();
        db.set_state_value("int", 1).unwrap();
        db.set_state_value("float", 1.0).unwrap();
        db.set_state_value("bool", true).unwrap();
        db.set_state_value("null", ()).unwrap();
        db.set_state_value("array", vec![1, 2, 3]).unwrap();
        db.set_state_value("object", serde_json::json!({ "test": "test" }))
            .unwrap();
        db.set_state_value("binary", b"test".to_vec()).unwrap();

        // get
        assert_eq!(db.get_state_value("test").unwrap().unwrap(), "test");
        assert_eq!(db.get_state_value("int").unwrap().unwrap(), 1);
        assert_eq!(db.get_state_value("float").unwrap().unwrap(), 1.0);
        assert_eq!(db.get_state_value("bool").unwrap().unwrap(), true);
        assert_eq!(db.get_state_value("null").unwrap().unwrap(), serde_json::Value::Null);
        assert_eq!(
            db.get_state_value("array").unwrap().unwrap(),
            serde_json::json!([1, 2, 3])
        );
        assert_eq!(
            db.get_state_value("object").unwrap().unwrap(),
            serde_json::json!({ "test": "test" })
        );
        assert_eq!(
            db.get_state_value("binary").unwrap().unwrap(),
            serde_json::json!(b"test".to_vec())
        );

        // unset
        db.unset_state_value("test").unwrap();
        db.unset_state_value("int").unwrap();

        // is_set
        assert!(!db.is_state_value_set("test").unwrap());
        assert!(!db.is_state_value_set("int").unwrap());
        assert!(db.is_state_value_set("float").unwrap());
        assert!(db.is_state_value_set("bool").unwrap());
    }

    #[test]
    fn auth_table_tests() {
        let db = mock();

        db.set_auth_value("test", "test").unwrap();
        assert_eq!(db.get_auth_value("test").unwrap().unwrap(), "test");
        assert!(db.is_auth_value_set("test").unwrap());
        db.unset_auth_value("test").unwrap();
        assert!(!db.is_auth_value_set("test").unwrap());

        assert_eq!(db.get_auth_value("test2").unwrap(), None);
        assert!(!db.is_auth_value_set("test2").unwrap());
    }

    #[test]
    fn db_open_time() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("data.sqlite3");

        // init the db
        let db = Db::open(&path).unwrap();
        db.migrate().unwrap();
        drop(db);

        let test_count = 100;

        let instant = std::time::Instant::now();
        let db = Db::open(&path).unwrap();
        for _ in 0..test_count {
            db.set_state_value("test", "test").unwrap();
            db.get_state_value("test").unwrap().unwrap();
        }
        let elapsed = instant.elapsed() / test_count;
        println!("time: {:?}", elapsed);
    }

    #[test]
    fn test_atomic_bool() {
        let key = "test";
        let db = mock();

        let cases = [
            (None, false, false, false),
            (None, true, false, true),
            (Some(false), false, false, false),
            (Some(false), true, false, true),
            (Some(true), false, true, true),
            (Some(true), true, true, true),
        ];

        for (a, b, c, d) in cases {
            db.set_state_value(key, a).unwrap();
            assert_eq!(db.atomic_bool_or(key, b).unwrap(), c);
            assert_eq!(db.get_state_value(key).unwrap().unwrap(), d);
            db.unset_state_value(key).unwrap();
        }
    }
}
