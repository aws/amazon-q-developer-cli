use kiro_telemetry::metric::{
    AgentMode,
    AuthFailureReason,
    AuthFlow,
    AuthMethod,
    CloudSessionEvent,
    CrashKind,
    Engine,
    ErrorKind,
    ExecutionContext,
    ExportDropReason,
    GoalOutcome,
    InstallSource,
    McpInitOutcome,
    McpServerSource,
    OsType,
    ProcessRole,
    ReleaseChannel,
    RenderKind,
    RunOutcome,
    SessionInterface,
    ToolMetricOrigin,
    ToolMetricOutcome,
    TrustPosture,
    TurnFailureReason,
    UiMode,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ScenarioIdentity {
    StableMacV1,
    StableWindowsV2,
    NightlyLinuxV3,
    BetaMacExternalV3,
    NightlyWindowsExternalV2,
    StableLinuxHeadlessV1,
    DevelopmentUnknown,
}

impl ScenarioIdentity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StableMacV1 => "stable-macos-v1",
            Self::StableWindowsV2 => "stable-windows-v2",
            Self::NightlyLinuxV3 => "nightly-linux-v3",
            Self::BetaMacExternalV3 => "beta-macos-external-v3",
            Self::NightlyWindowsExternalV2 => "nightly-windows-external-v2",
            Self::StableLinuxHeadlessV1 => "stable-linux-headless-v1",
            Self::DevelopmentUnknown => "development-unknown",
        }
    }
}

#[derive(Clone, Copy)]
pub enum SlashCommand {
    Help,
    Settings,
    UpgradeAgent,
    WorkflowRun,
    Goal,
    Model,
    Custom,
}

impl SlashCommand {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Help => "/help",
            Self::Settings => "/settings",
            Self::UpgradeAgent => "/upgrade-agent",
            Self::WorkflowRun => "/workflow-run",
            Self::Goal => "/goal",
            Self::Model => "/model",
            Self::Custom => "/custom",
        }
    }
}

#[derive(Clone, Copy)]
pub enum TopLevelCommand {
    Chat,
    Login,
    Doctor,
    Mcp,
    Update,
}

impl TopLevelCommand {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Login => "login",
            Self::Doctor => "doctor",
            Self::Mcp => "mcp",
            Self::Update => "update",
        }
    }
}

#[derive(Clone, Copy)]
pub enum DemoMcpServer {
    AwsDocs,
    Postgres,
    Filesystem,
    CustomAcp,
    Github,
    Builder,
    Unknown,
}

impl DemoMcpServer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwsDocs => "aws-docs",
            Self::Postgres => "postgres",
            Self::Filesystem => "filesystem",
            Self::CustomAcp => "custom-acp",
            Self::Github => "github",
            Self::Builder => "builder",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy)]
pub struct CommandPlan {
    pub slash: SlashCommand,
    pub top_level: TopLevelCommand,
}

#[derive(Clone, Copy)]
pub struct McpPlan {
    pub source: McpServerSource,
    pub outcome: McpInitOutcome,
    pub server: DemoMcpServer,
}

#[derive(Clone, Copy)]
pub struct ResourcePlan {
    pub rss_mebibytes: f64,
    pub open_file_descriptors: f64,
    pub windows_handles: f64,
    pub threads: f64,
    pub peak_rss_mebibytes: f64,
    pub tui_heap_mebibytes: f64,
}

#[derive(Clone, Copy)]
pub struct ActivityPlan {
    pub volume: u64,
    pub cloud_event: CloudSessionEvent,
    pub cloud_ready_seconds: f64,
    pub sample_scale_base: f64,
    pub run_outcome: RunOutcome,
    pub goal_outcome: GoalOutcome,
    pub render_kind: RenderKind,
    pub resources: ResourcePlan,
}

#[derive(Clone, Copy)]
pub struct FailureDiagnostics {
    pub error_kind: ErrorKind,
    pub request_duration_seconds: f64,
    pub turn_failure_reason: TurnFailureReason,
    pub crash_kind: CrashKind,
    pub auth_failure_reason: AuthFailureReason,
}

#[derive(Clone, Copy)]
pub enum DiagnosticPolicy {
    None,
    Failure(FailureDiagnostics),
    Cancellation {
        request_duration_seconds: f64,
    },
    TelemetryProtection {
        dropped_records: u64,
        drop_reason: ExportDropReason,
    },
}

