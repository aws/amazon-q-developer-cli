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
use crate::os::{
    Env,
    Fs,
};
use crate::util::paths::{
    GlobalPaths,
    PathResolver,
};

#[derive(Clone, Copy, Debug, strum::EnumIter, strum::EnumMessage, strum::EnumProperty)]
pub enum Setting {
    #[strum(
        message = "Enable/disable telemetry collection (boolean)",
        props(scope = "global_only")
    )]
    TelemetryEnabled,
    #[strum(
        message = "Legacy client identifier for telemetry (string)",
        props(scope = "global_only")
    )]
    OldClientId,
    #[strum(
        message = "Share content with CodeWhisperer service (boolean)",
        props(scope = "global_only")
    )]
    ShareCodeWhispererContent,
    #[strum(message = "Enable thinking tool for complex reasoning (boolean)")]
    EnabledThinking,
    #[strum(message = "Enable knowledge base functionality (boolean)")]
    EnabledKnowledge,
    #[strum(message = "Enable code intelligence with LSP integration (boolean)")]
    EnabledCodeIntelligence,
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
    #[strum(message = "Key binding for autocompletion hint acceptance (single character)")]
    AutocompletionKey,
    #[strum(message = "Enable tangent mode feature (boolean)")]
    EnabledTangentMode,
    #[strum(message = "Key binding for tangent mode toggle (single character)")]
    TangentModeKey,
    #[strum(message = "Enable subagent feature (boolean)")]
    EnabledSubagent,
    #[strum(message = "Key binding for delegate command (single character)")]
    DelegateModeKey,

    #[strum(message = "Auto-enter tangent mode for introspect questions (boolean)")]
    IntrospectTangentMode,
    #[strum(message = "Use progressive loading instead of semantic search for introspect (boolean)")]
    IntrospectProgressiveMode,
    #[strum(message = "Show greeting message on chat start (boolean)")]
    ChatGreetingEnabled,
    #[strum(message = "API request timeout in seconds (number)")]
    ApiTimeout,
    #[strum(message = "Enable edit mode for chat interface (boolean)")]
    ChatEditMode,
    #[strum(message = "Enable desktop notifications (boolean)")]
    ChatEnableNotifications,
    #[strum(
        message = "CodeWhisperer service endpoint URL (string)",
        props(scope = "global_only")
    )]
    ApiOidcScopePrefix,
    #[strum(message = "CodeWhisperer OIDC scope prefix (string)", props(scope = "global_only"))]
    ApiCodeWhispererService,
    #[strum(message = "Q service endpoint URL (string)", props(scope = "global_only"))]
    ApiQService,
    #[strum(message = "MCP server initialization timeout (number)")]
    ApiKiroAuthService,
    #[strum(message = "Kiro auth service endpoint", props(scope = "global_only"))]
    McpInitTimeout,
    #[strum(message = "Non-interactive MCP timeout (number)")]
    McpNoInteractiveTimeout,
    #[strum(
        message = "Track previously loaded MCP servers (boolean)",
        props(scope = "global_only")
    )]
    McpLoadedBefore,
    #[strum(message = "Show context usage percentage in prompt (boolean)")]
    EnabledContextUsageIndicator,
    #[strum(message = "Default AI model for conversations (string)")]
    ChatDefaultModel,
    #[strum(message = "Disable markdown formatting in chat (boolean)")]
    ChatDisableMarkdownRendering,
    #[strum(message = "Default agent configuration (string)")]
    ChatDefaultAgent,
    #[strum(message = "Disable automatic conversation summarization (boolean)")]
    ChatDisableAutoCompaction,
    #[strum(message = "Percentage of context window to exclude from compaction ([0, 100])")]
    CompactionExcludeContextWindowPercent,
    #[strum(message = "Minimum message pairs to exclude from compaction (number)")]
    CompactionExcludeMessages,
    #[strum(message = "Show conversation history hints (boolean)")]
    ChatEnableHistoryHints,
    #[strum(message = "Show rotating prompt hints on empty input (boolean)")]
    ChatEnablePromptHints,
    #[strum(message = "Enable the todo list feature (boolean)")]
    EnabledTodoList,
    #[strum(message = "Enable the checkpoint feature (boolean)")]
    EnabledCheckpoint,
    #[strum(message = "Enable the delegate tool for subagent management (boolean)")]
    EnabledDelegate,
    #[strum(message = "Specify UI variant to use (string)")]
    UiMode,
    #[strum(message = "External diff tool command (string)")]
    ChatDiffTool,
}

