use std::fmt::Display;

use serde_json::{
    Map,
    Value,
};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use super::DatabaseError;
use crate::util::paths::GlobalPaths;

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
    #[strum(message = "Notification method: 'auto', 'bel', 'osc9' (string)")]
    ChatNotificationMethod,
    #[strum(
        message = "CodeWhisperer service endpoint URL (string)",
        props(scope = "global_only")
    )]
    ApiCodeWhispererService,
    #[strum(message = "KRS endpoint override (JSON object)", props(scope = "global_only"))]
    ApiKrsService,
    #[strum(message = "CPS endpoint override (JSON object)", props(scope = "global_only"))]
    ApiCpsService,
    #[strum(message = "OIDC scope prefix (string)", props(scope = "global_only"))]
    ApiOidcScopePrefix,
    #[strum(message = "Q service endpoint URL (string)", props(scope = "global_only"))]
    ApiQService,
    #[strum(message = "Kiro auth service endpoint (string)", props(scope = "global_only"))]
    ApiKiroAuthService,
    #[strum(message = "MCP server initialization timeout (number)")]
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
    #[strum(message = "Show hook execution status messages (boolean, default: true)")]
    HooksShowStatus,
    #[strum(message = "Chat UI mode: 'legacy' or 'tui' (string)")]
    ChatUi,
    #[strum(
        message = "Days after which old conversations and data are deleted (number)",
        props(scope = "global_only")
    )]
    CleanupPeriodDays,
    #[strum(message = "Disable granular trust options for tool permissions (boolean)")]
    ChatDisableGranularTrust,
    #[strum(
        message = "Disable automatic updates on startup (boolean)",
        props(scope = "global_only")
    )]
    DisableAutoupdates,
    #[cfg(feature = "voice")]
    #[strum(message = "Voice server URL for remote transcription (string)")]
    VoiceServerUrl,
    #[cfg(feature = "voice")]
    #[strum(message = "Whisper model size for voice transcription (string)")]
    VoiceModelSize,
    #[cfg(feature = "voice")]
    #[strum(message = "Voice language for transcription (string)")]
    VoiceLanguage,
    #[cfg(feature = "voice")]
    #[strum(message = "Silence timeout in seconds for voice recording (number, default: 5)")]
    VoiceSilenceTimeout,
    #[cfg(feature = "voice")]
    #[strum(message = "Pause duration in ms to trigger partial transcription (number, default: 500)")]
    VoicePartialPause,
    #[cfg(feature = "voice")]
    #[strum(message = "Maximum session time in seconds for voice recording (number)")]
    VoiceMaxSessionTime,
    #[cfg(feature = "voice")]
    #[strum(message = "Auto-submit voice transcription without review (boolean, default: true)")]
    VoiceAutoSubmit,
    #[strum(message = "Always show full tool output inline without truncation (boolean)")]
    ChatAutoExpandToolOutput,
    #[strum(
        message = "Skip the trust-all-tools confirmation gate on startup (boolean)",
        props(scope = "global_only")
    )]
    ChatDisableTrustAllConfirmation,
    #[strum(message = "Enable tool search for MCP tool discovery (boolean)")]
    ToolSearchEnabled,
    #[strum(
        message = "Minimum context window percentage of MCP tool specs to activate tool search (number, e.g. 5 for 5%)"
    )]
    ToolSearchMinPct,
    #[strum(message = "Minimum MCP tool spec token count to activate tool search (number)")]
    ToolSearchMinTokens,
    #[strum(
        message = "Disable line wrapping in chat output; long lines soft-wrap visually but remain single logical lines for copy-paste (boolean)"
    )]
    ChatDisableWrap,
    #[strum(message = "Per-model additional field defaults (object of model ID → overrides)")]
    ChatModelDefaults,
    #[strum(
        message = "Enable animated spinners and progress indicators (boolean)",
        props(scope = "global_only")
    )]
    ChatAllowAnimations,
    #[strum(
        message = "Enable Unicode/braille symbols and decorative art (boolean)",
        props(scope = "global_only")
    )]
    ChatAllowAsciiArt,
    #[strum(message = "Show status indicator icons (boolean)", props(scope = "global_only"))]
    ChatAllowIcons,
    #[strum(
        message = "Whether the braille logo has been shown on first launch (boolean)",
        props(scope = "global_only")
    )]
    ChatHasSeenLogo,
    #[strum(message = "Show thinking/reasoning blocks in chat output (boolean, default: false; startup-only)")]
    ChatShowThinking,
    #[strum(
        message = "Update terminal window title with session info (boolean)",
        props(scope = "global_only")
    )]
    ChatTerminalTitle,
    #[strum(message = "Default follow-up delivery mode for new chat sessions: 'steer' or 'queue' (string)")]
    ChatDefaultInterruptBehavior,
    #[strum(message = "Key binding to toggle follow-up delivery mode (string, default: ctrl+s)")]
    ChatKeybindingsToggleInterruptBehavior,
    #[strum(
        message = "Disable inheriting default resources — global/workspace steering, skills, and project marker files like AGENTS.md — in custom (user-defined) agents (boolean, default: false)"
    )]
    ChatDisableInheritingDefaultResources,
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
            Self::ChatNotificationMethod => "chat.notificationMethod",
            Self::ApiCodeWhispererService => "api.codewhisperer.service",
            Self::ApiKrsService => "api.krs.service",
            Self::ApiCpsService => "api.cps.service",
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
            Self::HooksShowStatus => "hooks.showStatus",
            Self::ChatUi => "chat.ui",
            Self::CleanupPeriodDays => "cleanup.periodDays",
            Self::ChatDisableGranularTrust => "chat.disableGranularTrust",
            Self::DisableAutoupdates => "app.disableAutoupdates",
            Self::ChatAutoExpandToolOutput => "chat.autoExpandToolOutput",
            Self::ChatDisableTrustAllConfirmation => "chat.disableTrustAllConfirmation",
            Self::ToolSearchEnabled => "toolSearch.enabled",
            Self::ToolSearchMinPct => "toolSearch.minPct",
            Self::ToolSearchMinTokens => "toolSearch.minTokens",
            Self::ChatDisableWrap => "chat.disableWrap",
            Self::ChatModelDefaults => "chat.modelDefaults",
            Self::ChatAllowAnimations => "chat.allowAnimations",
            Self::ChatAllowAsciiArt => "chat.allowAsciiArt",
            Self::ChatAllowIcons => "chat.allowIcons",
            Self::ChatHasSeenLogo => "chat.hasSeenLogo",
            Self::ChatShowThinking => "chat.showThinking",
            Self::ChatTerminalTitle => "chat.terminalTitle",
            Self::ChatDefaultInterruptBehavior => "chat.defaultInterruptBehavior",
            Self::ChatKeybindingsToggleInterruptBehavior => "chat.keybindings.toggleInterruptBehavior",
            Self::ChatDisableInheritingDefaultResources => "chat.disableInheritingDefaultResources",
            #[cfg(feature = "voice")]
            Self::VoiceServerUrl => "voice.serverUrl",
            #[cfg(feature = "voice")]
            Self::VoiceModelSize => "voice.modelSize",
            #[cfg(feature = "voice")]
            Self::VoiceLanguage => "voice.language",
            #[cfg(feature = "voice")]
            Self::VoiceSilenceTimeout => "voice.silenceTimeout",
            #[cfg(feature = "voice")]
            Self::VoicePartialPause => "voice.partialPause",
            #[cfg(feature = "voice")]
            Self::VoiceMaxSessionTime => "voice.maxSessionTime",
            #[cfg(feature = "voice")]
            Self::VoiceAutoSubmit => "voice.autoSubmit",
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
            "chat.delegateModeKey" => Ok(Self::DelegateModeKey),

            "introspect.tangentMode" => Ok(Self::IntrospectTangentMode),
            "introspect.progressiveMode" => Ok(Self::IntrospectProgressiveMode),
            "chat.greeting.enabled" => Ok(Self::ChatGreetingEnabled),
            "api.timeout" => Ok(Self::ApiTimeout),
            "chat.editMode" => Ok(Self::ChatEditMode),
            "chat.enableNotifications" => Ok(Self::ChatEnableNotifications),
            "chat.notificationMethod" => Ok(Self::ChatNotificationMethod),
            "api.codewhisperer.service" => Ok(Self::ApiCodeWhispererService),
            "api.krs.service" => Ok(Self::ApiKrsService),
            "api.cps.service" => Ok(Self::ApiCpsService),
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
            "chat.enableDelegate" => Ok(Self::EnabledDelegate),
            "chat.enableCodeIntelligence" => Ok(Self::EnabledCodeIntelligence),
            // Frontend persists this key in `cli.json` using the dotted
            // `chat.ui.mode` form and mirrors it to the backend via
            // `kiro.setSetting(...)`. The canonical key here is camelCase
            // `chat.uiMode`; accept the dotted form as an alias so the
            // mirror write doesn't fail with InvalidSetting.
            "chat.uiMode" | "chat.ui.mode" => Ok(Self::UiMode),
            "chat.diffTool" => Ok(Self::ChatDiffTool),
            "hooks.showStatus" => Ok(Self::HooksShowStatus),
            "chat.ui" => Ok(Self::ChatUi),
            "cleanup.periodDays" => Ok(Self::CleanupPeriodDays),
            "chat.disableGranularTrust" => Ok(Self::ChatDisableGranularTrust),
            "app.disableAutoupdates" => Ok(Self::DisableAutoupdates),
            "chat.autoExpandToolOutput" => Ok(Self::ChatAutoExpandToolOutput),
            "chat.disableTrustAllConfirmation" => Ok(Self::ChatDisableTrustAllConfirmation),
            "toolSearch.enabled" => Ok(Self::ToolSearchEnabled),
            "toolSearch.minPct" => Ok(Self::ToolSearchMinPct),
            "toolSearch.minTokens" => Ok(Self::ToolSearchMinTokens),
            "chat.disableWrap" => Ok(Self::ChatDisableWrap),
            "chat.modelDefaults" => Ok(Self::ChatModelDefaults),
            "chat.allowAnimations" => Ok(Self::ChatAllowAnimations),
            "chat.allowAsciiArt" => Ok(Self::ChatAllowAsciiArt),
            "chat.allowIcons" => Ok(Self::ChatAllowIcons),
            "chat.hasSeenLogo" => Ok(Self::ChatHasSeenLogo),
            "chat.showThinking" => Ok(Self::ChatShowThinking),
            "chat.terminalTitle" => Ok(Self::ChatTerminalTitle),
            "chat.defaultInterruptBehavior" => Ok(Self::ChatDefaultInterruptBehavior),
            "chat.keybindings.toggleInterruptBehavior" => Ok(Self::ChatKeybindingsToggleInterruptBehavior),
            "chat.disableInheritingDefaultResources" => Ok(Self::ChatDisableInheritingDefaultResources),
            #[cfg(feature = "voice")]
            "voice.serverUrl" => Ok(Self::VoiceServerUrl),
            #[cfg(feature = "voice")]
            "voice.modelSize" => Ok(Self::VoiceModelSize),
            #[cfg(feature = "voice")]
            "voice.language" => Ok(Self::VoiceLanguage),
            #[cfg(feature = "voice")]
            "voice.silenceTimeout" => Ok(Self::VoiceSilenceTimeout),
            #[cfg(feature = "voice")]
            "voice.partialPause" => Ok(Self::VoicePartialPause),
            #[cfg(feature = "voice")]
            "voice.maxSessionTime" => Ok(Self::VoiceMaxSessionTime),
            #[cfg(feature = "voice")]
            "voice.autoSubmit" => Ok(Self::VoiceAutoSubmit),
            _ => Err(DatabaseError::InvalidSetting(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingScope {
    Global,
    Workspace,
    Session,
}

#[derive(Debug, Clone, Default)]
pub struct Settings {
    global: Map<String, Value>,
    workspace: Option<Map<String, Value>>,
    workspace_settings_path: Option<std::path::PathBuf>,
    /// Session-level overrides (not persisted, cleared when chat exits)
    session: Map<String, Value>,
}

impl Settings {
    /// Load global settings only (used by Database::new and other callers without Os)
    pub async fn new() -> Result<Self, DatabaseError> {
        if cfg!(test) {
            return Ok(Self::default());
        }

        let path = GlobalPaths::settings_path()?;
        let global = Self::load_settings_file(&path).await?;

        Ok(Self {
            global,
            workspace: None,
            workspace_settings_path: None,
            session: Map::new(),
        })
    }

    /// Load global + workspace settings
    pub async fn with_workspace(workspace_settings_path: Option<std::path::PathBuf>) -> Result<Self, DatabaseError> {
        if cfg!(test) {
            return Ok(Self::default());
        }

        let global_path = GlobalPaths::settings_path()?;
        let global = Self::load_settings_file(&global_path).await?;

        let workspace = if let Some(ref ws_path) = workspace_settings_path {
            if ws_path.exists() {
                Some(Self::load_settings_file(ws_path).await?)
            } else {
                Some(Map::new())
            }
        } else {
            None
        };

        Ok(Self {
            global,
            workspace,
            workspace_settings_path,
            session: Map::new(),
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
                let buf = tokio::fs::read(&path).await?;
                serde_json::from_slice(&buf)
                    .map_err(|e| DatabaseError::JsonParseWithPath(format!("failed to parse {}: {e}", path.display())))?
            },
            false => {
                let mut file_opts = File::options();
                file_opts.create(true).write(true).truncate(true);
                #[cfg(unix)]
                file_opts.mode(0o600);
                let mut file = file_opts.open(path).await?;
                file.write_all(b"{}").await?;
                file.flush().await?;
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
        if let Some(value) = self.session.get(key.as_ref()) {
            return Some(value);
        }
        if key.is_workspace_overridable()
            && let Some(workspace) = &self.workspace
            && let Some(value) = workspace.get(key.as_ref())
        {
            return Some(value);
        }
        self.global.get(key.as_ref())
    }

    pub fn get_scope(&self, key: Setting) -> Option<SettingScope> {
        if self.session.contains_key(key.as_ref()) {
            return Some(SettingScope::Session);
        }
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
                match self.workspace.as_mut() {
                    Some(ws) => {
                        ws.insert(key.to_string(), value.into());
                    },
                    None => {
                        return Err(DatabaseError::WorkspaceOverrideNotAllowed(
                            "no workspace settings loaded".to_string(),
                        ));
                    },
                }
                self.save_workspace().await
            },
            SettingScope::Session => {
                self.session.insert(key.to_string(), value.into());
                Ok(())
            },
        }
    }

    pub async fn remove(&mut self, key: Setting, scope: Option<SettingScope>) -> Result<Option<Value>, DatabaseError> {
        let scope = scope.unwrap_or(SettingScope::Global);
        let removed = match scope {
            SettingScope::Global => self.global.remove(key.as_ref()),
            SettingScope::Workspace => self.workspace.as_mut().and_then(|ws| ws.remove(key.as_ref())),
            SettingScope::Session => self.session.remove(key.as_ref()),
        };
        match scope {
            SettingScope::Global => self.save_global().await?,
            SettingScope::Workspace => self.save_workspace().await?,
            SettingScope::Session => {},
        }
        Ok(removed)
    }

    pub fn clear_session(&mut self) {
        self.session.clear();
    }

    /// Update a single key in the global settings file.
    ///
    /// This performs a read → merge → atomic-write cycle directly on disk,
    /// independent of any in-memory `Settings` snapshot. It is designed to
    /// be called from the ACP handler where the `Os` (and therefore the
    /// `Settings` struct) is a clone and mutations to the in-memory map
    /// would not propagate back to the original.
    ///
    /// Concurrency: the write itself is atomic (temp file + rename), so
    /// readers always observe a complete file. The read-modify-write is
    /// not atomic across writers — if another writer (another process, or
    /// another path in this process) renames between this function's read
    /// and rename, that update is lost. Last writer wins.
    pub async fn update_global_setting(key: Setting, value: Value) -> Result<(), DatabaseError> {
        let path = GlobalPaths::settings_path()?;

        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Read current contents.
        let mut map: Map<String, Value> = if path.exists() {
            let buf = tokio::fs::read(&path).await?;
            if buf.is_empty() {
                Map::new()
            } else {
                serde_json::from_slice(&buf).unwrap_or_default()
            }
        } else {
            Map::new()
        };

        // Merge the new value.
        map.insert(key.to_string(), value);

        // Atomic write via temp file + rename.
        Self::save_settings_file(&path, &map).await?;

        Ok(())
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

        let json = serde_json::to_string_pretty(map).unwrap_or_else(|_| "{}".to_string());

        // Write to a temp file then atomically rename to avoid truncating the
        // original if the process is interrupted mid-write.
        let tmp_path = path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));

        let result: Result<(), DatabaseError> = async {
            let mut file_opts = File::options();
            file_opts.create(true).write(true).truncate(true);
            #[cfg(unix)]
            file_opts.mode(0o600);

            let mut file = file_opts.open(&tmp_path).await?;
            file.write_all(json.as_bytes()).await?;
            // Flush userspace buffers, then fsync data so the bytes hit the
            // disk (or NFS server) before the rename. Without sync_data, a
            // crash between write and rename can leave the renamed file
            // empty even though rename(2) is atomic at the directory level.
            file.flush().await?;
            file.sync_data().await?;
            drop(file);

            tokio::fs::rename(&tmp_path, path).await?;
            Ok(())
        }
        .await;

        if result.is_err() {
            // Best-effort cleanup of the orphaned temp file. Ignore errors
            // here (e.g. NotFound if open() failed) — we want to surface the
            // original error, not mask it.
            let _ = tokio::fs::remove_file(&tmp_path).await;
        }

        result
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
    use strum::IntoEnumIterator;

    use super::*;

    /// Verify save_settings_file writes correctly and leaves no temp file.
    #[tokio::test]
    async fn test_save_settings_file_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");

        let mut map = Map::new();
        map.insert("chat.defaultModel".to_string(), Value::String("claude".to_string()));
        map.insert("telemetry.enabled".to_string(), Value::Bool(true));
        Settings::save_settings_file(&path, &map).await.unwrap();

        let saved: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(saved.get("chat.defaultModel").unwrap(), "claude");
        assert_eq!(saved.get("telemetry.enabled").unwrap(), true);

        // Overwrite
        map.insert("chat.defaultModel".to_string(), Value::String("sonnet".to_string()));
        Settings::save_settings_file(&path, &map).await.unwrap();

        let saved: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(saved.get("chat.defaultModel").unwrap(), "sonnet");

        // No leftover temp file
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(entries.is_empty(), "temp file should be cleaned up after rename");
    }

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

        settings.set(Setting::TelemetryEnabled, true, None).await.unwrap();
        settings.set(Setting::OldClientId, "test", None).await.unwrap();
        settings
            .set(Setting::ShareCodeWhispererContent, false, None)
            .await
            .unwrap();
        settings.set(Setting::KnowledgeIndexType, "fast", None).await.unwrap();
        settings.set(Setting::McpLoadedBefore, true, None).await.unwrap();
        settings.set(Setting::ChatDefaultModel, "model 1", None).await.unwrap();
        settings.set(Setting::ChatDiffTool, "diff tool", None).await.unwrap();
        settings
            .set(Setting::ChatDisableMarkdownRendering, false, None)
            .await
            .unwrap();
        settings.set(Setting::EnabledCheckpoint, true, None).await.unwrap();

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
            settings.get(Setting::ChatDiffTool),
            Some(&Value::String("diff tool".to_string()))
        );
        assert_eq!(
            settings.get(Setting::ChatDisableMarkdownRendering),
            Some(&Value::Bool(false))
        );
        assert_eq!(settings.get(Setting::EnabledCheckpoint), Some(&Value::Bool(true)));

        settings.remove(Setting::TelemetryEnabled, None).await.unwrap();
        settings.remove(Setting::OldClientId, None).await.unwrap();
        settings.remove(Setting::ShareCodeWhispererContent, None).await.unwrap();
        settings.remove(Setting::KnowledgeIndexType, None).await.unwrap();
        settings.remove(Setting::McpLoadedBefore, None).await.unwrap();
        settings
            .remove(Setting::ChatDisableMarkdownRendering, None)
            .await
            .unwrap();
        settings.remove(Setting::EnabledCheckpoint, None).await.unwrap();

        assert_eq!(settings.get(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get(Setting::OldClientId), None);
        assert_eq!(settings.get(Setting::ShareCodeWhispererContent), None);
        assert_eq!(settings.get(Setting::KnowledgeIndexType), None);
        assert_eq!(settings.get(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get(Setting::ChatDisableMarkdownRendering), None);
        assert_eq!(settings.get(Setting::EnabledCheckpoint), None);
    }

    #[test]
    fn test_auto_expand_tool_output_setting_key() {
        // Verify the setting key roundtrips through AsRef and TryFrom
        let key = Setting::ChatAutoExpandToolOutput.as_ref();
        assert_eq!(key, "chat.autoExpandToolOutput");
        let parsed = Setting::try_from(key).unwrap();
        assert!(matches!(parsed, Setting::ChatAutoExpandToolOutput));
    }

    #[test]
    fn test_interrupt_behavior_setting_keys() {
        // The TUI dual-mode feature reads these keys; the CLI `settings set`
        // path must accept them, so they have to roundtrip through AsRef and
        // TryFrom. Regression guard for the keys being absent from the enum.
        let default_key = Setting::ChatDefaultInterruptBehavior.as_ref();
        assert_eq!(default_key, "chat.defaultInterruptBehavior");
        assert!(matches!(
            Setting::try_from(default_key).unwrap(),
            Setting::ChatDefaultInterruptBehavior
        ));

        let toggle_key = Setting::ChatKeybindingsToggleInterruptBehavior.as_ref();
        assert_eq!(toggle_key, "chat.keybindings.toggleInterruptBehavior");
        assert!(matches!(
            Setting::try_from(toggle_key).unwrap(),
            Setting::ChatKeybindingsToggleInterruptBehavior
        ));
    }

    #[test]
    fn test_all_settings_roundtrip() {
        // Every Setting variant must parse back from its string key. Catches
        // missing or mismatched entries between AsRef and TryFrom.
        for setting in Setting::iter() {
            let key = setting.as_ref();
            let parsed =
                Setting::try_from(key).unwrap_or_else(|_| panic!("setting key `{key}` is not parseable via TryFrom"));
            assert_eq!(
                parsed.as_ref(),
                key,
                "setting key `{key}` did not roundtrip through TryFrom",
            );
        }
    }

    #[test]
    fn test_auto_expand_tool_output_is_workspace_overridable() {
        // UI settings should be overridable per workspace
        assert!(Setting::ChatAutoExpandToolOutput.is_workspace_overridable());
    }

    #[tokio::test]
    async fn test_auto_expand_tool_output_read_write() {
        let mut settings = Settings::new().await.unwrap();

        // Default: not set
        assert_eq!(settings.get(Setting::ChatAutoExpandToolOutput), None);

        // Set to true
        settings
            .set(Setting::ChatAutoExpandToolOutput, true, None)
            .await
            .unwrap();
        assert_eq!(
            settings.get(Setting::ChatAutoExpandToolOutput),
            Some(&Value::Bool(true))
        );

        // Set to false
        settings
            .set(Setting::ChatAutoExpandToolOutput, false, None)
            .await
            .unwrap();
        assert_eq!(
            settings.get(Setting::ChatAutoExpandToolOutput),
            Some(&Value::Bool(false))
        );

        // Remove
        settings.remove(Setting::ChatAutoExpandToolOutput, None).await.unwrap();
        assert_eq!(settings.get(Setting::ChatAutoExpandToolOutput), None);
    }

    #[test]
    fn test_ui_mode_accepts_dotted_alias() {
        // Frontend writes `chat.ui.mode`; canonical form is `chat.uiMode`.
        let canonical = Setting::try_from("chat.uiMode").unwrap();
        let aliased = Setting::try_from("chat.ui.mode").unwrap();
        assert!(matches!(canonical, Setting::UiMode));
        assert!(matches!(aliased, Setting::UiMode));
    }
}
