use super::Secret;
use super::sqlite::SqliteSecretStore;
use crate::Result;

pub struct SecretStoreImpl {
    inner: SqliteSecretStore,
}

impl SecretStoreImpl {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            inner: SqliteSecretStore::new().await?,
        })
    }

    pub async fn set(&self, key: &str, password: &str) -> Result<()> {
        self.inner.set(key, password).await
    }

    pub async fn get(&self, key: &str) -> Result<Option<Secret>> {
        self.inner.get(key).await
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        self.inner.delete(key).await
    }
}
