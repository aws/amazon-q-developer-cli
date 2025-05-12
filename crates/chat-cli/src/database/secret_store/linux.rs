use super::Secret;
use super::sqlite::SqliteSecretStore;

pub struct SqliteSecretStore {
    db: &'static Database,
}

impl SqliteSecretStore {
    pub async fn new() -> Result<Self, DatabaseError> {
        Ok(Self { db: database()? })
    }

    pub async fn set(&self, key: &str, password: &str) -> Result<(), DatabaseError> {
        Ok(self.db.set_auth_value(key, password)?)
    }

    pub async fn get(&self, key: &str) -> Result<Option<Secret>, DatabaseError> {
        Ok(self.db.get_auth_entry(key)?.map(Secret))
    }

    pub async fn delete(&self, key: &str) -> Result<(), DatabaseError> {
        Ok(self.db.unset_auth_value(key)?)
    }
}

pub struct SecretStoreImpl {
    inner: SqliteSecretStore,
}

impl SecretStoreImpl {
    pub async fn new() -> Result<Self, DatabaseError> {
        Ok(Self {
            inner: SqliteSecretStore::new().await?,
        })
    }

    pub async fn set(&self, key: &str, password: &str) -> Result<(), DatabaseError> {
        self.inner.set(key, password).await
    }

    pub async fn get(&self, key: &str) -> Result<Option<Secret>, DatabaseError> {
        self.inner.get(key).await
    }

    pub async fn delete(&self, key: &str) -> Result<(), DatabaseError> {
        self.inner.delete(key).await
    }
}
