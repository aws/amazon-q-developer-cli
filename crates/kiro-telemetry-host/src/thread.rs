//! Cross-harness `TelemetryThread`: spawns a tokio task that drains an mpsc
//! channel of [`Event`]s into one or more sinks. The legacy CloudWatch/Toolkit
//! sink is supplied by the caller through [`crate::config::LegacySink`] (V2
//! only); the OTel sink is constructed inline from
//! [`kiro_telemetry::TelemetryConfig`].

use std::sync::Arc;
use std::time::Duration;

use kiro_telemetry::{
    OtelMetricsSink,
    OtelProviders,
    TelemetryClient as OtelTelemetryClient,
    init_otel,
    metric,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio::task::JoinHandle;
use tokio::time::error::Elapsed;
use tracing::trace;

use crate::config::{
    EventEnricher,
    HostConfig,
    OtelEventTranslator,
};
use crate::event::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    EmptyResponseRetryOutcome,
    Event,
    EventType,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    TelemetryResult,
};
use crate::process::ProcessSampler;
use crate::run_receipt::{
    DeferredRunReceiptAcknowledgement,
    RunReceipt,
    RunReceiptStore,
};
use crate::tool_event::ToolUseEventBuilder;

/// Errors produced by [`TelemetryThread`] operations.
#[derive(thiserror::Error, Debug)]
pub enum TelemetryError {
    #[error(transparent)]
    Send(Box<mpsc::error::SendError<Event>>),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Timeout(#[from] Elapsed),
}

impl From<Box<mpsc::error::SendError<Event>>> for TelemetryError {
    fn from(value: Box<mpsc::error::SendError<Event>>) -> Self {
        Self::Send(value)
    }
}

/// Sender half of the telemetry channel; downgrades to a weak handle on clone
/// so the worker shuts down once the original `TelemetryThread` is dropped.
#[derive(Debug)]
pub enum TelemetrySender {
    Strong(mpsc::UnboundedSender<Event>),
    Weak(mpsc::WeakUnboundedSender<Event>),
}

impl TelemetrySender {
    pub fn send(&self, ev: Event) -> Result<(), Box<mpsc::error::SendError<Event>>> {
        match self {
            Self::Strong(sender) => sender.send(ev).map_err(Box::new),
            Self::Weak(sender) => {
                if let Some(sender) = sender.upgrade() {
                    sender.send(ev).map_err(Box::new)
                } else {
                    tracing::error!(
                        "Attempted to send telemetry after telemetry thread has been dropped. Event attempted {:?}",
                        ev
                    );
                    Ok(())
                }
            },
        }
    }
}

impl Clone for TelemetrySender {
    fn clone(&self) -> Self {
        match self {
            Self::Strong(sender) => Self::Weak(sender.downgrade()),
            Self::Weak(sender) => Self::Weak(sender.clone()),
        }
    }
}

/// Cross-harness telemetry thread. Construct via [`TelemetryThread::new`].
#[derive(Debug)]
pub struct TelemetryThread {
    handle: Option<JoinHandle<()>>,
    tx: TelemetrySender,
    run_receipt: Option<RunReceipt>,
}

impl Clone for TelemetryThread {
    fn clone(&self) -> Self {
        Self {
            handle: None,
            tx: self.tx.clone(),
            run_receipt: None,
        }
    }
}

/// Internal helper that owns the OTel client + providers and applies
/// metric/log emission to each event. Translation from `Event` into OTel
/// records is delegated to the caller-supplied [`OtelEventTranslator`].
#[derive(Clone, Debug)]
struct OtelEmitter {
    providers: OtelProviders,
    client: Arc<OtelTelemetryClient>,
    translator: Option<Arc<dyn OtelEventTranslator>>,
}

impl OtelEmitter {
    fn from_config(config: &kiro_telemetry::TelemetryConfig, translator: Option<Arc<dyn OtelEventTranslator>>) -> Self {
        let providers = init_otel(config);
        let client =
            OtelTelemetryClient::new(config.clone()).with_sink(Arc::new(OtelMetricsSink::new(kiro_telemetry::meter())));
        Self {
            providers,
            client: Arc::new(client),
            translator,
        }
    }

    fn exports_enabled(&self) -> bool {
        self.client.config().exports_enabled()
    }

