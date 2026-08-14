use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

/// Canonical bounded slash-command identity used by telemetry in Rust and the TUI.
#[typeshare]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum SlashCommandMetricName {
    #[serde(rename = "/agent")]
    Agent,
    #[serde(rename = "/autonomous")]
    Autonomous,
    #[serde(rename = "/changelog")]
    Changelog,
    #[serde(rename = "/chat")]
    Chat,
    #[serde(rename = "/checkpoint")]
    Checkpoint,
    #[serde(rename = "/clear")]
    Clear,
    #[serde(rename = "/code")]
    Code,
    #[serde(rename = "/compact")]
    Compact,
    #[serde(rename = "/config")]
    Config,
    #[serde(rename = "/context")]
    Context,
    #[serde(rename = "/context-gatherer")]
    ContextGatherer,
    #[serde(rename = "/copy")]
    Copy,
    #[serde(rename = "/custom")]
    Custom,
    #[serde(rename = "/disconnect")]
    Disconnect,
    #[serde(rename = "/editor")]
    Editor,
    #[serde(rename = "/effort")]
    Effort,
    #[serde(rename = "/exit")]
    Exit,
    #[serde(rename = "/experiment")]
    Experiment,
    #[serde(rename = "/feedback")]
    Feedback,
    #[serde(rename = "/general-task-execution")]
    GeneralTaskExecution,
    #[serde(rename = "/goal")]
    Goal,
    #[serde(rename = "/guide")]
    Guide,
    #[serde(rename = "/help")]
    Help,
    #[serde(rename = "/hooks")]
    Hooks,
    #[serde(rename = "/issue")]
    Issue,
    #[serde(rename = "/knowledge")]
    Knowledge,
    #[serde(rename = "/lite")]
    Lite,
    #[serde(rename = "/load")]
    Load,
    #[serde(rename = "/logdump")]
    Logdump,
    #[serde(rename = "/mcp")]
    Mcp,
    #[serde(rename = "/memories")]
    Memories,
    #[serde(rename = "/model")]
    Model,
    #[serde(rename = "/paste")]
    Paste,
    #[serde(rename = "/plan")]
    Plan,
    #[serde(rename = "/prompt")]
    Prompt,
    #[serde(rename = "/prompts")]
    Prompts,
    #[serde(rename = "/quit")]
    Quit,
    #[serde(rename = "/reply")]
    Reply,
    #[serde(rename = "/repo")]
    Repo,
    #[serde(rename = "/rewind")]
    Rewind,
    #[serde(rename = "/save")]
    Save,
    #[serde(rename = "/session-id")]
    SessionId,
    #[serde(rename = "/sessions")]
    Sessions,
    #[serde(rename = "/settings")]
    Settings,
    #[serde(rename = "/skill")]
    Skill,
    #[serde(rename = "/spawn")]
    Spawn,
    #[serde(rename = "/spec")]
    Spec,
    #[serde(rename = "/stats")]
    Stats,
    #[serde(rename = "/steering")]
    Steering,
    #[serde(rename = "/switch")]
    Switch,
    #[serde(rename = "/tangent")]
    Tangent,
    #[serde(rename = "/theme")]
    Theme,
    #[serde(rename = "/title")]
    Title,
    #[serde(rename = "/todos")]
    Todos,
    #[serde(rename = "/tools")]
    Tools,
    #[serde(rename = "/transcript")]
    Transcript,
    #[serde(rename = "/tui")]
    Tui,
    #[serde(rename = "/upgrade-agent")]
    UpgradeAgent,
    #[serde(rename = "/usage")]
    Usage,
    #[serde(rename = "/verbosity")]
    Verbosity,
    #[serde(rename = "/voice")]
    Voice,
    #[serde(rename = "/workflow")]
    Workflow,
    #[serde(rename = "/workflow-cancel")]
    WorkflowCancel,
    #[serde(rename = "/workflow-resume")]
    WorkflowResume,
    #[serde(rename = "/workflow-run")]
    WorkflowRun,
    #[serde(rename = "/workflow-status")]
    WorkflowStatus,
    #[serde(rename = "/workflows")]
    Workflows,
}

