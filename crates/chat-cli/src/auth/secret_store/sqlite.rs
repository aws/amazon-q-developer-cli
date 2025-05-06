#![allow(dead_code)]
use super::Secret;
use crate::Result;
use crate::auth::AuthError;
use crate::fig_settings::sqlite::{
    Db,
    database,
};

pub struct SqliteSecretStore {
    db: &'static Db,
}

impl SqliteSecretStore {
    pub async fn new() -> Result<Self, AuthError> {
        Ok(Self { db: database()? })
    }

    pub async fn set(&self, key: &str, password: &str) -> Result<(), AuthError> {
        Ok(self.db.set_auth_value(key, password)?)
    }

    pub async fn get(&self, key: &str) -> Result<Option<Secret>, AuthError> {
        Ok(self.db.get_auth_value(key)?.map(Secret))
    }

    pub async fn delete(&self, key: &str) -> Result<(), AuthError> {
        Ok(self.db.unset_auth_value(key)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_get_delete() {
        let store = SqliteSecretStore::new().await.unwrap();
        let key = "test_key";
        let password = "test_password";

        store.set(key, password).await.unwrap();

        let secret = store.get(key).await.unwrap();
        assert_eq!(secret, Some(Secret(password.to_string())));

        store.delete(key).await.unwrap();
        let secret = store.get(key).await.unwrap();
        assert_eq!(secret, None);
    }
}