#[derive(Clone, Copy)]
pub struct Scenario {
    pub identity: ScenarioIdentity,
    pub version: &'static str,
    pub release_channel: ReleaseChannel,
    pub os_type: OsType,
    pub install_method: InstallSource,
    pub engine: Engine,
    pub session_interface: SessionInterface,
    pub agent_mode: AgentMode,
    pub model: &'static str,
    pub process_role: ProcessRole,
    pub auth_method: AuthMethod,
    pub auth_flow: AuthFlow,
    pub tool_origin: ToolMetricOrigin,
    pub tool_outcome: ToolMetricOutcome,
    pub execution_context: ExecutionContext,
    pub trust_posture: TrustPosture,
    pub mcp: McpPlan,
    pub ui_mode: UiMode,
    pub commands: CommandPlan,
    pub activity: ActivityPlan,
    pub diagnostics: DiagnosticPolicy,
}

pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        identity: ScenarioIdentity::StableMacV1,
        version: "2.7.0",
        release_channel: ReleaseChannel::Stable,
        os_type: OsType::Macos,
        install_method: InstallSource::Brew,
        engine: Engine::V1,
        session_interface: SessionInterface::InteractiveCli,
        agent_mode: AgentMode::Default,
        model: "claude-sonnet-4",
        process_role: ProcessRole::Host,
        auth_method: AuthMethod::BuilderId,
        auth_flow: AuthFlow::Pkce,
        tool_origin: ToolMetricOrigin::Builtin,
        tool_outcome: ToolMetricOutcome::Success,
        execution_context: ExecutionContext::Main,
        trust_posture: TrustPosture::PromptOnDemand,
        mcp: McpPlan {
            source: McpServerSource::Registry,
            outcome: McpInitOutcome::Success,
            server: DemoMcpServer::AwsDocs,
        },
        ui_mode: UiMode::Tui,
        commands: CommandPlan {
            slash: SlashCommand::Help,
            top_level: TopLevelCommand::Chat,
        },
        activity: ActivityPlan {
            volume: 8,
            cloud_event: CloudSessionEvent::Started,
            cloud_ready_seconds: 0.8,
            sample_scale_base: 1.0,
            run_outcome: RunOutcome::Success,
            goal_outcome: GoalOutcome::Completed,
            render_kind: RenderKind::Partial,
            resources: ResourcePlan {
                rss_mebibytes: 96.0,
                open_file_descriptors: 32.0,
                windows_handles: 72.0,
                threads: 8.0,
                peak_rss_mebibytes: 128.0,
                tui_heap_mebibytes: 72.0,
            },
        },
        diagnostics: DiagnosticPolicy::None,
    },
    Scenario {
        identity: ScenarioIdentity::StableWindowsV2,
        version: "2.7.0",
        release_channel: ReleaseChannel::Stable,
        os_type: OsType::Windows,
        install_method: InstallSource::Unknown,
        engine: Engine::V2,
        session_interface: SessionInterface::InteractiveCli,
        agent_mode: AgentMode::Plan,
        model: "claude-sonnet-4.5",
        process_role: ProcessRole::Tui,
        auth_method: AuthMethod::IdentityCenter,
        auth_flow: AuthFlow::DeviceCode,
        tool_origin: ToolMetricOrigin::Mcp,
        tool_outcome: ToolMetricOutcome::Error,
        execution_context: ExecutionContext::Main,
        trust_posture: TrustPosture::TrustAllTools,
        mcp: McpPlan {
            source: McpServerSource::Global,
            outcome: McpInitOutcome::Success,
            server: DemoMcpServer::Postgres,
        },
        ui_mode: UiMode::Lite,
        commands: CommandPlan {
            slash: SlashCommand::Settings,
            top_level: TopLevelCommand::Login,
        },
        activity: ActivityPlan {
            volume: 6,
            cloud_event: CloudSessionEvent::Ready,
            cloud_ready_seconds: 1.2,
            sample_scale_base: 1.2,
            run_outcome: RunOutcome::UserInterrupt,
            goal_outcome: GoalOutcome::Cancelled,
            render_kind: RenderKind::Full,
            resources: ResourcePlan {
                rss_mebibytes: 120.0,
                open_file_descriptors: 40.0,
                windows_handles: 84.0,
                threads: 10.0,
                peak_rss_mebibytes: 158.0,
                tui_heap_mebibytes: 84.0,
            },
        },
        diagnostics: DiagnosticPolicy::Failure(FailureDiagnostics {
            error_kind: ErrorKind::Timeout,
            request_duration_seconds: 2.3,
            turn_failure_reason: TurnFailureReason::Timeout,
            crash_kind: CrashKind::Other,
            auth_failure_reason: AuthFailureReason::InvalidOrExpiredCredential,
        }),
    },
    Scenario {
        identity: ScenarioIdentity::NightlyLinuxV3,
        version: "2.8.0-nightly.14",
        release_channel: ReleaseChannel::Nightly,
        os_type: OsType::Linux,
        install_method: InstallSource::Unknown,
        engine: Engine::V3,
        session_interface: SessionInterface::NoninteractiveCli,
        agent_mode: AgentMode::Spec,
        model: "claude-opus-4.1",
        process_role: ProcessRole::KasSubprocess,
        auth_method: AuthMethod::Social,
        auth_flow: AuthFlow::NotApplicable,
        tool_origin: ToolMetricOrigin::Builtin,
        tool_outcome: ToolMetricOutcome::Success,
        execution_context: ExecutionContext::Subagent,
        trust_posture: TrustPosture::PromptOnDemand,
        mcp: McpPlan {
            source: McpServerSource::AcpInjected,
            outcome: McpInitOutcome::Failure,
            server: DemoMcpServer::Filesystem,
        },
        ui_mode: UiMode::Tui,
        commands: CommandPlan {
            slash: SlashCommand::UpgradeAgent,
            top_level: TopLevelCommand::Doctor,
        },
        activity: ActivityPlan {
            volume: 5,
            cloud_event: CloudSessionEvent::ProvisionFailed,
            cloud_ready_seconds: 1.6,
            sample_scale_base: 1.4,
            run_outcome: RunOutcome::Failure,
            goal_outcome: GoalOutcome::IterationLimit,
            render_kind: RenderKind::Partial,
            resources: ResourcePlan {
                rss_mebibytes: 144.0,
                open_file_descriptors: 48.0,
                windows_handles: 96.0,
                threads: 12.0,
                peak_rss_mebibytes: 188.0,
                tui_heap_mebibytes: 96.0,
            },
        },
        diagnostics: DiagnosticPolicy::TelemetryProtection {
            dropped_records: 2,
            drop_reason: ExportDropReason::RetryExhausted,
        },
    },
    Scenario {
        identity: ScenarioIdentity::BetaMacExternalV3,
        version: "2.8.0-beta.2",
        release_channel: ReleaseChannel::Beta,
        os_type: OsType::Macos,
        install_method: InstallSource::Internal,
        engine: Engine::V3,
        session_interface: SessionInterface::ExternalAcp,
        agent_mode: AgentMode::Custom,
        model: "auto",
        process_role: ProcessRole::Host,
        auth_method: AuthMethod::ExternalIdp,
        auth_flow: AuthFlow::DeviceCode,
        tool_origin: ToolMetricOrigin::Builtin,
        tool_outcome: ToolMetricOutcome::Denied,
        execution_context: ExecutionContext::Subagent,
        trust_posture: TrustPosture::TrustAllTools,
        mcp: McpPlan {
            source: McpServerSource::Unknown,
            outcome: McpInitOutcome::Success,
            server: DemoMcpServer::CustomAcp,
        },
        ui_mode: UiMode::Tui,
        commands: CommandPlan {
            slash: SlashCommand::WorkflowRun,
            top_level: TopLevelCommand::Mcp,
        },
        activity: ActivityPlan {
            volume: 4,
            cloud_event: CloudSessionEvent::Reattached,
            cloud_ready_seconds: 2.0,
            sample_scale_base: 1.6,
            run_outcome: RunOutcome::Success,
            goal_outcome: GoalOutcome::DispatchFailure,
            render_kind: RenderKind::Full,
            resources: ResourcePlan {
                rss_mebibytes: 168.0,
                open_file_descriptors: 56.0,
                windows_handles: 108.0,
                threads: 14.0,
                peak_rss_mebibytes: 218.0,
                tui_heap_mebibytes: 108.0,
            },
        },
        diagnostics: DiagnosticPolicy::Failure(FailureDiagnostics {
            error_kind: ErrorKind::ServerError,
            request_duration_seconds: 2.9,
            turn_failure_reason: TurnFailureReason::ToolError,
            crash_kind: CrashKind::UncleanExit,
            auth_failure_reason: AuthFailureReason::Timeout,
        }),
    },
    Scenario {
        identity: ScenarioIdentity::NightlyWindowsExternalV2,
        version: "2.8.0-nightly.14",
        release_channel: ReleaseChannel::Nightly,
        os_type: OsType::Windows,
        install_method: InstallSource::Unknown,
        engine: Engine::V2,
        session_interface: SessionInterface::ExternalAcp,
        agent_mode: AgentMode::Default,
        model: "claude-haiku-4",
        process_role: ProcessRole::Tui,
        auth_method: AuthMethod::BuilderId,
        auth_flow: AuthFlow::DeviceCode,
        tool_origin: ToolMetricOrigin::Mcp,
        tool_outcome: ToolMetricOutcome::Cancelled,
        execution_context: ExecutionContext::Subagent,
        trust_posture: TrustPosture::PromptOnDemand,
        mcp: McpPlan {
            source: McpServerSource::Registry,
            outcome: McpInitOutcome::Failure,
            server: DemoMcpServer::Github,
        },
        ui_mode: UiMode::Lite,
        commands: CommandPlan {
            slash: SlashCommand::Goal,
            top_level: TopLevelCommand::Update,
        },
        activity: ActivityPlan {
            volume: 3,
            cloud_event: CloudSessionEvent::Detached,
            cloud_ready_seconds: 2.4,
            sample_scale_base: 1.8,
            run_outcome: RunOutcome::Failure,
            goal_outcome: GoalOutcome::ReinjectionFailure,
            render_kind: RenderKind::Partial,
            resources: ResourcePlan {
                rss_mebibytes: 192.0,
                open_file_descriptors: 64.0,
                windows_handles: 120.0,
                threads: 16.0,
                peak_rss_mebibytes: 248.0,
                tui_heap_mebibytes: 120.0,
            },
        },
        diagnostics: DiagnosticPolicy::Cancellation {
            request_duration_seconds: 1.6,
        },
    },
    Scenario {
        identity: ScenarioIdentity::StableLinuxHeadlessV1,
        version: "2.6.3",
        release_channel: ReleaseChannel::Stable,
        os_type: OsType::Linux,
        install_method: InstallSource::Unknown,
        engine: Engine::V1,
        session_interface: SessionInterface::NoninteractiveCli,
        agent_mode: AgentMode::Plan,
        model: "claude-sonnet-3.7",
        process_role: ProcessRole::Host,
        auth_method: AuthMethod::IdentityCenter,
        auth_flow: AuthFlow::Pkce,
        tool_origin: ToolMetricOrigin::Mcp,
        tool_outcome: ToolMetricOutcome::Success,
        execution_context: ExecutionContext::Subagent,
        trust_posture: TrustPosture::TrustAllTools,
        mcp: McpPlan {
            source: McpServerSource::Workspace,
            outcome: McpInitOutcome::Success,
            server: DemoMcpServer::Builder,
        },
        ui_mode: UiMode::Tui,
        commands: CommandPlan {
            slash: SlashCommand::Model,
            top_level: TopLevelCommand::Chat,
        },
        activity: ActivityPlan {
            volume: 7,
            cloud_event: CloudSessionEvent::FellBackLocal,
            cloud_ready_seconds: 2.8,
            sample_scale_base: 2.0,
            run_outcome: RunOutcome::Success,
            goal_outcome: GoalOutcome::AgentError,
            render_kind: RenderKind::Full,
            resources: ResourcePlan {
                rss_mebibytes: 216.0,
                open_file_descriptors: 72.0,
                windows_handles: 132.0,
                threads: 18.0,
                peak_rss_mebibytes: 278.0,
                tui_heap_mebibytes: 132.0,
            },
        },
        diagnostics: DiagnosticPolicy::Failure(FailureDiagnostics {
            error_kind: ErrorKind::Timeout,
            request_duration_seconds: 3.5,
            turn_failure_reason: TurnFailureReason::InternalError,
            crash_kind: CrashKind::Other,
            auth_failure_reason: AuthFailureReason::Configuration,
        }),
    },
    Scenario {
        identity: ScenarioIdentity::DevelopmentUnknown,
        version: "0.0.0-dev",
        release_channel: ReleaseChannel::Other,
        os_type: OsType::Other,
        install_method: InstallSource::Unknown,
        engine: Engine::Unknown,
        session_interface: SessionInterface::InteractiveCli,
        agent_mode: AgentMode::Autonomous,
        model: "unknown",
        process_role: ProcessRole::KasSubprocess,
        auth_method: AuthMethod::Unknown,
        auth_flow: AuthFlow::Unknown,
        tool_origin: ToolMetricOrigin::Unknown,
        tool_outcome: ToolMetricOutcome::Unknown,
        execution_context: ExecutionContext::Main,
        trust_posture: TrustPosture::PromptOnDemand,
        mcp: McpPlan {
            source: McpServerSource::Agent,
            outcome: McpInitOutcome::Unknown,
            server: DemoMcpServer::Unknown,
        },
        ui_mode: UiMode::Lite,
        commands: CommandPlan {
            slash: SlashCommand::Custom,
            top_level: TopLevelCommand::Chat,
        },
        activity: ActivityPlan {
            volume: 2,
            cloud_event: CloudSessionEvent::StartFailed,
            cloud_ready_seconds: 3.2,
            sample_scale_base: 2.2,
            run_outcome: RunOutcome::Failure,
            goal_outcome: GoalOutcome::Unknown,
            render_kind: RenderKind::Partial,
            resources: ResourcePlan {
                rss_mebibytes: 240.0,
                open_file_descriptors: 80.0,
                windows_handles: 144.0,
                threads: 20.0,
                peak_rss_mebibytes: 308.0,
                tui_heap_mebibytes: 144.0,
            },
        },
        diagnostics: DiagnosticPolicy::None,
    },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn scenario_identities_are_unique_and_stable() {
        let identities = SCENARIOS
            .iter()
            .map(|scenario| scenario.identity)
            .collect::<BTreeSet<_>>();
        assert_eq!(identities.len(), SCENARIOS.len());
        assert_eq!(
            SCENARIOS
                .iter()
                .map(|scenario| scenario.identity.as_str())
                .collect::<Vec<_>>(),
            [
                "stable-macos-v1",
                "stable-windows-v2",
                "nightly-linux-v3",
                "beta-macos-external-v3",
                "nightly-windows-external-v2",
                "stable-linux-headless-v1",
                "development-unknown",
            ]
        );
    }

    #[test]
    fn scenario_matrix_preserves_expected_cohorts() {
        assert_eq!(
            SCENARIOS.iter().map(|scenario| scenario.activity.volume).sum::<u64>(),
            35
        );
        assert_eq!(
            cohort_volume(|scenario| {
                scenario.version == "2.7.0"
                    && scenario.engine == Engine::V2
                    && scenario.session_interface == SessionInterface::InteractiveCli
                    && scenario.agent_mode == AgentMode::Plan
            }),
            6
        );
        assert_eq!(
            cohort_volume(|scenario| {
                scenario.version == "2.8.0-nightly.14"
                    && scenario.engine == Engine::V3
                    && scenario.model == "claude-opus-4.1"
            }),
            5
        );
        assert_eq!(
            cohort_volume(|scenario| {
                scenario.engine == Engine::V3
                    && scenario.tool_origin == ToolMetricOrigin::Builtin
                    && scenario.execution_context == ExecutionContext::Subagent
            }),
            9
        );
        assert_eq!(
            cohort_volume(|scenario| {
                scenario.engine == Engine::V3
                    && scenario.mcp.source == McpServerSource::AcpInjected
                    && scenario.mcp.outcome == McpInitOutcome::Failure
            }),
            5
        );
        assert_eq!(
            SCENARIOS
                .iter()
                .filter(|scenario| {
                    scenario.engine == Engine::V3 && scenario.process_role == ProcessRole::KasSubprocess
                })
                .count(),
            1
        );
    }

    #[test]
    fn scenario_vocabularies_cover_each_declared_plan() {
        assert_eq!(
            values(|scenario| scenario.commands.slash.as_str()),
            BTreeSet::from([
                "/custom",
                "/goal",
                "/help",
                "/model",
                "/settings",
                "/upgrade-agent",
                "/workflow-run",
            ])
        );
        assert_eq!(
            values(|scenario| scenario.mcp.server.as_str()),
            BTreeSet::from([
                "aws-docs",
                "builder",
                "custom-acp",
                "filesystem",
                "github",
                "postgres",
                "unknown",
            ])
        );
        assert!(
            SCENARIOS
                .iter()
                .any(|scenario| matches!(scenario.diagnostics, DiagnosticPolicy::Failure(_)))
        );
        assert!(
            SCENARIOS
                .iter()
                .any(|scenario| matches!(scenario.diagnostics, DiagnosticPolicy::Cancellation { .. }))
        );
        assert!(
            SCENARIOS
                .iter()
                .any(|scenario| matches!(scenario.diagnostics, DiagnosticPolicy::TelemetryProtection { .. }))
        );
    }

    fn cohort_volume(predicate: impl Fn(&Scenario) -> bool) -> u64 {
        SCENARIOS
            .iter()
            .filter(|scenario| predicate(scenario))
            .map(|scenario| scenario.activity.volume)
            .sum()
    }

    fn values(selector: impl Fn(&Scenario) -> &'static str) -> BTreeSet<&'static str> {
        SCENARIOS.iter().map(selector).collect()
    }
}
