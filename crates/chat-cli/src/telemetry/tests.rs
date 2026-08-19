use amzn_codewhisperer_client::types::{
    ChatAddMessageEvent,
    IdeCategory,
    OperatingSystem,
    TelemetryEvent,
};
use kiro_telemetry::OtelMode;
use uuid::uuid;

use super::legacy_sink::{
    PRODUCT,
    PRODUCT_VERSION,
    should_build_codewhisperer_telemetry_client,
    should_build_toolkit_telemetry_client,
};
use super::*;

#[tokio::test]
async fn client_context() {
    let mut database = Database::new_default().await.unwrap();
    let client = TelemetryClient::new(
        &Env::new(),
        &Fs::new(),
        &mut database,
        None,
        uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff"),
        false,
    )
    .await
    .unwrap();
    let context = client.user_context().unwrap();

    assert_eq!(context.ide_category, IdeCategory::Cli);
    assert!(matches!(
        context.operating_system,
        OperatingSystem::Linux | OperatingSystem::Mac | OperatingSystem::Windows
    ));
    assert_eq!(context.product, PRODUCT);
    assert_eq!(
        context.client_id,
        Some(uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff").hyphenated().to_string())
    );
    assert_eq!(context.ide_version.as_deref(), Some(PRODUCT_VERSION));
}

#[test]
fn otel_config_disables_exports_when_telemetry_is_disabled() {
    let env = Env::from_slice(&[(KIRO_TELEMETRY_OTEL, "1")]);
    let client_id = uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e");
    let config = otel_telemetry_config(&env, false, client_id, None);

    assert!(!config.exports_enabled());
}

#[test]
fn otel_config_parses_env_controls() {
    let env = Env::from_slice(&[
        (KIRO_TELEMETRY_OTEL, "1"),
        (
            KIRO_TELEMETRY_OTLP_ENDPOINT,
            "https://prod.us-east-1.telemetry-v2.kiro.dev",
        ),
    ]);
    let client_id = uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e");
    let config = otel_telemetry_config(&env, true, client_id, Some("eu-central-1"));

    assert_eq!(config.otel_mode, OtelMode::DualWrite);
    assert!(config.exports_enabled());
    assert_eq!(config.machine_id, client_id.hyphenated().to_string());
    assert_eq!(config.deployment_environment, "prod");
    assert_eq!(
        config.otlp_endpoint.as_deref(),
        Some("https://prod.us-east-1.telemetry-v2.kiro.dev")
    );
}

#[test]
fn otel_config_defaults_to_regional_kuts_endpoint() {
    let env = Env::from_slice(&[(KIRO_TELEMETRY_OTEL, "2")]);
    let config = otel_telemetry_config(
        &env,
        true,
        uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
        Some("eu-central-1"),
    );

    assert!(config.exports_enabled());
    assert_eq!(
        config.otlp_endpoint.as_deref(),
        Some("https://prod.eu-central-1.telemetry-v2.kiro.dev")
    );
}

#[test]
fn cli_session_started_event_sets_launch_dimensions() {
    let event = cli_session_started_event(
        metric::ClientApplication::ChatCliV3,
        metric::SessionInterface::InteractiveCli,
        metric::Engine::V3,
    );

    assert_eq!(event.client_application.as_deref(), Some("chat_cli_v3"));
    assert_eq!(event.session_interface, Some(metric::SessionInterface::InteractiveCli));
    assert_eq!(event.engine, Some(metric::Engine::V3));
    match event.ty {
        EventType::CliSessionStarted {
            os_type,
            install_source,
        } => {
            assert!(matches!(
                os_type,
                metric::OsType::Linux | metric::OsType::Macos | metric::OsType::Windows | metric::OsType::Other
            ));
            assert!(matches!(
                install_source,
                metric::InstallSource::Brew | metric::InstallSource::Internal | metric::InstallSource::Unknown
            ));
        },
        _ => panic!("expected CLI session-started event"),
    }
}

#[test]
fn cli_session_completed_event_sets_exit_dimensions() {
    let event = cli_session_completed_event(
        metric::ExitReason::Clean,
        metric::AgentKind::Kas,
        metric::SessionInterface::InteractiveCli,
        metric::Engine::V3,
        metric::RunOutcome::Success,
    );

    assert_eq!(event.session_interface, Some(metric::SessionInterface::InteractiveCli));
    assert_eq!(event.engine, Some(metric::Engine::V3));
    assert_eq!(event.metric_context.run_outcome, Some(metric::RunOutcome::Success));
    match event.ty {
        EventType::CliSessionCompleted {
            exit_reason,
            agent_kind,
        } => {
            assert_eq!(exit_reason, metric::ExitReason::Clean);
            assert_eq!(agent_kind, metric::AgentKind::Kas);
        },
        _ => panic!("expected CLI session-completed event"),
    }
}

#[test]
fn chat_session_started_round_trips_typed_agent_modes() {
    for agent_mode in [metric::AgentMode::Spec, metric::AgentMode::Autonomous] {
        let event = chat_session_started_event(
            metric::SessionInterface::NoninteractiveCli,
            agent_mode,
            metric::Engine::V3,
            metric::TrustPosture::PromptOnDemand,
        );
        let record = kiro_telemetry_legacy::event_to_otel_metric_record(&event).unwrap();
        let recorded_agent_mode = record
            .attributes
            .iter()
            .find(|attribute| attribute.key == "agent_mode")
            .map(|attribute| attribute.value.as_str());

        assert_eq!(recorded_agent_mode, Some(agent_mode.as_str()));
    }
}

#[test]
fn govcloud_partition_detects_gov_regions() {
    assert_eq!(govcloud_partition(US_GOV_EAST), Some("aws-us-gov"));
    assert_eq!(govcloud_partition(US_GOV_WEST), Some("aws-us-gov"));
    assert_eq!(govcloud_partition("us-east-1"), None);
}

#[test]
fn legacy_sink_feature_flags_control_client_construction() {
    assert_eq!(
        should_build_toolkit_telemetry_client(true, None),
        cfg!(feature = "legacy_toolkit_sink")
    );
    assert!(!should_build_toolkit_telemetry_client(false, None));
    assert!(!should_build_toolkit_telemetry_client(true, Some("aws-us-gov")));
    assert_eq!(
        should_build_codewhisperer_telemetry_client(),
        cfg!(feature = "legacy_codewhisperer_sink")
    );
}

#[tokio::test]
async fn cloned_telemetry_thread_can_finish_before_original() {
    let mut database = Database::new_default().await.unwrap();
    let env = Env::from_slice(&[(KIRO_TELEMETRY_OTEL, "0")]);
    let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, false)
        .await
        .unwrap();
    let clone = thread.clone();

    assert_eq!(clone.client_id(), thread.client_id());
    clone.finish().await.unwrap();
    thread.finish().await.unwrap();
}

