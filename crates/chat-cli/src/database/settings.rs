use std::fmt::Display;
use std::io::SeekFrom;

use fd_lock::RwLock as FileRwLock;
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde_json::{
    Map,
    Value,
};
use tokio::fs::File;
use tokio::io::{
    AsyncReadExt,
    AsyncSeekExt,
    AsyncWriteExt,
};

use super::DatabaseError;

#[derive(Clone, Copy, Debug)]
pub enum Setting {
    TelemetryEnabled,
    OldClientId,
    ShareCodeWhispererContent,
    EnabledThinking,
    SkimCommandKey,
    ChatGreetingEnabled,
    ApiTimeout,
    ChatEditMode,
    ChatEnableNotifications,
}

impl AsRef<str> for Setting {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::TelemetryEnabled => "telemetry.enabled",
            Self::OldClientId => "telemetryClientId",
            Self::ShareCodeWhispererContent => "codeWhisperer.shareCodeWhispererContentWithAWS",
            Self::EnabledThinking => "chat.enableThinking",
            Self::SkimCommandKey => "chat.skimCommandKey",
            Self::ChatGreetingEnabled => "chat.greeting.enabled",
            Self::ApiTimeout => "api.timeout",
            Self::ChatEditMode => "chat.editMode",
            Self::ChatEnableNotifications => "chat.enableNotifications",
        }
    }
}

impl Into<String> for Setting {
    fn into(self) -> String {
        self.as_ref().to_string()
    }
}

impl Display for Setting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl TryFrom<&str> for Setting {
    type Error = DatabaseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "telemetry.enabled" => Ok(Self::TelemetryEnabled),
            "telemetryClientId" => Ok(Self::OldClientId),
            "codeWhisperer.shareCodeWhispererContentWithAWS" => Ok(Self::ShareCodeWhispererContent),
            "chat.enableThinking" => Ok(Self::EnabledThinking),
            "chat.skimCommandKey" => Ok(Self::SkimCommandKey),
            "chat.greeting.enabled" => Ok(Self::ChatGreetingEnabled),
            "api.timeout" => Ok(Self::ApiTimeout),
            "chat.editMode" => Ok(Self::ChatEditMode),
            "chat.enableNotifications" => Ok(Self::ChatEnableNotifications),
            _ => Err(DatabaseError::InvalidSetting(value.to_string())),
        }
    }
}

static SETTINGS_FILE_LOCK: RwLock<()> = RwLock::new(());

#[derive(Debug, Clone, Default)]
pub struct Settings(Map<String, Value>);

impl Settings {
    pub async fn new() -> Result<Self, DatabaseError> {
        if cfg!(test) {
            return Ok(Self::default());
        }

        let path = crate::util::directories::settings_path()?;

        // If the folder doesn't exist, create it.
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let _lock_guard = SETTINGS_FILE_LOCK.write();

        Ok(Self(match path.exists() {
            true => {
                let mut file = FileRwLock::new(File::open(&path).await?);
                let mut buf = Vec::new();
                file.write()?.read_to_end(&mut buf).await?;
                serde_json::from_slice(&buf)?
            },
            false => {
                let mut file = FileRwLock::new(File::create(path).await?);
                file.write()?.write_all(b"{}").await?;
                serde_json::Map::new()
            },
        }))
    }

    pub fn map(&self) -> &'_ Map<String, Value> {
        &self.0
    }

    pub fn get(&self, key: Setting) -> Option<&Value> {
        self.0.get(key.as_ref())
    }

    pub fn get_mut(&mut self, key: Setting) -> Option<&mut Value> {
        self.0.get_mut(key.as_ref())
    }

    pub async fn set(&mut self, key: Setting, value: impl Into<serde_json::Value>) -> Result<(), DatabaseError> {
        self.0.insert(key.into(), value.into());
        self.save_to_file().await
    }

    pub async fn remove(&mut self, key: Setting) -> Result<Option<Value>, DatabaseError> {
        let key = self.0.remove(key.as_ref());
        self.save_to_file().await?;
        Ok(key)
    }

    pub fn get_bool(&self, key: Setting) -> Option<bool> {
        self.get(key).map(|value| value.as_bool()).flatten()
    }

    pub fn get_string(&self, key: Setting) -> Option<String> {
        self.get(key).map(|value| value.as_str().map(|s| s.into())).flatten()
    }

    pub fn get_int(&self, key: Setting) -> Option<i64> {
        self.get(key).and_then(|value| value.as_i64())
    }

    pub fn get_as<T: DeserializeOwned>(&self, key: Setting) -> Result<Option<T>, DatabaseError> {
        match self.get(key) {
            Some(value) => Ok(serde_json::from_value(value.clone())?),
            None => Ok(None),
        }
    }

    async fn save_to_file(&self) -> Result<(), DatabaseError> {
        if cfg!(test) {
            return Ok(());
        }

        let path = crate::util::directories::settings_path()?;

        // If the folder doesn't exist, create it.
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let _lock_guard = SETTINGS_FILE_LOCK.write();

        let mut file_opts = File::options();
        file_opts.create(true).write(true).truncate(true);

        #[cfg(unix)]
        file_opts.mode(0o600);
        let mut file = FileRwLock::new(file_opts.open(&path).await?);
        let mut lock = file.write()?;

        match serde_json::to_string_pretty(&self.0) {
            Ok(json) => lock.write_all(json.as_bytes()).await?,
            Err(_err) => {
                lock.seek(SeekFrom::Start(0)).await?;
                lock.set_len(0).await?;
                lock.write_all(b"{}").await?;
            },
        }
        lock.flush().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// General read/write settings test
    #[tokio::test]
    async fn test_settings() {
        let mut settings = Settings::new().await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get(Setting::OldClientId), None);
        assert_eq!(settings.get(Setting::ShareCodeWhispererContent), None);

        settings.set(Setting::TelemetryEnabled, true).await.unwrap();
        settings.set(Setting::OldClientId, "test").await.unwrap();
        settings.set(Setting::ShareCodeWhispererContent, false).await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), Some(&Value::Bool(true)));
        assert_eq!(
            settings.get(Setting::OldClientId),
            Some(&Value::String("test".to_string()))
        );
        assert_eq!(
            settings.get(Setting::ShareCodeWhispererContent),
            Some(&Value::Bool(false))
        );

        settings.remove(Setting::TelemetryEnabled).await.unwrap();
        settings.remove(Setting::OldClientId).await.unwrap();
        settings.remove(Setting::ShareCodeWhispererContent).await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get(Setting::OldClientId), None);
        assert_eq!(settings.get(Setting::ShareCodeWhispererContent), None);
    }
}