    fn flush(&self) -> bool {
        match self.providers.force_flush() {
            Ok(()) => true,
            Err(err) => {
                trace!(%err, "failed to flush OTel provider");
                false
            },
        }
    }

    fn emit_metric_records(&self, event: &Event) {
        if !self.exports_enabled() {
            return;
        }
        let Some(translator) = self.translator.as_ref() else {
            return;
        };
        let records = translator.metric_records(event);
        if records.is_empty() {
            if let Some(legacy) = event.ty.legacy_event_type() {
                trace!(
                    legacy_event_type = legacy.as_str(),
                    "event maps to OTel log/derived target"
                );
            }
            return;
        }
        let properties = event.metric_log_properties();
        for record in records {
            if let Err(err) = self.client.emit_with_log_properties(record, &properties) {
                trace!(%err, "failed to emit OTel metric record");
            }
        }
    }

    fn emit_records(&self, records: impl IntoIterator<Item = kiro_telemetry::MetricRecord>) {
        if !self.exports_enabled() {
            return;
        }
        for record in records {
            if let Err(err) = self.client.emit(record) {
                trace!(%err, "failed to emit OTel metric record");
            }
        }
    }
}

fn spawn_recovery_flush(
    otel: OtelEmitter,
    acknowledgement: DeferredRunReceiptAcknowledgement,
) -> Option<oneshot::Receiver<()>> {
    let (complete_tx, complete_rx) = oneshot::channel();
    match std::thread::Builder::new()
        .name("kiro-telemetry-recovery".to_string())
        .spawn(move || {
            if otel.flush() {
                acknowledgement.acknowledge();
            }
            let _ = complete_tx.send(());
        }) {
        Ok(_) => Some(complete_rx),
        Err(err) => {
            trace!(%err, "failed to start recovered telemetry flush");
            None
        },
    }
}

impl TelemetryThread {
    pub async fn new(config: HostConfig) -> Result<Self, TelemetryError> {
        let HostConfig {
            otel_config,
            telemetry_enabled,
            legacy_sink,
            otel_translator,
            client_application,
            engine,
            process_identity,
            govcloud_partition,
            ..
        } = config;

        let otel = OtelEmitter::from_config(&otel_config, otel_translator);
        let receipt_store = RunReceiptStore::new(otel_config.state_dir.clone());
        let (run_receipt, recovery_acknowledgement) = if telemetry_enabled && otel.exports_enabled() {
            let recovery = receipt_store.recover();
            let recovered_records = recovery.records().collect::<Vec<_>>();
            otel.emit_records(recovered_records.iter().cloned());
            let recovery_acknowledgement = if recovered_records.is_empty() {
                recovery.acknowledge();
                None
            } else {
                Some(recovery.defer_acknowledgement())
            };
            (
                process_identity.and_then(|identity| receipt_store.start(identity).ok()),
                recovery_acknowledgement,
            )
        } else {
            receipt_store.clear_unlocked();
            (None, None)
        };
        let mut process_sampler = process_identity
            .filter(|_| otel.exports_enabled())
            .and_then(|identity| ProcessSampler::new().map(|sampler| (identity, sampler)));
        let mut process_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(60),
            Duration::from_secs(60),
        );

        let (tx, mut rx) = mpsc::unbounded_channel();
        let tx = TelemetrySender::Strong(tx);

        let recovery_flush =
            recovery_acknowledgement.and_then(|acknowledgement| spawn_recovery_flush(otel.clone(), acknowledgement));
        let has_recovery = recovery_flush.is_some();
        let recovery_monitor = async move {
            if let Some(recovery_flush) = recovery_flush
                && recovery_flush.await.is_err()
            {
                trace!("recovered telemetry flush exited without reporting completion");
            }
        };