#[derive(Debug, Default)]
struct CapturingLegacySink {
    events: Mutex<Vec<Event>>,
}

impl kiro_telemetry_host::LegacySink for CapturingLegacySink {
    fn send_event(&self, event: Event) -> futures::future::BoxFuture<'_, ()> {
        self.events.lock().unwrap().push(event);
        Box::pin(async {})
    }

    fn send_event_govcloud(&self, event: Event, _partition: &'static str) -> futures::future::BoxFuture<'_, ()> {
        self.send_event(event)
    }
}

async fn telemetry_thread_with_capturing_sink() -> (TelemetryThread, Arc<CapturingLegacySink>) {
    let sink = Arc::new(CapturingLegacySink::default());
    let host = kiro_telemetry_host::TelemetryThread::new(kiro_telemetry_host::HostConfig {
        legacy_sink: Some(sink.clone()),
        ..kiro_telemetry_host::HostConfig::default()
    })
    .await
    .unwrap();
    let identity_epochs = host.identity_epochs();
    (
        TelemetryThread {
            host: Arc::new(Mutex::new(Some(host))),
            identity_epochs,
            client_id: uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            enabled: true,
            startup_state: Arc::new(AtomicU8::new(STARTUP_PENDING)),
        },
        sink,
    )
}

#[tokio::test]
async fn startup_terminal_events_are_mutually_exclusive() {
    let (successful, successful_sink) = telemetry_thread_with_capturing_sink().await;
    successful
        .send_startup_duration(1.0, metric::SessionInterface::InteractiveCli, metric::Engine::V2)
        .unwrap();
    successful
        .send_startup_failure(
            metric::SessionInterface::InteractiveCli,
            metric::Engine::V2,
            metric::StartupFailureStage::InterfaceInit,
        )
        .unwrap();
    successful.finish().await.unwrap();
    assert!(matches!(successful_sink.events.lock().unwrap().as_slice(), [Event {
        ty: EventType::StartupDuration { .. },
        ..
    }]));

    let (failed, failed_sink) = telemetry_thread_with_capturing_sink().await;
    failed
        .send_startup_failure(
            metric::SessionInterface::NoninteractiveCli,
            metric::Engine::V1,
            metric::StartupFailureStage::RuntimeSetup,
        )
        .unwrap();
    failed
        .send_startup_duration(1.0, metric::SessionInterface::NoninteractiveCli, metric::Engine::V1)
        .unwrap();
    failed.finish().await.unwrap();
    assert!(matches!(failed_sink.events.lock().unwrap().as_slice(), [Event {
        ty: EventType::StartupFailure { .. },
        ..
    }]));
}

