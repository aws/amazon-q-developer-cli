use std::fmt::Display;
use std::io::SeekFrom;

use fd_lock::RwLock;
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
    EnabledKnowledge,
    SkimCommandKey,
    ChatGreetingEnabled,
    ApiTimeout,
    ChatEditMode,
    ChatEnableNotifications,
    ApiCodeWhispererService,
    ApiQService,
    McpInitTimeout,
    McpNoInteractiveTimeout,
    McpLoadedBefore,
    ChatDefaultModel,
    ChatDisableMarkdownRendering,
    ChatDefaultAgent,
    ChatDisableAutoCompaction,
    ChatEnableHistoryHints,
}

impl AsRef<str> for Setting {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::TelemetryEnabled => "telemetry.enabled",
            Self::OldClientId => "telemetryClientId",
            Self::ShareCodeWhispererContent => "codeWhisperer.shareCodeWhispererContentWithAWS",
            Self::EnabledThinking => "chat.enableThinking",
            Self::EnabledKnowledge => "chat.enableKnowledge",
            Self::SkimCommandKey => "chat.skimCommandKey",
            Self::ChatGreetingEnabled => "chat.greeting.enabled",
            Self::ApiTimeout => "api.timeout",
            Self::ChatEditMode => "chat.editMode",
            Self::ChatEnableNotifications => "chat.enableNotifications",
            Self::ApiCodeWhispererService => "api.codewhisperer.service",
            Self::ApiQService => "api.q.service",
            Self::McpInitTimeout => "mcp.initTimeout",
            Self::McpNoInteractiveTimeout => "mcp.noInteractiveTimeout",
            Self::McpLoadedBefore => "mcp.loadedBefore",
            Self::ChatDefaultModel => "chat.defaultModel",
            Self::ChatDisableMarkdownRendering => "chat.disableMarkdownRendering",
            Self::ChatDefaultAgent => "chat.defaultAgent",
            Self::ChatDisableAutoCompaction => "chat.disableAutoCompaction",
            Self::ChatEnableHistoryHints => "chat.enableHistoryHints",
        }
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
            "chat.enableKnowledge" => Ok(Self::EnabledKnowledge),
            "chat.skimCommandKey" => Ok(Self::SkimCommandKey),
            "chat.greeting.enabled" => Ok(Self::ChatGreetingEnabled),
            "api.timeout" => Ok(Self::ApiTimeout),
            "chat.editMode" => Ok(Self::ChatEditMode),
            "chat.enableNotifications" => Ok(Self::ChatEnableNotifications),
            "api.codewhisperer.service" => Ok(Self::ApiCodeWhispererService),
            "api.q.service" => Ok(Self::ApiQService),
            "mcp.initTimeout" => Ok(Self::McpInitTimeout),
            "mcp.noInteractiveTimeout" => Ok(Self::McpNoInteractiveTimeout),
            "mcp.loadedBefore" => Ok(Self::McpLoadedBefore),
            "chat.defaultModel" => Ok(Self::ChatDefaultModel),
            "chat.disableMarkdownRendering" => Ok(Self::ChatDisableMarkdownRendering),
            "chat.defaultAgent" => Ok(Self::ChatDefaultAgent),
            "chat.disableAutoCompaction" => Ok(Self::ChatDisableAutoCompaction),
            "chat.enableHistoryHints" => Ok(Self::ChatEnableHistoryHints),
            _ => Err(DatabaseError::InvalidSetting(value.to_string())),
        }
    }
}

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

        Ok(Self(match path.exists() {
            true => {
                let mut file = RwLock::new(File::open(&path).await?);
                let mut buf = Vec::new();
                file.write()?.read_to_end(&mut buf).await?;
                serde_json::from_slice(&buf)?
            },
            false => {
                let mut file = RwLock::new(File::create(path).await?);
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

    pub async fn set(&mut self, key: Setting, value: impl Into<serde_json::Value>) -> Result<(), DatabaseError> {
        self.0.insert(key.to_string(), value.into());
        self.save_to_file().await
    }

    pub async fn remove(&mut self, key: Setting) -> Result<Option<Value>, DatabaseError> {
        let key = self.0.remove(key.as_ref());
        self.save_to_file().await?;
        Ok(key)
    }

    pub fn get_bool(&self, key: Setting) -> Option<bool> {
        self.get(key).and_then(|value| value.as_bool())
    }

    pub fn get_string(&self, key: Setting) -> Option<String> {
        self.get(key).and_then(|value| value.as_str().map(|s| s.into()))
    }

    pub fn get_int(&self, key: Setting) -> Option<i64> {
        self.get(key).and_then(|value| value.as_i64())
    }

    pub async fn save_to_file(&self) -> Result<(), DatabaseError> {
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

        let mut file_opts = File::options();
        file_opts.create(true).write(true).truncate(true);

        #[cfg(unix)]
        file_opts.mode(0o600);
        let mut file = RwLock::new(file_opts.open(&path).await?);
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
        assert_eq!(settings.get(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get(Setting::ChatDefaultModel), None);
        assert_eq!(settings.get(Setting::ChatDisableMarkdownRendering), None);

        settings.set(Setting::TelemetryEnabled, true).await.unwrap();
        settings.set(Setting::OldClientId, "test").await.unwrap();
        settings.set(Setting::ShareCodeWhispererContent, false).await.unwrap();
        settings.set(Setting::McpLoadedBefore, true).await.unwrap();
        settings.set(Setting::ChatDefaultModel, "model 1").await.unwrap();
        settings
            .set(Setting::ChatDisableMarkdownRendering, false)
            .await
            .unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), Some(&Value::Bool(true)));
        assert_eq!(
            settings.get(Setting::OldClientId),
            Some(&Value::String("test".to_string()))
        );
        assert_eq!(
            settings.get(Setting::ShareCodeWhispererContent),
            Some(&Value::Bool(false))
        );
        assert_eq!(settings.get(Setting::McpLoadedBefore), Some(&Value::Bool(true)));
        assert_eq!(
            settings.get(Setting::ChatDefaultModel),
            Some(&Value::String("model 1".to_string()))
        );
        assert_eq!(
            settings.get(Setting::ChatDisableMarkdownRendering),
            Some(&Value::Bool(false))
        );

        settings.remove(Setting::TelemetryEnabled).await.unwrap();
        settings.remove(Setting::OldClientId).await.unwrap();
        settings.remove(Setting::ShareCodeWhispererContent).await.unwrap();
        settings.remove(Setting::McpLoadedBefore).await.unwrap();
        settings.remove(Setting::ChatDisableMarkdownRendering).await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get(Setting::OldClientId), None);
        assert_eq!(settings.get(Setting::ShareCodeWhispererContent), None);
        assert_eq!(settings.get(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get(Setting::ChatDisableMarkdownRendering), None);
    }
}