impl Setting {
    pub fn is_workspace_overridable(&self) -> bool {
        use strum::EnumProperty;
        self.get_str("scope") != Some("global_only")
    }
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
            Self::AutocompletionKey => "chat.autocompletionKey",
            Self::EnabledTangentMode => "chat.enableTangentMode",
            Self::TangentModeKey => "chat.tangentModeKey",
            Self::EnabledSubagent => "chat.enableSubagent",
            Self::DelegateModeKey => "chat.delegateModeKey",

            Self::IntrospectTangentMode => "introspect.tangentMode",
            Self::IntrospectProgressiveMode => "introspect.progressiveMode",
            Self::ChatGreetingEnabled => "chat.greeting.enabled",
            Self::ApiTimeout => "api.timeout",
            Self::ChatEditMode => "chat.editMode",
            Self::ChatEnableNotifications => "chat.enableNotifications",
            Self::ApiCodeWhispererService => "api.codewhisperer.service",
            Self::ApiOidcScopePrefix => "api.oidc.scopePrefix",
            Self::ApiQService => "api.q.service",
            Self::ApiKiroAuthService => "api.kiroauth.service",
            Self::McpInitTimeout => "mcp.initTimeout",
            Self::McpNoInteractiveTimeout => "mcp.noInteractiveTimeout",
            Self::McpLoadedBefore => "mcp.loadedBefore",
            Self::ChatDefaultModel => "chat.defaultModel",
            Self::ChatDisableMarkdownRendering => "chat.disableMarkdownRendering",
            Self::ChatDefaultAgent => "chat.defaultAgent",
            Self::ChatDisableAutoCompaction => "chat.disableAutoCompaction",
            Self::CompactionExcludeContextWindowPercent => "compaction.excludeContextWindowPercent",
            Self::CompactionExcludeMessages => "compaction.excludeMessages",
            Self::ChatEnableHistoryHints => "chat.enableHistoryHints",
            Self::ChatEnablePromptHints => "chat.enablePromptHints",
            Self::EnabledTodoList => "chat.enableTodoList",
            Self::EnabledCheckpoint => "chat.enableCheckpoint",
            Self::EnabledContextUsageIndicator => "chat.enableContextUsageIndicator",
            Self::EnabledDelegate => "chat.enableDelegate",
            Self::EnabledCodeIntelligence => "chat.enableCodeIntelligence",
            Self::UiMode => "chat.uiMode",
            Self::ChatDiffTool => "chat.diffTool",
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
            "chat.autocompletionKey" => Ok(Self::AutocompletionKey),
            "chat.enableTangentMode" => Ok(Self::EnabledTangentMode),
            "chat.tangentModeKey" => Ok(Self::TangentModeKey),
            "chat.enableSubagent" => Ok(Self::EnabledSubagent),

