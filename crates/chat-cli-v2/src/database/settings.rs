use std::fmt::Display;
use std::sync::{
    Arc,
    RwLock,
};
use std::time::Duration;

use serde_json::{
    Map,
    Value,
};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use super::DatabaseError;
use crate::util::file_lock::with_file_lock_at;
use crate::util::paths::GlobalPaths;

/// Bound on waiting for the settings file lock; generous because holders only
/// perform one small mutate + write cycle.
const SETTINGS_FILE_LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// Recursively merge `patch` into `base`. Objects are merged key-by-key; all
/// other types are replaced.
pub(crate) fn deep_merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (k, v) in patch_map {
                deep_merge(base_map.entry(k).or_insert(Value::Null), v);
            }
        },
        (base, patch) => *base = patch,
    }
}

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
    #[strum(message = "Specify UI variant to use (string)", props(scope = "global_only"))]
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
    #[strum(
        message = "Repaint only the viewport instead of clearing terminal scrollback on full redraws, on every surface; where the left status bar spans the repaint boundary it shows a gap at the seam (boolean, default: false)",
        props(scope = "global_only")
    )]
    ChatPreserveScrollback,
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
    #[strum(message = "Show a feature tip below the thinking indicator while waiting (boolean, default: true)")]
    ChatShowThinkingTips,
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
    #[strum(message = "Disable auto-migration prompt for V2 agent configs on V3 engine launch (boolean)")]
    ChatDisableAutoAgentUpgrade,
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
            Self::ChatPreserveScrollback => "chat.preserveScrollback",
            Self::ChatModelDefaults => "chat.modelDefaults",
            Self::ChatAllowAnimations => "chat.allowAnimations",
            Self::ChatAllowAsciiArt => "chat.allowAsciiArt",
            Self::ChatAllowIcons => "chat.allowIcons",
            Self::ChatHasSeenLogo => "chat.hasSeenLogo",
            Self::ChatShowThinking => "chat.showThinking",
            Self::ChatShowThinkingTips => "chat.showThinkingTips",
            Self::ChatTerminalTitle => "chat.terminalTitle",
            Self::ChatDefaultInterruptBehavior => "chat.defaultInterruptBehavior",
            Self::ChatKeybindingsToggleInterruptBehavior => "chat.keybindings.toggleInterruptBehavior",
            Self::ChatDisableInheritingDefaultResources => "chat.disableInheritingDefaultResources",
            Self::ChatDisableAutoAgentUpgrade => "chat.disableAutoAgentUpgrade",
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
            "chat.preserveScrollback" => Ok(Self::ChatPreserveScrollback),
            "chat.modelDefaults" => Ok(Self::ChatModelDefaults),
            "chat.allowAnimations" => Ok(Self::ChatAllowAnimations),
            "chat.allowAsciiArt" => Ok(Self::ChatAllowAsciiArt),
            "chat.allowIcons" => Ok(Self::ChatAllowIcons),
            "chat.hasSeenLogo" => Ok(Self::ChatHasSeenLogo),
            "chat.showThinking" => Ok(Self::ChatShowThinking),
            "chat.showThinkingTips" => Ok(Self::ChatShowThinkingTips),
            "chat.terminalTitle" => Ok(Self::ChatTerminalTitle),
            "chat.defaultInterruptBehavior" => Ok(Self::ChatDefaultInterruptBehavior),
            "chat.keybindings.toggleInterruptBehavior" => Ok(Self::ChatKeybindingsToggleInterruptBehavior),
            "chat.disableInheritingDefaultResources" => Ok(Self::ChatDisableInheritingDefaultResources),
            "chat.disableAutoAgentUpgrade" => Ok(Self::ChatDisableAutoAgentUpgrade),
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
    /// In-memory-only override for the current process, never persisted. Dead in
    /// V2 today (no V2 caller writes session scope); retained to keep this module
    /// aligned with V1's `settings.rs` ahead of the planned v1/v2 single-crate merge.
    Session,
}

