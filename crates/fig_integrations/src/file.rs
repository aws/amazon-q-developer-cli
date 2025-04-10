use std::io::ErrorKind;
use std::path::PathBuf;

use async_trait::async_trait;
use tokio::fs::{
    self,
    File,
};
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::Integration;
use crate::error::{
    Error,
    ErrorExt,
    Result,
};

#[derive(Debug, Clone)]
pub struct FileIntegration {
    pub path: PathBuf,
    pub contents: String,
    #[cfg(unix)]
    pub mode: Option<u32>,
}

#[async_trait]
impl Integration for FileIntegration {
    fn describe(&self) -> String {
        format!("File Integration @ {}", self.path.to_string_lossy())
    }

    async fn is_installed(&self) -> Result<()> {
        // Check for parent folder permissions issues
        #[cfg(unix)]
        {
            use nix::unistd::{
                AccessFlags,
                access,
            };

            let mut path = self.path.as_path();
            let mut res = Ok(());
            loop {
                if let Some(parent) = path.parent() {
                    match access(parent, AccessFlags::R_OK | AccessFlags::W_OK | AccessFlags::X_OK) {
                        Ok(_) => {
                            break;
                        },
                        Err(err) => {
                            res = Err(Error::PermissionDenied {
                                path: parent.into(),
                                inner: err.into(),
                            });
                            path = parent;
                        },
                    }
                }
            }
            res?;
        }

        let current_contents = match fs::read_to_string(&self.path).await.with_path(&self.path) {
            Ok(contents) => contents,
            Err(Error::Io(err)) if err.kind() == ErrorKind::NotFound => {
                return Err(Error::FileDoesNotExist(self.path.clone().into()));
            },
            Err(err) => return Err(err),
        };
        if current_contents.ne(&self.contents) {
            let message = format!("{} should contain:\n{}", self.path.display(), self.contents);
            return Err(Error::ImproperInstallation(message.into()));
        }
        Ok(())
    }

    async fn install(&self) -> Result<()> {
        if self.is_installed().await.is_ok() {
            return Ok(());
        }

        let parent_dir = self
            .path
            .parent()
            .ok_or_else(|| Error::Custom("Could not get integration file directory".into()))?;

        if !parent_dir.is_dir() {
            fs::create_dir_all(&parent_dir).await.with_path(parent_dir)?;
        }

        let mut options = File::options();
        options.write(true).create(true).truncate(true);

        #[cfg(unix)]
        if let Some(mode) = self.mode {
            options.mode(mode);
        }

        let mut file = options.open(&self.path).await.with_path(&self.path)?;

        debug!(path =? self.path, "Writing file integrations");
        file.write_all(self.contents.as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        match fs::remove_file(&self.path).await.with_path(&self.path) {
            Ok(_) => Ok(()),
            Err(Error::Io(err)) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_integration() {
        let tempdir = tempfile::tempdir().unwrap();
        let integration = FileIntegration {
            path: tempdir.path().join("integration.txt"),
            contents: "test".into(),
            // weird mode for testing
            #[cfg(unix)]
            mode: None,
        };

        assert_eq!(
            format!("File Integration @ {}/integration.txt", tempdir.path().display()),
            integration.describe()
        );

        // ensure no intgration is marked as not installed
        assert!(matches!(
            integration.is_installed().await,
            Err(Error::FileDoesNotExist(_))
        ));

        // ensure the intgration can be installed
        integration.install().await.unwrap();
        assert!(integration.is_installed().await.is_ok());

        // ensure the intgration can be installed while already installed
        integration.install().await.unwrap();
        assert!(integration.is_installed().await.is_ok());

        // ensure the intgration can be uninstalled
        integration.uninstall().await.unwrap();
        assert!(matches!(
            integration.is_installed().await,
            Err(Error::FileDoesNotExist(_))
        ));

        // write bad data to integration file
        fs::write(&integration.path, "bad data").await.unwrap();
        assert!(matches!(
            integration.is_installed().await,
            Err(Error::ImproperInstallation(_))
        ));

        // fix integration file
        integration.install().await.unwrap();
        assert!(integration.is_installed().await.is_ok());
    }
}