            "introspect.tangentMode" => Ok(Self::IntrospectTangentMode),
            "introspect.progressiveMode" => Ok(Self::IntrospectProgressiveMode),
            "chat.greeting.enabled" => Ok(Self::ChatGreetingEnabled),
            "api.timeout" => Ok(Self::ApiTimeout),
            "chat.editMode" => Ok(Self::ChatEditMode),
            "chat.enableNotifications" => Ok(Self::ChatEnableNotifications),
            "api.codewhisperer.service" => Ok(Self::ApiCodeWhispererService),
            "api.oidc.scopePrefix" => Ok(Self::ApiOidcScopePrefix),
            "api.q.service" => Ok(Self::ApiQService),
            "api.kiroauth.service" => Ok(Self::ApiKiroAuthService),
            "mcp.initTimeout" => Ok(Self::McpInitTimeout),
            "mcp.noInteractiveTimeout" => Ok(Self::McpNoInteractiveTimeout),
            "mcp.loadedBefore" => Ok(Self::McpLoadedBefore),
            "chat.defaultModel" => Ok(Self::ChatDefaultModel),
            "chat.disableMarkdownRendering" => Ok(Self::ChatDisableMarkdownRendering),
            "chat.defaultAgent" => Ok(Self::ChatDefaultAgent),
            "chat.disableAutoCompaction" => Ok(Self::ChatDisableAutoCompaction),
            "compaction.excludeContextWindowPercent" => Ok(Self::CompactionExcludeContextWindowPercent),
            "compaction.excludeMessages" => Ok(Self::CompactionExcludeMessages),
            "chat.enableHistoryHints" => Ok(Self::ChatEnableHistoryHints),
            "chat.enablePromptHints" => Ok(Self::ChatEnablePromptHints),
            "chat.enableTodoList" => Ok(Self::EnabledTodoList),
            "chat.enableCheckpoint" => Ok(Self::EnabledCheckpoint),
            "chat.enableContextUsageIndicator" => Ok(Self::EnabledContextUsageIndicator),
            "chat.enableCodeIntelligence" => Ok(Self::EnabledCodeIntelligence),
            "chat.uiMode" => Ok(Self::UiMode),
            "chat.diffTool" => Ok(Self::ChatDiffTool),
            _ => Err(DatabaseError::InvalidSetting(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Settings {
    global: Map<String, Value>,
    workspace: Option<Map<String, Value>>,
    workspace_settings_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingScope {
    Global,
    Workspace,
}

impl Settings {
    pub async fn new(env: &Env, fs: &Fs) -> Result<Self, DatabaseError> {
        if cfg!(test) {
            return Ok(Self::default());
        }

        let global_path = GlobalPaths::settings_path()?;
        let global = Self::load_settings_file(&global_path).await?;

        let resolver = PathResolver::new(env, fs);
        let workspace_settings_path = resolver.workspace().settings_path().ok();

        let workspace = if workspace_settings_path.is_some() {
            if let Some(ref ws_path) = workspace_settings_path {
                if ws_path.exists() {
                    Some(Self::load_settings_file(ws_path).await?)
                } else {
                    Some(Map::new())
                }
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            global,
            workspace,
            workspace_settings_path,
        })
    }

    async fn load_settings_file(path: &std::path::PathBuf) -> Result<Map<String, Value>, DatabaseError> {
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }

        Ok(match path.exists() {
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
        })
    }

    pub fn map(&self) -> Map<String, Value> {
        let mut merged = self.global.clone();
        if let Some(workspace) = &self.workspace {
            for (key, value) in workspace {
                if let Ok(setting) = Setting::try_from(key.as_str())
                    && setting.is_workspace_overridable()
                {
                    merged.insert(key.clone(), value.clone());
                }
            }
        }
        merged
    }

    pub fn get(&self, key: Setting) -> Option<&Value> {
        if key.is_workspace_overridable()
            && let Some(workspace) = &self.workspace
            && let Some(value) = workspace.get(key.as_ref())
        {
            return Some(value);
        }
        self.global.get(key.as_ref())
    }

    pub fn get_scope(&self, key: Setting) -> Option<SettingScope> {
        if key.is_workspace_overridable()
            && let Some(workspace) = &self.workspace
            && workspace.contains_key(key.as_ref())
        {
            return Some(SettingScope::Workspace);
        }
        if self.global.contains_key(key.as_ref()) {
            Some(SettingScope::Global)
        } else {
            None
        }
    }

    pub async fn set(
        &mut self,
        key: Setting,
        value: impl Into<serde_json::Value>,
        scope: Option<SettingScope>,
    ) -> Result<(), DatabaseError> {
        let scope = scope.unwrap_or(SettingScope::Global);

        match scope {
            SettingScope::Global => {
                self.global.insert(key.to_string(), value.into());
                self.save_global().await
            },
            SettingScope::Workspace => {
                if !key.is_workspace_overridable() {
                    return Err(DatabaseError::WorkspaceOverrideNotAllowed(key.to_string()));
                }
                self.workspace.as_mut().unwrap().insert(key.to_string(), value.into());
                self.save_workspace().await
            },
        }
    }

    pub async fn remove(&mut self, key: Setting, scope: Option<SettingScope>) -> Result<Option<Value>, DatabaseError> {
        let scope = scope.unwrap_or(SettingScope::Global);

        let removed = match scope {
            SettingScope::Global => self.global.remove(key.as_ref()),
            SettingScope::Workspace => self.workspace.as_mut().and_then(|ws| ws.remove(key.as_ref())),
        };

        match scope {
            SettingScope::Global => self.save_global().await?,
            SettingScope::Workspace => self.save_workspace().await?,
        }

        Ok(removed)
    }

    async fn save_global(&self) -> Result<(), DatabaseError> {
        let path = GlobalPaths::settings_path()?;
        Self::save_settings_file(&path, &self.global).await
    }

    async fn save_workspace(&self) -> Result<(), DatabaseError> {
        if let Some(path) = &self.workspace_settings_path
            && let Some(workspace) = &self.workspace
        {
            Self::save_settings_file(path, workspace).await?;
        }
        Ok(())
    }

    async fn save_settings_file(path: &std::path::PathBuf, map: &Map<String, Value>) -> Result<(), DatabaseError> {
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut file_opts = File::options();
        file_opts.create(true).write(true).truncate(true);

        #[cfg(unix)]
        file_opts.mode(0o600);
        let mut file = RwLock::new(file_opts.open(&path).await?);
        let mut lock = file.write()?;

        match serde_json::to_string_pretty(map) {
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
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_workspace_overridable() {
        assert!(!Setting::TelemetryEnabled.is_workspace_overridable());
        assert!(!Setting::ApiCodeWhispererService.is_workspace_overridable());
        assert!(Setting::ChatDefaultModel.is_workspace_overridable());
        assert!(Setting::EnabledTangentMode.is_workspace_overridable());
    }

    #[test]
    fn test_workspace_override() {
        let mut settings = Settings::default();
        settings
            .global
            .insert("chat.defaultModel".to_string(), Value::String("model1".to_string()));
        settings.workspace = Some(serde_json::Map::new());
        settings
            .workspace
            .as_mut()
            .unwrap()
            .insert("chat.defaultModel".to_string(), Value::String("model2".to_string()));

        assert_eq!(
            settings.get(Setting::ChatDefaultModel),
            Some(&Value::String("model2".to_string()))
        );
        assert_eq!(
            settings.get_scope(Setting::ChatDefaultModel),
            Some(SettingScope::Workspace)
        );
    }

    #[test]
    fn test_global_fallback() {
        let mut settings = Settings::default();
        settings
            .global
            .insert("chat.defaultModel".to_string(), Value::String("model1".to_string()));

        assert_eq!(
            settings.get(Setting::ChatDefaultModel),
            Some(&Value::String("model1".to_string()))
        );
        assert_eq!(
            settings.get_scope(Setting::ChatDefaultModel),
            Some(SettingScope::Global)
        );
    }

    #[test]
    fn test_no_value_set() {
        let settings = Settings::default();
        assert_eq!(settings.get(Setting::ChatDefaultModel), None);
        assert_eq!(settings.get_scope(Setting::ChatDefaultModel), None);
    }

    #[test]
    fn test_global_only_not_overridable() {
        let mut settings = Settings::default();
        settings
            .global
            .insert("telemetry.enabled".to_string(), Value::Bool(true));
        settings.workspace = Some(serde_json::Map::new());
        settings
            .workspace
            .as_mut()
            .unwrap()
            .insert("telemetry.enabled".to_string(), Value::Bool(false));

        // Should return global value, not workspace
        assert_eq!(settings.get(Setting::TelemetryEnabled), Some(&Value::Bool(true)));
        assert_eq!(
            settings.get_scope(Setting::TelemetryEnabled),
            Some(SettingScope::Global)
        );
    }

    #[tokio::test]
    async fn test_set_and_get_global() {
        let mut settings = Settings::default();

        settings.set(Setting::TelemetryEnabled, true, None).await.unwrap();
        settings.set(Setting::ChatDefaultModel, "model1", None).await.unwrap();
        settings.set(Setting::EnabledTangentMode, false, None).await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), Some(&Value::Bool(true)));
        assert_eq!(
            settings.get(Setting::ChatDefaultModel),
            Some(&Value::String("model1".to_string()))
        );
        assert_eq!(settings.get(Setting::EnabledTangentMode), Some(&Value::Bool(false)));
    }

    #[tokio::test]
    async fn test_remove_global() {
        let mut settings = Settings::default();
        settings
            .global
            .insert("chat.defaultModel".to_string(), Value::String("model1".to_string()));

        let removed = settings.remove(Setting::ChatDefaultModel, None).await.unwrap();
        assert_eq!(removed, Some(Value::String("model1".to_string())));
        assert_eq!(settings.get(Setting::ChatDefaultModel), None);
    }

    #[tokio::test]
    async fn test_set_workspace() {
        let mut settings = Settings::default();
        settings.workspace = Some(serde_json::Map::new());
        settings.workspace_settings_path = Some(std::path::PathBuf::from("/tmp/test.json"));

        settings
            .set(
                Setting::ChatDefaultModel,
                "workspace-model",
                Some(SettingScope::Workspace),
            )
            .await
            .unwrap();

        assert_eq!(
            settings.get(Setting::ChatDefaultModel),
            Some(&Value::String("workspace-model".to_string()))
        );
        assert_eq!(
            settings.get_scope(Setting::ChatDefaultModel),
            Some(SettingScope::Workspace)
        );
    }

    #[tokio::test]
    async fn test_remove_workspace() {
        let mut settings = Settings::default();
        settings.workspace = Some(serde_json::Map::new());
        settings.workspace_settings_path = Some(std::path::PathBuf::from("/tmp/test.json"));
        settings
            .workspace
            .as_mut()
            .unwrap()
            .insert("chat.defaultModel".to_string(), Value::String("model".to_string()));

        let removed = settings
            .remove(Setting::ChatDefaultModel, Some(SettingScope::Workspace))
            .await
            .unwrap();
        assert_eq!(removed, Some(Value::String("model".to_string())));
        assert_eq!(settings.get(Setting::ChatDefaultModel), None);
    }

    #[tokio::test]
    async fn test_workspace_override_rejected() {
        let mut settings = Settings::default();
        settings.workspace = Some(serde_json::Map::new());
        settings.workspace_settings_path = Some(std::path::PathBuf::from("/tmp/test.json"));

        let result = settings
            .set(Setting::TelemetryEnabled, true, Some(SettingScope::Workspace))
            .await;

        assert!(matches!(result, Err(DatabaseError::WorkspaceOverrideNotAllowed(_))));
    }

    #[test]
    fn test_get_bool() {
        let mut settings = Settings::default();
        settings
            .global
            .insert("chat.enableTangentMode".to_string(), Value::Bool(true));
        assert_eq!(settings.get_bool(Setting::EnabledTangentMode), Some(true));
    }

    #[test]
    fn test_get_string() {
        let mut settings = Settings::default();
        settings
            .global
            .insert("chat.defaultModel".to_string(), Value::String("sonnet".to_string()));
        assert_eq!(
            settings.get_string(Setting::ChatDefaultModel),
            Some("sonnet".to_string())
        );
    }

    #[test]
    fn test_get_int() {
        let mut settings = Settings::default();
        settings
            .global
            .insert("api.timeout".to_string(), Value::Number(30.into()));
        assert_eq!(settings.get_int(Setting::ApiTimeout), Some(30));
    }

    #[test]
    fn test_get_int_or() {
        let settings = Settings::default();
        assert_eq!(settings.get_int_or(Setting::ApiTimeout, 60), 60);

        let mut settings = Settings::default();
        settings
            .global
            .insert("api.timeout".to_string(), Value::Number(30.into()));
        assert_eq!(settings.get_int_or(Setting::ApiTimeout, 60), 30);
    }
}