/// Which settings file a locked write persists to. Session scope never touches
/// disk, so it has no target here.
#[derive(Debug, Clone, Copy)]
enum PersistTarget {
    Global,
    Workspace,
}

/// Process-wide settings store.
///
/// `Settings` is a cheap cloneable handle around a single shared lock. Cloning an
/// `Os` (hence its `Database`) shares the same underlying store, so a write
/// through any handle is immediately visible to every other holder in-process.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    inner: Arc<RwLock<SettingsData>>,
}

#[derive(Debug, Default)]
struct SettingsData {
    global: Map<String, Value>,
    /// Disk target for global settings, resolved once at construction. `None`
    /// (test default) makes global writes in-memory only.
    global_settings_path: Option<std::path::PathBuf>,
    workspace: Option<Map<String, Value>>,
    workspace_settings_path: Option<std::path::PathBuf>,
    /// In-memory-only session overrides, never persisted. Dead in V2 (no V2
    /// caller sets session scope); kept for parity with V1 ahead of the v1/v2
    /// single-crate merge.
    session: Map<String, Value>,
}

impl SettingsData {
    fn get(&self, key: Setting) -> Option<&Value> {
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

    fn get_scope(&self, key: Setting) -> Option<SettingScope> {
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

    fn map(&self) -> Map<String, Value> {
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
}

impl Settings {
    fn from_data(data: SettingsData) -> Self {
        Self {
            inner: Arc::new(RwLock::new(data)),
        }
    }

    /// Load global settings only (used by Database::new and other callers without Os)
    pub async fn new() -> Result<Self, DatabaseError> {
        if cfg!(test) {
            return Ok(Self::default());
        }

        let path = GlobalPaths::settings_path()?;
        let global = Self::load_settings_file(&path).await?;

        Ok(Self::from_data(SettingsData {
            global,
            global_settings_path: Some(path),
            workspace: None,
            workspace_settings_path: None,
            session: Map::new(),
        }))
    }

    /// Construct a store persisting global settings to an explicit path.
    #[cfg(test)]
    fn test_with_global_path(path: std::path::PathBuf) -> Self {
        Self::from_data(SettingsData {
            global_settings_path: Some(path),
            ..Default::default()
        })
    }

    /// Construct a store with a loaded (empty) workspace persisting to an
    /// explicit path.
    #[cfg(test)]
    fn test_with_workspace_path(path: std::path::PathBuf) -> Self {
        Self::from_data(SettingsData {
            workspace: Some(Map::new()),
            workspace_settings_path: Some(path),
            ..Default::default()
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

        Ok(Self::from_data(SettingsData {
            global,
            global_settings_path: Some(global_path),
            workspace,
            workspace_settings_path,
            session: Map::new(),
        }))
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

    fn with_read<R>(&self, f: impl FnOnce(&SettingsData) -> R) -> R {
        f(&self.inner.read().expect("settings lock poisoned"))
    }

    /// Merged view of global + applicable workspace overrides.
    pub fn map(&self) -> Map<String, Value> {
        self.with_read(SettingsData::map)
    }

    /// Look up a setting (honoring session/workspace precedence) as an owned clone.
    pub fn get_value(&self, key: Setting) -> Option<Value> {
        self.with_read(|d| d.get(key).cloned())
    }

    pub fn get_scope(&self, key: Setting) -> Option<SettingScope> {
        self.with_read(|d| d.get_scope(key))
    }

    pub async fn set(
        &self,
        key: Setting,
        value: impl Into<serde_json::Value>,
        scope: Option<SettingScope>,
    ) -> Result<(), DatabaseError> {
        let scope = scope.unwrap_or(SettingScope::Global);
        let value = value.into();

        match scope {
            SettingScope::Global => {
                self.locked_write(PersistTarget::Global, |map| {
                    map.insert(key.to_string(), value);
                })
                .await
            },
            SettingScope::Workspace => {
                if !key.is_workspace_overridable() {
                    return Err(DatabaseError::WorkspaceOverrideNotAllowed(key.to_string()));
                }
                if self.with_read(|d| d.workspace.is_none()) {
                    return Err(DatabaseError::WorkspaceOverrideNotAllowed(
                        "no workspace settings loaded".to_string(),
                    ));
                }
                self.locked_write(PersistTarget::Workspace, |map| {
                    map.insert(key.to_string(), value);
                })
                .await
            },
            SettingScope::Session => {
                self.inner
                    .write()
                    .expect("settings lock poisoned")
                    .session
                    .insert(key.to_string(), value);
                Ok(())
            },
        }
    }

    /// Deep-merge `patch` into the global value of `key` and persist, as one
    /// atomic RMW under the settings file lock.
    pub async fn merge(&self, key: Setting, patch: serde_json::Value) -> Result<(), DatabaseError> {
        self.locked_write(PersistTarget::Global, |map| {
            let mut current = map.get(key.as_ref()).cloned().unwrap_or_else(|| serde_json::json!({}));
            deep_merge(&mut current, patch);
            map.insert(key.to_string(), current);
        })
        .await
    }

    /// Replace the global value of `key` with `f` applied to the current value
    /// (`None` when unset), as one atomic RMW under the settings file lock.
    /// The closure can remove nested keys, which a deep merge cannot.
    pub async fn update(&self, key: Setting, f: impl FnOnce(Option<Value>) -> Value) -> Result<(), DatabaseError> {
        self.locked_write(PersistTarget::Global, |map| {
            let current = map.get(key.as_ref()).cloned();
            map.insert(key.to_string(), f(current));
        })
        .await
    }

    pub async fn remove(&self, key: Setting, scope: Option<SettingScope>) -> Result<Option<Value>, DatabaseError> {
        let scope = scope.unwrap_or(SettingScope::Global);

        match scope {
            SettingScope::Global => {
                self.locked_write(PersistTarget::Global, |map| map.remove(key.as_ref()))
                    .await
            },
            SettingScope::Workspace => {
                if self.with_read(|d| d.workspace.is_none()) {
                    return Err(DatabaseError::WorkspaceOverrideNotAllowed(
                        "no workspace settings loaded".to_string(),
                    ));
                }
                self.locked_write(PersistTarget::Workspace, |map| map.remove(key.as_ref()))
                    .await
            },
            SettingScope::Session => Ok(self
                .inner
                .write()
                .expect("settings lock poisoned")
                .session
                .remove(key.as_ref())),
        }
    }

    /// Read-modify-write of one settings file under an exclusive advisory file lock.
    ///
    /// Semantics:
    /// - Re-reads the file under the lock, so a write folds in other processes' changes instead of
    ///   clobbering them
    /// - Converges in-memory state to the written result
    /// - Corrupt/unreadable file: falls back to the in-memory copy (self-heals)
    /// - Disk before memory: a failed write applies nowhere and returns `Err`
    /// - No disk target (test default): in-memory only
    async fn locked_write<R>(
        &self,
        target: PersistTarget,
        mutate: impl FnOnce(&mut Map<String, Value>) -> R,
    ) -> Result<R, DatabaseError> {
        let path = self.with_read(|d| match target {
            PersistTarget::Global => d.global_settings_path.clone(),
            PersistTarget::Workspace => d.workspace_settings_path.clone(),
        });
        let Some(path) = path else {
            let mut data = self.inner.write().expect("settings lock poisoned");
            let map = match target {
                PersistTarget::Global => &mut data.global,
                PersistTarget::Workspace => data.workspace.get_or_insert_with(Map::new),
            };
            return Ok(mutate(map));
        };

        // Sibling lock file: append ".lock" to the full file name (cli.json ->
        // cli.json.lock). `with_extension` would replace ".json" instead.
        let mut lock_path = path.clone().into_os_string();
        lock_path.push(".lock");
        let lock_path = std::path::PathBuf::from(lock_path);

        with_file_lock_at(&lock_path, SETTINGS_FILE_LOCK_TIMEOUT, || async {
            let mut fresh = match Self::load_settings_file(&path).await {
                Ok(map) => map,
                Err(_) => self.with_read(|d| match target {
                    PersistTarget::Global => d.global.clone(),
                    PersistTarget::Workspace => d.workspace.clone().unwrap_or_default(),
                }),
            };
            let result = mutate(&mut fresh);
            Self::save_settings_file(&path, &fresh).await?;
            {
                let mut data = self.inner.write().expect("settings lock poisoned");
                match target {
                    PersistTarget::Global => data.global = fresh,
                    PersistTarget::Workspace => data.workspace = Some(fresh),
                }
            }
            Ok(result)
        })
        .await?
    }

    pub fn clear_session(&self) {
        self.inner.write().expect("settings lock poisoned").session.clear();
    }

    async fn save_settings_file(path: &std::path::PathBuf, map: &Map<String, Value>) -> Result<(), DatabaseError> {
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
        self.with_read(|d| d.get(key).and_then(|value| value.as_bool()))
    }

    pub fn get_string(&self, key: Setting) -> Option<String> {
        self.with_read(|d| d.get(key).and_then(|value| value.as_str().map(|s| s.into())))
    }

    pub fn get_int(&self, key: Setting) -> Option<i64> {
        self.with_read(|d| d.get(key).and_then(|value| value.as_i64()))
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

    /// A cloned handle must observe writes made through the original: cloning
    /// shares one underlying store rather than deep-copying. Guards against the
    /// per-session settings drift this shared-handle design exists to prevent.
    #[tokio::test]
    async fn test_clone_shares_store() {
        let original = Settings::new().await.unwrap();
        let clone = original.clone();

        original
            .set(Setting::ChatDefaultModel, "shared-model", None)
            .await
            .unwrap();

        assert_eq!(
            clone.get_string(Setting::ChatDefaultModel).as_deref(),
            Some("shared-model"),
            "a write through one handle must be visible through a clone"
        );
    }

    #[test]
    fn test_deep_merge_objects_merge_other_types_replace() {
        let mut base = serde_json::json!({
            "a": { "x": 1, "keep": true },
            "s": "old",
            "arr": [1, 2, 3],
        });
        deep_merge(
            &mut base,
            serde_json::json!({
                "a": { "x": 2, "new": 3 },
                "s": "new",
                "arr": [9],
                "added": null,
            }),
        );
        assert_eq!(
            base,
            serde_json::json!({
                "a": { "x": 2, "keep": true, "new": 3 },
                "s": "new",
                "arr": [9],
                "added": null,
            })
        );
    }

    /// `merge` must deep-merge the patch into the existing value and persist
    /// the result: disk contents equal the in-memory view after the call.
    #[tokio::test]
    async fn test_merge_deep_merges_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");
        let settings = Settings::test_with_global_path(path.clone());

        settings
            .set(
                Setting::ChatModelDefaults,
                serde_json::json!({ "model-a": { "effort": "low" } }),
                None,
            )
            .await
            .unwrap();
        settings
            .merge(
                Setting::ChatModelDefaults,
                serde_json::json!({ "model-a": { "extra": true }, "model-b": { "effort": "high" } }),
            )
            .await
            .unwrap();

        let expected = serde_json::json!({
            "model-a": { "effort": "low", "extra": true },
            "model-b": { "effort": "high" },
        });
        assert_eq!(settings.get_value(Setting::ChatModelDefaults), Some(expected));

        let disk: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(disk, settings.map(), "disk must match in-memory state after merge");
    }

    /// `merge` on a key with no existing value starts from an empty object.
    #[tokio::test]
    async fn test_merge_into_absent_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");
        let settings = Settings::test_with_global_path(path);

        settings
            .merge(
                Setting::ChatModelDefaults,
                serde_json::json!({ "m": { "effort": "low" } }),
            )
            .await
            .unwrap();

        assert_eq!(
            settings.get_value(Setting::ChatModelDefaults),
            Some(serde_json::json!({ "m": { "effort": "low" } }))
        );
    }

    /// Workspace-scope `set`/`remove` persist through the same locked write
    /// path as global, creating the settings file (and parent dirs) on first
    /// write. A store with no workspace loaded rejects workspace-scope sets.
    #[tokio::test]
    async fn test_workspace_scope_set_and_remove_persist() {
        let dir = tempfile::tempdir().unwrap();
        // Parent dirs do not exist yet; the first locked write must create them.
        let path = dir.path().join(".kiro").join("settings").join("cli.json");
        let settings = Settings::test_with_workspace_path(path.clone());

        settings
            .set(Setting::ChatAutoExpandToolOutput, true, Some(SettingScope::Workspace))
            .await
            .unwrap();
        assert_eq!(
            settings.get_scope(Setting::ChatAutoExpandToolOutput),
            Some(SettingScope::Workspace)
        );
        let disk: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(disk.get("chat.autoExpandToolOutput"), Some(&Value::Bool(true)));

        let removed = settings
            .remove(Setting::ChatAutoExpandToolOutput, Some(SettingScope::Workspace))
            .await
            .unwrap();
        assert_eq!(removed, Some(Value::Bool(true)));
        let disk: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert!(!disk.contains_key("chat.autoExpandToolOutput"));

        // No workspace loaded: workspace-scope set is rejected.
        let no_workspace = Settings::test_with_global_path(dir.path().join("global.json"));
        let err = no_workspace
            .set(Setting::ChatAutoExpandToolOutput, true, Some(SettingScope::Workspace))
            .await
            .unwrap_err();
        assert!(matches!(err, DatabaseError::WorkspaceOverrideNotAllowed(_)));
    }

    /// Global `remove` runs the same locked snapshot-and-write cycle as `set`:
    /// the removed value is returned and the file reflects the removal.
    #[tokio::test]
    async fn test_remove_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");
        let settings = Settings::test_with_global_path(path.clone());

        settings.set(Setting::ChatDefaultModel, "opus", None).await.unwrap();
        let removed = settings.remove(Setting::ChatDefaultModel, None).await.unwrap();
        assert_eq!(removed, Some(Value::String("opus".to_string())));

        let disk: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert!(!disk.contains_key("chat.defaultModel"));
        assert_eq!(settings.get_value(Setting::ChatDefaultModel), None);
    }

    /// Two stores on one path simulate two processes: a write folds in the
    /// other's on-disk changes instead of clobbering them.
    #[tokio::test]
    async fn test_writes_fold_in_foreign_process_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");
        let a = Settings::test_with_global_path(path.clone());
        let b = Settings::test_with_global_path(path.clone());

        a.set(Setting::ChatDefaultModel, "opus", None).await.unwrap();
        b.set(Setting::TelemetryEnabled, true, None).await.unwrap();

        let disk: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(
            disk.get("chat.defaultModel"),
            Some(&Value::String("opus".to_string())),
            "b's write must not clobber a's earlier write"
        );
        assert_eq!(disk.get("telemetry.enabled"), Some(&Value::Bool(true)));

        // b's in-memory view folded in a's foreign write.
        assert_eq!(b.get_string(Setting::ChatDefaultModel).as_deref(), Some("opus"));

        // A foreign removal is adopted the same way: a removes its key, then
        // b's next write must not resurrect it.
        a.remove(Setting::ChatDefaultModel, None).await.unwrap();
        b.set(Setting::McpLoadedBefore, true, None).await.unwrap();
        let disk: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert!(
            !disk.contains_key("chat.defaultModel"),
            "b's write must not resurrect a key removed by a"
        );
        assert_eq!(b.get_value(Setting::ChatDefaultModel), None);
    }

    /// A corrupt settings file must not brick writes: the writer falls back to
    /// its in-memory state, so the write succeeds and the file is restored to
    /// valid JSON containing both prior and new state.
    #[tokio::test]
    async fn test_write_self_heals_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");
        let settings = Settings::test_with_global_path(path.clone());

        settings.set(Setting::ChatDefaultModel, "opus", None).await.unwrap();
        tokio::fs::write(&path, "not json").await.unwrap();

        settings.set(Setting::TelemetryEnabled, true, None).await.unwrap();

        let disk: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(disk.get("chat.defaultModel"), Some(&Value::String("opus".to_string())));
        assert_eq!(disk.get("telemetry.enabled"), Some(&Value::Bool(true)));
    }

    /// Workspace-scope ops on a store with no workspace loaded are rejected;
    /// a `remove` must not materialize a workspace scope that persists nowhere.
    #[tokio::test]
    async fn test_workspace_ops_rejected_without_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");
        let settings = Settings::test_with_global_path(path);

        let err = settings
            .remove(Setting::ChatAutoExpandToolOutput, Some(SettingScope::Workspace))
            .await
            .unwrap_err();
        assert!(matches!(err, DatabaseError::WorkspaceOverrideNotAllowed(_)));

        // The rejected remove must not have flipped the store into a fake
        // workspace mode that lets set pass its guard.
        let err = settings
            .set(Setting::ChatAutoExpandToolOutput, true, Some(SettingScope::Workspace))
            .await
            .unwrap_err();
        assert!(matches!(err, DatabaseError::WorkspaceOverrideNotAllowed(_)));
        assert_eq!(settings.get_scope(Setting::ChatAutoExpandToolOutput), None);
    }

    /// A failed disk write applies nowhere: the error propagates and the
    /// in-memory value is untouched.
    #[tokio::test]
    async fn test_failed_save_leaves_memory_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");
        let settings = Settings::test_with_global_path(path.clone());
        settings.set(Setting::ChatDefaultModel, "opus", None).await.unwrap();

        // Replace the settings file with a directory: the re-read fails over
        // to memory and the atomic rename onto a directory fails the save.
        tokio::fs::remove_file(&path).await.unwrap();
        tokio::fs::create_dir(&path).await.unwrap();

        let result = settings.set(Setting::ChatDefaultModel, "haiku", None).await;
        assert!(result.is_err());
        assert_eq!(settings.get_string(Setting::ChatDefaultModel).as_deref(), Some("opus"));
    }

