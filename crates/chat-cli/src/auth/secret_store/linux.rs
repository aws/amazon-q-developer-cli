use super::Secret;
use super::sqlite::SqliteSecretStore;
use crate::Result;
use crate::auth::AuthError;

pub struct SecretStoreImpl {
    inner: SqliteSecretStore,
}

impl SecretStoreImpl {
    pub async fn new() -> Result<Self, AuthError> {
        Ok(Self {
            inner: SqliteSecretStore::new().await?,
        })
    }

    pub async fn set(&self, key: &str, password: &str) -> Result<(), AuthError> {
        self.inner.set(key, password).await
    }

    pub async fn get(&self, key: &str) -> Result<Option<Secret>, AuthError> {
        self.inner.get(key).await
    }

    pub async fn delete(&self, key: &str) -> Result<(), AuthError> {
        self.inner.delete(key).await
    }
}