        let handle = tokio::spawn(async move {
            tokio::pin!(recovery_monitor);
            let mut recovery_complete = !has_recovery;
            loop {
                tokio::select! {
                    () = &mut recovery_monitor, if !recovery_complete => {
                        recovery_complete = true;
                    },
                    event = rx.recv() => {
                        let Some(mut event) = event else {
                            break;
                        };
                        apply_event_defaults(&mut event, engine, client_application);
                        trace!("TelemetryThread received new telemetry event: {:?}", event);
                        otel.emit_metric_records(&event);
                        if let Some(partition) = govcloud_partition {
                            if let Some(sink) = legacy_sink.as_ref() {
                                sink.send_event_govcloud(event, partition).await;
                            }
                        } else if let Some(sink) = legacy_sink.as_ref() {
                            sink.send_event(event).await;
                        }
                    },
                    _ = process_interval.tick(), if process_sampler.is_some() => {
                        if let Some((identity, sampler)) = process_sampler.as_mut() {
                            otel.emit_records(sampler.sample(*identity));
                        }
                    },
                }
            }

            if !recovery_complete {
                recovery_monitor.await;
            }
            if let Some((identity, sampler)) = process_sampler.as_mut() {
                otel.emit_records(sampler.final_sample(*identity));
            }
            otel.flush();
        });

        Ok(Self {
            handle: Some(handle),
            tx,
            run_receipt,
        })
    }

    pub async fn finish(self) -> Result<(), TelemetryError> {
        self.finish_with_timeout(Duration::from_millis(1000)).await
    }