    /// Concurrent `merge`s and `set`s through cloned handles must not lose
    /// updates in memory, and the last disk write must reflect the final
    /// in-memory state (one total order across memory mutation and disk write).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_writers_converge_disk_and_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cli.json");
        let settings = Settings::test_with_global_path(path.clone());

        let mut handles = Vec::new();
        for i in 0..16 {
            let s = settings.clone();
            handles.push(tokio::spawn(async move {
                let mut patch = Map::new();
                patch.insert(format!("model-{i}"), serde_json::json!({ "effort": i }));
                s.merge(Setting::ChatModelDefaults, Value::Object(patch)).await.unwrap();
            }));
        }
        for i in 0..8 {
            let s = settings.clone();
            handles.push(tokio::spawn(async move {
                s.set(Setting::ChatDefaultModel, format!("m{i}"), None).await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        // No merge may be lost to a read-modify-write race.
        let merged = settings.get_value(Setting::ChatModelDefaults).unwrap();
        for i in 0..16 {
            assert_eq!(
                merged.get(format!("model-{i}")).and_then(|m| m.get("effort")),
                Some(&serde_json::json!(i)),
                "merge {i} was lost"
            );
        }

        // Disk must equal final memory: no stale snapshot may land last.
        let disk: Map<String, Value> = serde_json::from_str(&tokio::fs::read_to_string(&path).await.unwrap()).unwrap();
        assert_eq!(disk, settings.map(), "a stale snapshot won the last disk write");
    }

    /// General read/write settings test
    #[tokio::test]
    async fn test_settings() {
        let settings = Settings::new().await.unwrap();

        assert_eq!(settings.get_value(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get_value(Setting::OldClientId), None);
        assert_eq!(settings.get_value(Setting::ShareCodeWhispererContent), None);
        assert_eq!(settings.get_value(Setting::KnowledgeIndexType), None);
        assert_eq!(settings.get_value(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get_value(Setting::ChatDefaultModel), None);
        assert_eq!(settings.get_value(Setting::ChatDisableMarkdownRendering), None);

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

        assert_eq!(settings.get_value(Setting::TelemetryEnabled), Some(Value::Bool(true)));
        assert_eq!(
            settings.get_value(Setting::OldClientId),
            Some(Value::String("test".to_string()))
        );
        assert_eq!(
            settings.get_value(Setting::ShareCodeWhispererContent),
            Some(Value::Bool(false))
        );
        assert_eq!(
            settings.get_value(Setting::KnowledgeIndexType),
            Some(Value::String("fast".to_string()))
        );
        assert_eq!(settings.get_value(Setting::McpLoadedBefore), Some(Value::Bool(true)));
        assert_eq!(
            settings.get_value(Setting::ChatDefaultModel),
            Some(Value::String("model 1".to_string()))
        );
        assert_eq!(
            settings.get_value(Setting::ChatDiffTool),
            Some(Value::String("diff tool".to_string()))
        );
        assert_eq!(
            settings.get_value(Setting::ChatDisableMarkdownRendering),
            Some(Value::Bool(false))
        );
        assert_eq!(settings.get_value(Setting::EnabledCheckpoint), Some(Value::Bool(true)));

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

        assert_eq!(settings.get_value(Setting::TelemetryEnabled), None);
        assert_eq!(settings.get_value(Setting::OldClientId), None);
        assert_eq!(settings.get_value(Setting::ShareCodeWhispererContent), None);
        assert_eq!(settings.get_value(Setting::KnowledgeIndexType), None);
        assert_eq!(settings.get_value(Setting::McpLoadedBefore), None);
        assert_eq!(settings.get_value(Setting::ChatDisableMarkdownRendering), None);
        assert_eq!(settings.get_value(Setting::EnabledCheckpoint), None);
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

    #[test]
    fn test_ui_mode_is_global_only() {
        // The default UI (chat.ui.mode: lite vs tui) is global so it applies to all workspaces.
        assert!(!Setting::UiMode.is_workspace_overridable());
    }

    #[tokio::test]
    async fn test_auto_expand_tool_output_read_write() {
        let settings = Settings::new().await.unwrap();

        // Default: not set
        assert_eq!(settings.get_value(Setting::ChatAutoExpandToolOutput), None);

        // Set to true
        settings
            .set(Setting::ChatAutoExpandToolOutput, true, None)
            .await
            .unwrap();
        assert_eq!(
            settings.get_value(Setting::ChatAutoExpandToolOutput),
            Some(Value::Bool(true))
        );

        // Set to false
        settings
            .set(Setting::ChatAutoExpandToolOutput, false, None)
            .await
            .unwrap();
        assert_eq!(
            settings.get_value(Setting::ChatAutoExpandToolOutput),
            Some(Value::Bool(false))
        );

        // Remove
        settings.remove(Setting::ChatAutoExpandToolOutput, None).await.unwrap();
        assert_eq!(settings.get_value(Setting::ChatAutoExpandToolOutput), None);
    }

    #[test]
    fn test_ui_mode_accepts_dotted_alias() {
        // Frontend writes `chat.ui.mode`; canonical form is `chat.uiMode`.
        let canonical = Setting::try_from("chat.uiMode").unwrap();
        let aliased = Setting::try_from("chat.ui.mode").unwrap();
        assert!(matches!(canonical, Setting::UiMode));
        assert!(matches!(aliased, Setting::UiMode));
    }

    #[test]
    fn test_preserve_scrollback_is_global_only() {
        // The TUI reads this key from the global settings file only.
        assert!(!Setting::ChatPreserveScrollback.is_workspace_overridable());
    }

    #[tokio::test]
    async fn test_preserve_scrollback_read_write() {
        let settings = Settings::new().await.unwrap();
        assert_eq!(settings.get_value(Setting::ChatPreserveScrollback), None);
        settings.set(Setting::ChatPreserveScrollback, true, None).await.unwrap();
        assert_eq!(
            settings.get_value(Setting::ChatPreserveScrollback),
            Some(Value::Bool(true))
        );
        settings.remove(Setting::ChatPreserveScrollback, None).await.unwrap();
        assert_eq!(settings.get_value(Setting::ChatPreserveScrollback), None);
    }
}
