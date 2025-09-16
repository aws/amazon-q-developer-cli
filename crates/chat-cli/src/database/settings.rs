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

#[derive(Clone, Copy, Debug, strum::EnumIter, strum::EnumMessage)]
pub enum Setting {
    #[strum(message = "Enable/disable telemetry collection (boolean)")]
    TelemetryEnabled,
    #[strum(message = "Legacy client identifier for telemetry (string)")]
    OldClientId,
    #[strum(message = "Share content with CodeWhisperer service (boolean)")]
    ShareCodeWhispererContent,
    #[strum(message = "Enable thinking tool for complex reasoning (boolean)")]
    EnabledThinking,
    #[strum(message = "Enable knowledge base functionality (boolean)")]
    EnabledKnowledge,
    #[strum(message = "Default file patterns to include in knowledge base (array)")]
    KnowledgeDefaultIncludePatterns,
    #[strum(message = "Default file patterns to exclude from knowledge base (array)")]
    KnowledgeDefaultExcludePatterns,
    #[strum(message = "Maximum number of files for knowledge indexing (number)")]
    KnowledgeMaxFiles,
    #[strum(message = "Text chunk size for knowledge processing (number)")]
    KnowledgeChunkSize,
    #[strum(message = "Overlap between text chunks (number)")]
    KnowledgeChunkOverlap,
    #[strum(message = "Type of knowledge index to use (string)")]
    KnowledgeIndexType,
    #[strum(message = "Key binding for fuzzy search command (single character)")]
    SkimCommandKey,
    #[strum(message = "Enable tangent mode feature (boolean)")]
    EnabledTangentMode,
    #[strum(message = "Key binding for tangent mode toggle (single character)")]
    TangentModeKey,
    #[strum(message = "Auto-enter tangent mode for introspect questions (boolean)")]
    IntrospectTangentMode,
    #[strum(message = "Show greeting message on chat start (boolean)")]
    ChatGreetingEnabled,
    #[strum(message = "API request timeout in seconds (number)")]
    ApiTimeout,
    #[strum(message = "Enable edit mode for chat interface (boolean)")]
    ChatEditMode,
    #[strum(message = "Enable desktop notifications (boolean)")]
    ChatEnableNotifications,
    #[strum(message = "CodeWhisperer service endpoint URL (string)")]
    ApiCodeWhispererService,
    #[strum(message = "Q service endpoint URL (string)")]
    ApiQService,
    #[strum(message = "MCP server initialization timeout (number)")]
    McpInitTimeout,
    #[strum(message = "Non-interactive MCP timeout (number)")]
    McpNoInteractiveTimeout,
    #[strum(message = "Track previously loaded MCP servers (boolean)")]
    McpLoadedBefore,
    #[strum(message = "Default AI model for conversations (string)")]
    ChatDefaultModel,
    #[strum(message = "Disable markdown formatting in chat (boolean)")]
    ChatDisableMarkdownRendering,
    #[strum(message = "Default agent configuration (string)")]
    ChatDefaultAgent,
    #[strum(message = "Disable automatic conversation summarization (boolean)")]
    ChatDisableAutoCompaction,
    #[strum(message = "Show conversation history hints (boolean)")]
    ChatEnableHistoryHints,
    #[strum(message = "Enable the todo list feature (boolean)")]
    EnabledTodoList,
}

impl AsRef<str> for Setting {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::TelemetryEnabled => "telemetry.enabled",
            Self::OldClientId => "telemetryClientId",
            Self::ShareCodeWhispererContent => "codeWhisperer.shareCodeWhispererContentWithAWS",
            Self::EnabledThinking => "chat.enableThinking",
            Self::EnabledKnowledge => "chat.enableKnowledge",
            Self::KnowledgeDefaultIncludePatterns => "knowledge.defaultIncludePatterns",
            Self::KnowledgeDefaultExcludePatterns => "knowledge.defaultExcludePatterns",
            Self::KnowledgeMaxFiles => "knowledge.maxFiles",
            Self::KnowledgeChunkSize => "knowledge.chunkSize",
            Self::KnowledgeChunkOverlap => "knowledge.chunkOverlap",
            Self::KnowledgeIndexType => "knowledge.indexType",
            Self::SkimCommandKey => "chat.skimCommandKey",
            Self::EnabledTangentMode => "chat.enableTangentMode",
            Self::TangentModeKey => "chat.tangentModeKey",
            Self::IntrospectTangentMode => "introspect.tangentMode",
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
            Self::EnabledTodoList => "chat.enableTodoList",
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
            "knowledge.defaultIncludePatterns" => Ok(Self::KnowledgeDefaultIncludePatterns),
            "knowledge.defaultExcludePatterns" => Ok(Self::KnowledgeDefaultExcludePatterns),
            "knowledge.maxFiles" => Ok(Self::KnowledgeMaxFiles),
            "knowledge.chunkSize" => Ok(Self::KnowledgeChunkSize),
            "knowledge.chunkOverlap" => Ok(Self::KnowledgeChunkOverlap),
            "knowledge.indexType" => Ok(Self::KnowledgeIndexType),
            "chat.skimCommandKey" => Ok(Self::SkimCommandKey),
            "chat.enableTangentMode" => Ok(Self::EnabledTangentMode),
            "chat.tangentModeKey" => Ok(Self::TangentModeKey),
            "introspect.tangentMode" => Ok(Self::IntrospectTangentMode),
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
            "chat.enableTodoList" => Ok(Self::EnabledTodoList),
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

    pub fn get_int_or(&self, key: Setting, default: usize) -> usize {
        self.get_int(key).map_or(default, |v| v as usize)
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
        assert_eq!(settings.get(Setting::KnowledgeIndexType), None);
        assert_eq!(settings.get(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get(Setting::ChatDefaultModel), None);
        assert_eq!(settings.get(Setting::ChatDisableMarkdownRendering), None);

        settings.set(Setting::TelemetryEnabled, true).await.unwrap();
        settings.set(Setting::OldClientId, "test").await.unwrap();
        settings.set(Setting::ShareCodeWhispererContent, false).await.unwrap();
        settings.set(Setting::KnowledgeIndexType, "fast").await.unwrap();
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
        assert_eq!(
            settings.get(Setting::KnowledgeIndexType),
            Some(&Value::String("fast".to_string()))
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
        settings.remove(Setting::KnowledgeIndexType).await.unwrap();
        settings.remove(Setting::McpLoadedBefore).await.unwrap();
        settings.remove(Setting::ChatDisableMarkdownRendering).await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get(Setting::OldClientId), None);
        assert_eq!(settings.get(Setting::ShareCodeWhispererContent), None);
        assert_eq!(settings.get(Setting::KnowledgeIndexType), None);
        assert_eq!(settings.get(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get(Setting::ChatDisableMarkdownRendering), None);
    }
}