    pub async fn finish_with_timeout(mut self, timeout: Duration) -> Result<(), TelemetryError> {
        drop(self.tx);
        let result = if let Some(handle) = self.handle.take() {
            let mut handle = handle;
            match tokio::time::timeout(timeout, &mut handle).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(TelemetryError::Join(err)),
                Err(_) => {
                    handle.abort();
                    Ok(())
                },
            }
        } else {
            Ok(())
        };
        if let Some(receipt) = self.run_receipt.take() {
            receipt.complete();
        }
        result
    }

    /// Send a pre-built telemetry event directly.
    pub fn send_event(&self, event: Event) -> Result<(), TelemetryError> {
        Ok(self.tx.send(event)?)
    }

    /// Expose the underlying sender for harness-internal use.
    pub fn sender(&self) -> &TelemetrySender {
        &self.tx
    }

    pub fn send_user_logged_in(&self) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::UserLoggedIn {}))?)
    }

    pub fn send_auth_failed(
        &self,
        auth_method: &str,
        oauth_flow: &str,
        error_type: &str,
        error_code: Option<String>,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::AuthFailed {
            auth_method: auth_method.to_string(),
            oauth_flow: oauth_flow.to_string(),
            error_type: error_type.to_string(),
            error_code,
        }))?)
    }

    pub fn send_did_select_profile(
        &self,
        source: crate::event::QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: crate::event::TelemetryResult,
        sso_region: Option<String>,
        profile_count: Option<i64>,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::DidSelectProfile {
            source,
            amazonq_profile_region,
            result,
            sso_region,
            profile_count,
        }))?)
    }

    pub fn send_profile_state(
        &self,
        source: crate::event::QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: crate::event::TelemetryResult,
        sso_region: Option<String>,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::ProfileState {
            source,
            amazonq_profile_region,
            result,
            sso_region,
        }))?)
    }

    pub fn send_subagent_invocation(
        &self,
        parent_conversation_id: String,
        subagent_name: String,
        builtin_tool_uses: u32,
        mcp_tool_uses: u32,
        parent_tool_use_id: String,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::SubagentInvocation {
            parent_conversation_id,
            subagent_name,
            builtin_tool_uses,
            mcp_tool_uses,
            parent_tool_use_id,
        }))?)
    }

    pub fn send_subagent_record_user_turn_completion(
        &self,
        conversation_id: String,
        result: crate::event::TelemetryResult,
        args: crate::event::RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        }))?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_goal_completed(
        &self,
        conversation_id: Option<String>,
        terminal_state: String,
        iterations: i64,
        max_iterations: i64,
        duration_sec: i64,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::GoalCompleted {
            conversation_id,
            terminal_state,
            iterations,
            max_iterations,
            duration_sec,
        }))?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_process_health_snapshot(
        &self,
        agent_kind: metric::AgentKind,
        rss_mb: f64,
        heap_used_mb: f64,
        peak_rss_mb: f64,
        cpu_user_pct: f64,
        cpu_system_pct: f64,
        last_render_ms: f64,
        max_render_ms: f64,
        renders_per_min: i64,
        full_redraws_per_min: i64,
        yoga_node_count: i64,
        event_loop_p99_ms: Option<f64>,
        input_latency_p95_ms: Option<f64>,
        session_duration_sec: i64,
        cpu_cores: i64,
        total_memory_mb: i64,
        terminal: String,
        session_id: Option<String>,
        version: String,
        platform: String,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::ProcessHealthMetric {
            agent_kind,
            rss_mb,
            heap_used_mb,
            peak_rss_mb,
            cpu_user_pct,
            cpu_system_pct,
            last_render_ms,
            max_render_ms,
            renders_per_min,
            full_redraws_per_min,
            yoga_node_count,
            event_loop_p99_ms,
            input_latency_p95_ms,
            session_duration_sec,
            cpu_cores,
            total_memory_mb,
            terminal,
            session_id,
            version,
            platform,
        }))?)
    }

    pub fn send_mode_changed(
        &self,
        from_mode: String,
        to_mode: String,
        source: crate::event::ModeChangeSource,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::ModeChanged {
            from_mode,
            to_mode,
            source,
            session_id,
        }))?)
    }

    // ----------------------------------------------------------------------
    // Metadata-aware send helpers.
    //
    // Each helper builds the corresponding [`Event`], runs the optional
    // [`EventEnricher`] closure on it (which V2 implements as a closure
    // capturing `Arc<Database>`), and forwards it via [`Self::send_event`].
    // V3 / kiro-bot / tests pass `None` and skip enrichment.
    // ----------------------------------------------------------------------

    pub async fn send_cli_subcommand_executed(
        &self,
        enricher: Option<&EventEnricher>,
        subcommand_name: String,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::CliSubcommandExecuted {
            subcommand: subcommand_name,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    pub async fn send_chat_slash_command_executed(
        &self,
        enricher: Option<&EventEnricher>,
        conversation_id: String,
        command: String,
        subcommand: Option<String>,
        result: TelemetryResult,
        reason: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::ChatSlashCommandExecuted {
            conversation_id,
            command,
            subcommand,
            result,
            reason,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_agent_contribution_metric(
        &self,
        enricher: Option<&EventEnricher>,
        conversation_id: String,
        utterance_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        lines_by_agent: Option<isize>,
        lines_by_user: Option<isize>,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::AgentContribution {
            conversation_id,
            utterance_id,
            tool_use_id,
            tool_name,
            lines_by_agent,
            lines_by_user,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    pub async fn send_chat_added_message(
        &self,
        enricher: Option<&EventEnricher>,
        conversation_id: String,
        result: TelemetryResult,
        data: ChatAddedMessageParams,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::ChatAddedMessage {
            conversation_id,
            result,
            data,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    pub async fn send_record_user_turn_completion(
        &self,
        enricher: Option<&EventEnricher>,
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_metering_event(
        &self,
        enricher: Option<&EventEnricher>,
        request_id: Option<String>,
        model: Option<String>,
        usage: f64,
        unit: String,
        unit_plural: String,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::MeteringEvent {
            request_id,
            model,
            usage,
            unit,
            unit_plural,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    pub async fn send_empty_response_retry(
        &self,
        enricher: Option<&EventEnricher>,
        model: Option<String>,
        outcome: EmptyResponseRetryOutcome,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::EmptyResponseRetry { model, outcome });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    pub async fn send_tangent_mode_session(
        &self,
        enricher: Option<&EventEnricher>,
        conversation_id: String,
        result: TelemetryResult,
        args: TangentModeSessionArgs,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::TangentModeSession {
            conversation_id,
            result,
            args,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    pub async fn send_tool_use_suggested(
        &self,
        enricher: Option<&EventEnricher>,
        event: ToolUseEventBuilder,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::ToolUseSuggested {
            conversation_id: event.conversation_id,
            utterance_id: event.utterance_id,
            user_input_id: event.user_input_id,
            tool_use_id: event.tool_use_id,
            tool_name: event.tool_name,
            mcp_server_name: event.mcp_server_name,
            is_accepted: event.is_accepted,
            is_trusted: event.is_trusted,
            is_success: event.is_success,
            reason_desc: event.reason_desc,
            is_valid: event.is_valid,
            is_custom_tool: event.is_custom_tool,
            input_token_size: event.input_token_size,
            output_token_size: event.output_token_size,
            custom_tool_call_latency: event.custom_tool_call_latency,
            model: event.model,
            execution_duration: event.execution_duration,
            turn_duration: event.turn_duration,
            aws_service_name: event.aws_service_name,
            aws_operation_name: event.aws_operation_name,
        });
        enrich(enricher, &mut telemetry_event).await;
        self.send_event(telemetry_event)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_mcp_server_init(
        &self,
        enricher: Option<&EventEnricher>,
        conversation_id: String,
        server_name: String,
        mcp_server_source: metric::McpServerSource,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
        all_tool_names: Option<String>,
        loaded_tool_names: Option<String>,
        all_tools_count: usize,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::McpServerInit {
            conversation_id,
            server_name,
            mcp_server_source,
            init_failure_reason,
            number_of_tools,
            all_tool_names,
            loaded_tool_names,
            all_tools_count,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    pub async fn send_agent_config_init(
        &self,
        enricher: Option<&EventEnricher>,
        conversation_id: String,
        args: AgentConfigInitArgs,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::AgentConfigInit { conversation_id, args });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_response_error(
        &self,
        enricher: Option<&EventEnricher>,
        conversation_id: String,
        context_file_length: Option<usize>,
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        status_code: Option<u16>,
        request_id: Option<String>,
        message_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::MessageResponseError {
            result,
            reason,
            reason_desc,
            status_code,
            conversation_id,
            context_file_length,
            request_id,
            message_id,
            model: None,
        });
        enrich(enricher, &mut event).await;
        self.send_event(event)
    }

    pub fn send_ui_mode_session_start(
        &self,
        ui_mode: String,
        ui_mode_source: crate::event::UiModeSource,
        ui_mode_default: String,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let event = Event::new(EventType::UiModeSessionStart {
            ui_mode,
            ui_mode_source,
            ui_mode_default,
            session_id,
        });
        self.send_event(event)
    }

    pub fn send_ui_mode_changed(
        &self,
        from: String,
        to: String,
        source: crate::event::ModeChangeSource,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let event = Event::new(EventType::UiModeChanged {
            from,
            to,
            source,
            session_id,
        });
        self.send_event(event)
    }

    pub fn send_ui_mode_default_changed(
        &self,
        from: String,
        to: String,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let event = Event::new(EventType::UiModeDefaultChanged { from, to, session_id });
        self.send_event(event)
    }
}

fn apply_event_defaults(
    event: &mut Event,
    engine: Option<metric::Engine>,
    client_application: Option<metric::ClientApplication>,
) {
    if event.engine.is_none()
        && let Some(engine) = engine
    {
        event.set_engine(engine);
    }
    if event.client_application.is_none()
        && let Some(client_application) = client_application
    {
        event.set_client_application_kind(client_application);
    }
}

/// Run the optional [`EventEnricher`] closure on an event, if provided.
async fn enrich(enricher: Option<&EventEnricher>, event: &mut Event) {
    if let Some(enricher) = enricher {
        enricher(event).await;
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::mpsc as std_mpsc;
    use std::time::Instant;

    use kiro_telemetry::{
        OtelMode,
        TelemetryConfig,
    };

    use super::*;
    use crate::config::govcloud_partition;

    #[tokio::test]
    async fn default_host_config_runs_without_sinks() {
        let thread = TelemetryThread::new(HostConfig::default()).await.unwrap();
        thread.send_user_logged_in().unwrap();
        thread.finish().await.unwrap();
    }

    #[tokio::test]
    async fn cloned_thread_can_finish_before_original() {
        let thread = TelemetryThread::new(HostConfig::default()).await.unwrap();
        let clone = thread.clone();
        clone.finish().await.unwrap();
        thread.finish().await.unwrap();
    }

    #[test]
    fn recovered_receipt_flush_does_not_block_thread_startup() {
        let state = tempfile::tempdir().unwrap();
        let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
        let store = RunReceiptStore::new(state.path());
        drop(store.start(identity).unwrap());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (respond_tx, respond_rx) = std_mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            respond_rx.recv().unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .unwrap();
        });
        let config = HostConfig {
            telemetry_enabled: true,
            otel_config: TelemetryConfig::new(true, OtelMode::OtelOnly, Some(endpoint), state.path().to_path_buf()),
            ..HostConfig::default()
        };
        let (started_tx, started_rx) = std_mpsc::channel();
        let runtime = std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let thread = TelemetryThread::new(config).await.unwrap();
                    started_tx.send(()).unwrap();
                    thread.finish().await.unwrap();
                });
        });

        let startup_result = started_rx.recv_timeout(Duration::from_secs(2));
        respond_tx.send(()).unwrap();
        server.join().unwrap();
        runtime.join().unwrap();

        assert!(startup_result.is_ok(), "recovery export blocked telemetry startup");
    }

    #[test]
    fn blocked_recovery_flush_does_not_hold_runtime_shutdown() {
        let state = tempfile::tempdir().unwrap();
        let identity = crate::process::ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
        let store = RunReceiptStore::new(state.path());
        drop(store.start(identity).unwrap());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (accepted_tx, accepted_rx) = std_mpsc::channel();
        let (release_tx, release_rx) = std_mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            accepted_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .unwrap();
        });
        let config = HostConfig {
            telemetry_enabled: true,
            otel_config: TelemetryConfig::new(true, OtelMode::OtelOnly, Some(endpoint), state.path().to_path_buf()),
            process_identity: Some(identity),
            ..HostConfig::default()
        };
        let (finished_tx, finished_rx) = std_mpsc::channel();
        let runtime_thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = runtime.block_on(async move {
                let thread = TelemetryThread::new(config).await.unwrap();
                thread.finish_with_timeout(Duration::from_millis(100)).await
            });
            drop(runtime);
            finished_tx.send(result).unwrap();
        });

        accepted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let started_waiting = Instant::now();
        let result = finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        assert!(result.is_ok());
        assert!(started_waiting.elapsed() < Duration::from_secs(1));

        release_tx.send(()).unwrap();
        server.join().unwrap();
        runtime_thread.join().unwrap();

        let receipt_directory = state.path().join("run-receipts");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline
            && std::fs::read_dir(&receipt_directory)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .any(|entry| entry.path().extension().is_some_and(|extension| extension == "json"))
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            std::fs::read_dir(receipt_directory)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .all(|entry| entry.path().extension().is_none_or(|extension| extension != "json"))
        );
    }

    #[tokio::test]
    async fn join_failure_still_completes_run_receipt() {
        let state = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(state.path());
        let receipt = store
            .start(crate::process::ProcessIdentity::new(
                metric::Engine::V2,
                metric::ProcessRole::Host,
            ))
            .unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();
        let thread = TelemetryThread {
            handle: Some(tokio::spawn(async { panic!("test worker failure") })),
            tx: TelemetrySender::Strong(tx),
            run_receipt: Some(receipt),
        };

        let result = thread.finish().await;

        assert!(matches!(result, Err(TelemetryError::Join(_))));
        assert_eq!(store.recover().records().count(), 0);
    }

    #[test]
    fn host_defaults_do_not_override_event_attribution() {
        let mut event = Event::new(EventType::UserLoggedIn {});
        event.set_engine(metric::Engine::V3);
        event.set_client_application_kind(metric::ClientApplication::ChatCliV3);

        apply_event_defaults(
            &mut event,
            Some(metric::Engine::V2),
            Some(metric::ClientApplication::ChatCliV2),
        );

        assert_eq!(event.engine, Some(metric::Engine::V3));
        assert_eq!(event.client_application.as_deref(), Some("chat_cli_v3"));
    }

    #[test]
    fn host_defaults_fill_missing_event_attribution() {
        let mut event = Event::new(EventType::UserLoggedIn {});

        apply_event_defaults(
            &mut event,
            Some(metric::Engine::V2),
            Some(metric::ClientApplication::ChatCliV2),
        );

        assert_eq!(event.engine, Some(metric::Engine::V2));
        assert_eq!(event.client_application.as_deref(), Some("chat_cli_v2"));
    }

    #[test]
    fn govcloud_partition_detects_gov_regions() {
        assert_eq!(govcloud_partition("us-gov-east-1"), Some("aws-us-gov"));
        assert_eq!(govcloud_partition("us-gov-west-1"), Some("aws-us-gov"));
        assert_eq!(govcloud_partition("us-east-1"), None);
    }
}