#[test]
fn tool_use_event_normalizes_delegation_and_execution_context() {
    let mut event = tool_use_suggested_event(
        ToolUseEventBuilder::new(
            "conversation".to_string(),
            "tool-use".to_string(),
            Some("model".to_string()),
        )
        .set_tool_name("subagent".to_string()),
        metric::ExecutionContext::Subagent,
    );

    prepare_v1_event(&mut event);

    assert!(event.is_subagent);
    assert_eq!(
        event.metric_context.canonical_tool_name.as_deref(),
        Some("use_subagent")
    );

    let event = tool_use_suggested_event(
        ToolUseEventBuilder::new("conversation".to_string(), "tool-use".to_string(), None),
        metric::ExecutionContext::Main,
    );
    assert!(!event.is_subagent);
}

#[tracing_test::traced_test]
#[tokio::test]
#[ignore = "needs auth which is not in CI"]
async fn test_send() {
    let mut database = Database::new_default().await.unwrap();
    let env = Env::new();
    let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, false)
        .await
        .unwrap();
    thread.send_user_logged_in(&database).await.ok();
    drop(thread);

    assert!(!logs_contain("ERROR"));
    assert!(!logs_contain("error"));
    assert!(!logs_contain("WARN"));
    assert!(!logs_contain("warn"));
    assert!(!logs_contain("Failed to post metric"));
}

#[tracing_test::traced_test]
#[tokio::test]
#[ignore = "needs auth which is not in CI"]
async fn test_all_telemetry() {
    let mut database = Database::new_default().await.unwrap();
    let env = Env::new();
    let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, false)
        .await
        .unwrap();

    thread.send_user_logged_in(&database).await.ok();
    thread
        .send_cli_subcommand_executed(&database, "version".to_string())
        .await
        .ok();
    thread
        .send_chat_added_message(
            &database,
            "conv_id".to_owned(),
            TelemetryResult::Succeeded,
            ChatAddedMessageParams {
                message_id: Some("message_id".to_owned()),
                context_file_length: Some(123),
                ..Default::default()
            },
        )
        .await
        .ok();

    drop(thread);

    assert!(!logs_contain("ERROR"));
    assert!(!logs_contain("error"));
    assert!(!logs_contain("WARN"));
    assert!(!logs_contain("warn"));
    assert!(!logs_contain("Failed to post metric"));
}

#[tokio::test]
#[ignore = "needs auth which is not in CI"]
async fn test_without_optout() {
    let mut database = Database::new_default().await.unwrap();
    let client = TelemetryClient::new(
        &Env::new(),
        &Fs::new(),
        &mut database,
        None,
        uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff"),
        false,
    )
    .await
    .unwrap();
    client
        .codewhisperer_client
        .as_ref()
        .expect("cw telemetry client should exist")
        .send_telemetry_event(
            TelemetryEvent::ChatAddMessageEvent(
                ChatAddMessageEvent::builder()
                    .conversation_id("debug".to_owned())
                    .message_id("debug".to_owned())
                    .build()
                    .unwrap(),
            ),
            client.user_context().unwrap(),
            false,
            Some("model".to_owned()),
        )
        .await
        .unwrap();
}