impl SlashCommandMetricName {
    pub const ALL: &'static [Self] = &[
        Self::Agent,
        Self::Autonomous,
        Self::Changelog,
        Self::Chat,
        Self::Checkpoint,
        Self::Clear,
        Self::Code,
        Self::Compact,
        Self::Config,
        Self::Context,
        Self::ContextGatherer,
        Self::Copy,
        Self::Custom,
        Self::Disconnect,
        Self::Editor,
        Self::Effort,
        Self::Exit,
        Self::Experiment,
        Self::Feedback,
        Self::GeneralTaskExecution,
        Self::Goal,
        Self::Guide,
        Self::Help,
        Self::Hooks,
        Self::Issue,
        Self::Knowledge,
        Self::Lite,
        Self::Load,
        Self::Logdump,
        Self::Mcp,
        Self::Memories,
        Self::Model,
        Self::Paste,
        Self::Plan,
        Self::Prompt,
        Self::Prompts,
        Self::Quit,
        Self::Reply,
        Self::Repo,
        Self::Rewind,
        Self::Save,
        Self::SessionId,
        Self::Sessions,
        Self::Settings,
        Self::Skill,
        Self::Spawn,
        Self::Spec,
        Self::Stats,
        Self::Steering,
        Self::Switch,
        Self::Tangent,
        Self::Theme,
        Self::Title,
        Self::Todos,
        Self::Tools,
        Self::Transcript,
        Self::Tui,
        Self::UpgradeAgent,
        Self::Usage,
        Self::Verbosity,
        Self::Voice,
        Self::Workflow,
        Self::WorkflowCancel,
        Self::WorkflowResume,
        Self::WorkflowRun,
        Self::WorkflowStatus,
        Self::Workflows,
    ];

    pub fn from_name(value: &str) -> Self {
        let normalized = value.trim().to_ascii_lowercase();
        let normalized = if normalized.starts_with('/') {
            normalized
        } else {
            format!("/{normalized}")
        };
        match normalized.as_str() {
            "/agent" => Self::Agent,
            "/autonomous" => Self::Autonomous,
            "/changelog" => Self::Changelog,
            "/chat" => Self::Chat,
            "/checkpoint" => Self::Checkpoint,
            "/clear" => Self::Clear,
            "/code" => Self::Code,
            "/compact" => Self::Compact,
            "/config" => Self::Config,
            "/context" => Self::Context,
            "/context-gatherer" => Self::ContextGatherer,
            "/copy" => Self::Copy,
            "/custom" => Self::Custom,
            "/disconnect" => Self::Disconnect,
            "/editor" => Self::Editor,
            "/effort" => Self::Effort,
            "/exit" => Self::Exit,
            "/experiment" => Self::Experiment,
            "/feedback" => Self::Feedback,
            "/general-task-execution" => Self::GeneralTaskExecution,
            "/goal" => Self::Goal,
            "/guide" => Self::Guide,
            "/help" => Self::Help,
            "/hooks" => Self::Hooks,
            "/issue" => Self::Issue,
            "/knowledge" => Self::Knowledge,
            "/lite" => Self::Lite,
            "/load" => Self::Load,
            "/logdump" => Self::Logdump,
            "/mcp" => Self::Mcp,
            "/memories" => Self::Memories,
            "/model" => Self::Model,
            "/paste" => Self::Paste,
            "/plan" => Self::Plan,
            "/prompt" => Self::Prompt,
            "/prompts" => Self::Prompts,
            "/quit" => Self::Quit,
            "/reply" => Self::Reply,
            "/repo" => Self::Repo,
            "/rewind" => Self::Rewind,
            "/save" => Self::Save,
            "/session-id" => Self::SessionId,
            "/sessions" => Self::Sessions,
            "/settings" => Self::Settings,
            "/skill" => Self::Skill,
            "/spawn" => Self::Spawn,
            "/spec" => Self::Spec,
            "/stats" => Self::Stats,
            "/steering" => Self::Steering,
            "/switch" => Self::Switch,
            "/tangent" => Self::Tangent,
            "/theme" => Self::Theme,
            "/title" => Self::Title,
            "/todos" => Self::Todos,
            "/tools" => Self::Tools,
            "/transcript" => Self::Transcript,
            "/tui" => Self::Tui,
            "/upgrade-agent" => Self::UpgradeAgent,
            "/usage" => Self::Usage,
            "/verbosity" => Self::Verbosity,
            "/voice" => Self::Voice,
            "/workflow" => Self::Workflow,
            "/workflow-cancel" => Self::WorkflowCancel,
            "/workflow-resume" => Self::WorkflowResume,
            "/workflow-run" => Self::WorkflowRun,
            "/workflow-status" => Self::WorkflowStatus,
            "/workflows" => Self::Workflows,
            _ => Self::Custom,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "/agent",
            Self::Autonomous => "/autonomous",
            Self::Changelog => "/changelog",
            Self::Chat => "/chat",
            Self::Checkpoint => "/checkpoint",
            Self::Clear => "/clear",
            Self::Code => "/code",
            Self::Compact => "/compact",
            Self::Config => "/config",
            Self::Context => "/context",
            Self::ContextGatherer => "/context-gatherer",
            Self::Copy => "/copy",
            Self::Custom => "/custom",
            Self::Disconnect => "/disconnect",
            Self::Editor => "/editor",
            Self::Effort => "/effort",
            Self::Exit => "/exit",
            Self::Experiment => "/experiment",
            Self::Feedback => "/feedback",
            Self::GeneralTaskExecution => "/general-task-execution",
            Self::Goal => "/goal",
            Self::Guide => "/guide",
            Self::Help => "/help",
            Self::Hooks => "/hooks",
            Self::Issue => "/issue",
            Self::Knowledge => "/knowledge",
            Self::Lite => "/lite",
            Self::Load => "/load",
            Self::Logdump => "/logdump",
            Self::Mcp => "/mcp",
            Self::Memories => "/memories",
            Self::Model => "/model",
            Self::Paste => "/paste",
            Self::Plan => "/plan",
            Self::Prompt => "/prompt",
            Self::Prompts => "/prompts",
            Self::Quit => "/quit",
            Self::Reply => "/reply",
            Self::Repo => "/repo",
            Self::Rewind => "/rewind",
            Self::Save => "/save",
            Self::SessionId => "/session-id",
            Self::Sessions => "/sessions",
            Self::Settings => "/settings",
            Self::Skill => "/skill",
            Self::Spawn => "/spawn",
            Self::Spec => "/spec",
            Self::Stats => "/stats",
            Self::Steering => "/steering",
            Self::Switch => "/switch",
            Self::Tangent => "/tangent",
            Self::Theme => "/theme",
            Self::Title => "/title",
            Self::Todos => "/todos",
            Self::Tools => "/tools",
            Self::Transcript => "/transcript",
            Self::Tui => "/tui",
            Self::UpgradeAgent => "/upgrade-agent",
            Self::Usage => "/usage",
            Self::Verbosity => "/verbosity",
            Self::Voice => "/voice",
            Self::Workflow => "/workflow",
            Self::WorkflowCancel => "/workflow-cancel",
            Self::WorkflowResume => "/workflow-resume",
            Self::WorkflowRun => "/workflow-run",
            Self::WorkflowStatus => "/workflow-status",
            Self::Workflows => "/workflows",
        }
    }
}

