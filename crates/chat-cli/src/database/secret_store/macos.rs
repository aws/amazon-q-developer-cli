use super::Secret;
use crate::database::DatabaseError;

/// Path to the `security` binary
const SECURITY_BIN: &str = "/usr/bin/security";

/// The account name is not used.
const ACCOUNT: &str = "";

pub struct SecretStoreImpl {
    _private: (),
}

impl SecretStoreImpl {
    pub async fn new() -> Result<Self, DatabaseError> {
        Ok(Self { _private: () })
    }

    /// Sets the `key` to `password` on the keychain, this will override any existing value
    pub async fn set(&self, key: &str, password: &str) -> Result<(), DatabaseError> {
        let output = tokio::process::Command::new(SECURITY_BIN)
            .args(["add-generic-password", "-U", "-s", key, "-a", ACCOUNT, "-w", password])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = std::str::from_utf8(&output.stderr)?;
            return Err(DatabaseError::Security(stderr.into()));
        }

        Ok(())
    }

    /// Returns the password for the `key`
    ///
    /// If not found the result will be `Ok(None)`, other errors will be returned
    pub async fn get(&self, key: &str) -> Result<Option<Secret>, DatabaseError> {
        let output = tokio::process::Command::new(SECURITY_BIN)
            .args(["find-generic-password", "-s", key, "-a", ACCOUNT, "-w"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = std::str::from_utf8(&output.stderr)?;
            if stderr.contains("could not be found") {
                return Ok(None);
            } else {
                return Err(DatabaseError::Security(stderr.into()));
            }
        }

        let stdout = std::str::from_utf8(&output.stdout)?;

        // strip newline
        let stdout = match stdout.strip_suffix('\n') {
            Some(stdout) => stdout,
            None => stdout,
        };

        Ok(Some(stdout.into()))
    }

    /// Deletes the `key` from the keychain
    pub async fn delete(&self, key: &str) -> Result<(), DatabaseError> {
        let output = tokio::process::Command::new(SECURITY_BIN)
            .args(["delete-generic-password", "-s", key, "-a", ACCOUNT])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = std::str::from_utf8(&output.stderr)?;
            return Err(DatabaseError::Security(stderr.into()));
        }

        Ok(())
    }
}
