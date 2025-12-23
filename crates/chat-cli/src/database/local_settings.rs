use std::path::Path;

use serde_json::{Map, Value};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use super::{DatabaseError, settings::Setting};

/// Local settings for a specific workspace/project directory
#[derive(Debug, Clone, Default)]
pub struct LocalSettings(Map<String, Value>);

impl LocalSettings {
    /// Create a new LocalSettings instance by reading from the workspace directory
    pub async fn new(workspace_dir: &Path) -> Result<Self, DatabaseError> {
        let settings_path = workspace_dir.join(".amazonq").join("settings.json");

        Ok(Self(match settings_path.exists() {
            true => {
                let mut file = File::open(&settings_path).await?;
                let mut buf = Vec::new();
                file.read_to_end(&mut buf).await?;
                serde_json::from_slice(&buf)?
            },
            false => Map::new(),
        }))
    }

    /// Get a setting value
    pub fn get(&self, key: Setting) -> Option<&Value> {
        self.0.get(key.as_ref())
    }

    /// Get a string setting value
    pub fn get_string(&self, key: Setting) -> Option<String> {
        self.get(key).and_then(|value| value.as_str().map(|s| s.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_settings_new_empty() {
        let temp_dir = TempDir::new().unwrap();
        let settings = LocalSettings::new(temp_dir.path()).await.unwrap();
        assert!(settings.0.is_empty());
    }

    #[tokio::test]
    async fn test_local_settings_get_string() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create a settings file manually
        let amazonq_dir = temp_dir.path().join(".amazonq");
        tokio::fs::create_dir_all(&amazonq_dir).await.unwrap();
        let settings_path = amazonq_dir.join("settings.json");
        tokio::fs::write(&settings_path, r#"{"chat.defaultAgent": "test_agent"}"#).await.unwrap();

        let settings = LocalSettings::new(temp_dir.path()).await.unwrap();
        assert_eq!(
            settings.get_string(Setting::ChatDefaultAgent),
            Some("test_agent".to_string())
        );
    }
}