/// Canonical bounded top-level CLI command identity used by telemetry.
#[typeshare]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopLevelCommandMetricName {
    Agent,
    Chat,
    Completion,
    Doctor,
    Help,
    Issue,
    Login,
    Logout,
    Mcp,
    Profile,
    Settings,
    Unknown,
    Update,
    Version,
}

impl TopLevelCommandMetricName {
    pub const ALL: &'static [Self] = &[
        Self::Agent,
        Self::Chat,
        Self::Completion,
        Self::Doctor,
        Self::Help,
        Self::Issue,
        Self::Login,
        Self::Logout,
        Self::Mcp,
        Self::Profile,
        Self::Settings,
        Self::Unknown,
        Self::Update,
        Self::Version,
    ];

    pub fn from_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "agent" => Self::Agent,
            "chat" => Self::Chat,
            "completion" => Self::Completion,
            "doctor" => Self::Doctor,
            "help" => Self::Help,
            "issue" => Self::Issue,
            "login" => Self::Login,
            "logout" => Self::Logout,
            "mcp" => Self::Mcp,
            "profile" => Self::Profile,
            "settings" => Self::Settings,
            "unknown" => Self::Unknown,
            "update" => Self::Update,
            "version" => Self::Version,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Chat => "chat",
            Self::Completion => "completion",
            Self::Doctor => "doctor",
            Self::Help => "help",
            Self::Issue => "issue",
            Self::Login => "login",
            Self::Logout => "logout",
            Self::Mcp => "mcp",
            Self::Profile => "profile",
            Self::Settings => "settings",
            Self::Unknown => "unknown",
            Self::Update => "update",
            Self::Version => "version",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_commands_use_bounded_fallbacks() {
        assert_eq!(
            SlashCommandMetricName::from_name("future-command"),
            SlashCommandMetricName::Custom
        );
        assert_eq!(
            TopLevelCommandMetricName::from_name("future-command"),
            TopLevelCommandMetricName::Unknown
        );
    }
}
