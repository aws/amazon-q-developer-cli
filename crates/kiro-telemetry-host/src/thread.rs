//! Cross-harness `TelemetryThread`: spawns a tokio task that drains an mpsc
//! channel of [`Event`]s into one or more sinks. The legacy CloudWatch/Toolkit
//! sink is supplied by the caller through [`crate::config::LegacySink`]; the
//! OTel sink is constructed inline from [`kiro_telemetry::TelemetryConfig`].

use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use kiro_telemetry::{
    IdentityEpochs,
    OtelMetricsSink,
    OtelProviders,
    TelemetryClient as OtelTelemetryClient,
    export_metric_records_once,
    init_otel,
    metric,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio::task::JoinHandle;
use tokio::time::error::Elapsed;
use tracing::{
    error,
    trace,
};

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
use crate::process::{
    ProcessIdentity,
    ProcessSampler,
};
use crate::run_receipt::{
    DeferredRunReceiptAcknowledgement,
    RunReceipt,
    RunReceiptStore,
};
use crate::tool_event::ToolUseEventBuilder;

const MAX_POST_ABORT_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);

/// Errors produced by [`TelemetryThread`] operations.
#[derive(thiserror::Error, Debug)]
pub enum TelemetryError {
    #[error("failed to enqueue telemetry message")]
    Send,
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Timeout(#[from] Elapsed),
}

// Keep events inline on the hot path rather than adding a heap allocation per datapoint.
#[allow(clippy::large_enum_variant)]
enum TelemetryMessage {
    Event { event: Event, epoch: u64 },
}

impl std::fmt::Debug for TelemetryMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Event { epoch, .. } => f.debug_struct("Event").field("epoch", epoch).finish(),
        }
    }
}

/// Sender half of the telemetry channel; downgrades to a weak handle on clone
/// so the worker shuts down once the original `TelemetryThread` is dropped.
#[derive(Debug)]
pub struct TelemetrySender {
    inner: TelemetrySenderInner,
    identity_epochs: Arc<IdentityEpochs>,
}

#[derive(Debug)]
enum TelemetrySenderInner {
    Strong(mpsc::UnboundedSender<TelemetryMessage>),
    Weak(mpsc::WeakUnboundedSender<TelemetryMessage>),
}

impl TelemetrySender {
    #[cfg(test)]
    fn strong(sender: mpsc::UnboundedSender<TelemetryMessage>) -> Self {
        Self::strong_with_identity(sender, Arc::new(IdentityEpochs::from_persisted(None)))
    }

    fn strong_with_identity(
        sender: mpsc::UnboundedSender<TelemetryMessage>,
        identity_epochs: Arc<IdentityEpochs>,
    ) -> Self {
        Self {
            inner: TelemetrySenderInner::Strong(sender),
            identity_epochs,
        }
    }

    pub fn send(&self, event: Event) -> Result<(), TelemetryError> {
        self.send_message(TelemetryMessage::Event {
            event,
            epoch: self.identity_epochs.current(),
        })
    }

    fn send_message(&self, message: TelemetryMessage) -> Result<(), TelemetryError> {
        match &self.inner {
            TelemetrySenderInner::Strong(sender) => sender.send(message).map_err(|_send_error| TelemetryError::Send),
            TelemetrySenderInner::Weak(sender) => {
                if let Some(sender) = sender.upgrade() {
                    sender.send(message).map_err(|_send_error| TelemetryError::Send)
                } else {
                    tracing::error!("Attempted to send telemetry after telemetry thread was dropped");
                    Ok(())
                }
            },
        }
    }
}

impl Clone for TelemetrySender {
    fn clone(&self) -> Self {
        let inner = match &self.inner {
            TelemetrySenderInner::Strong(sender) => TelemetrySenderInner::Weak(sender.downgrade()),
            TelemetrySenderInner::Weak(sender) => TelemetrySenderInner::Weak(sender.clone()),
        };
        Self {
            inner,
            identity_epochs: Arc::clone(&self.identity_epochs),
        }
    }
}

/// Cross-harness telemetry thread. Construct via [`TelemetryThread::new`].
#[derive(Debug)]
pub struct TelemetryThread {
    handle: Option<JoinHandle<()>>,
    legacy_handle: Option<JoinHandle<()>>,
    tx: TelemetrySender,
    identity_epochs: Arc<IdentityEpochs>,
    process_identity: Arc<Mutex<Option<ProcessIdentity>>>,
    run_receipt_store: Option<RunReceiptStore>,
    run_receipt: Option<Arc<Mutex<Option<RunReceipt>>>>,
}

impl Clone for TelemetryThread {
    fn clone(&self) -> Self {
        Self {
            handle: None,
            legacy_handle: None,
            tx: self.tx.clone(),
            identity_epochs: Arc::clone(&self.identity_epochs),
            process_identity: Arc::clone(&self.process_identity),
            run_receipt_store: None,
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
            OtelTelemetryClient::new(config.clone()).with_sink(Arc::new(OtelMetricsSink::new(providers.meter())));
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
                error!(%err, "failed to flush OTel provider");
                false
            },
        }
    }

    fn emit_metric_records(&self, event: &Event, user_id: Option<&str>) {
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
            if let Err(err) =
                self.client
                    .emit_with_log_properties_and_pseudonymous_user_id(record, &properties, user_id)
            {
                trace!(%err, "failed to emit OTel metric record");
            }
        }
    }

    fn emit_records(&self, records: impl IntoIterator<Item = kiro_telemetry::MetricRecord>, user_id: Option<&str>) {
        if !self.exports_enabled() {
            return;
        }
        for record in records {
            if let Err(err) = self.client.emit_with_pseudonymous_user_id(record, user_id) {
                trace!(%err, "failed to emit OTel metric record");
            }
        }
    }
}

struct FlushRequest {
    flush: Box<dyn FnOnce() -> bool + Send + 'static>,
    on_success: Box<dyn FnOnce() + Send + 'static>,
    complete: oneshot::Sender<bool>,
}

#[derive(Debug)]
struct FlushWorker {
    tx: std::sync::mpsc::Sender<FlushRequest>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FlushOutcome {
    Succeeded,
    Failed,
    NotAccepted,
    Abandoned,
}

impl FlushWorker {
    fn spawn() -> Result<Self, std::io::Error> {
        Self::spawn_with(|worker| {
            std::thread::Builder::new()
                .name("kiro-telemetry-flush".to_string())
                .spawn(worker)
                .map(drop)
        })
    }

    fn spawn_with(
        spawn: impl FnOnce(Box<dyn FnOnce() + Send + 'static>) -> Result<(), std::io::Error>,
    ) -> Result<Self, std::io::Error> {
        let (tx, rx) = std::sync::mpsc::channel::<FlushRequest>();
        spawn(Box::new(move || {
            while let Ok(request) = rx.recv() {
                let succeeded = (request.flush)();
                if succeeded {
                    (request.on_success)();
                }
                let _ = request.complete.send(succeeded);
            }
        }))?;
        Ok(Self { tx })
    }

    fn flush<F, G>(&self, flush: F, on_success: G) -> Option<oneshot::Receiver<bool>>
    where
        F: FnOnce() -> bool + Send + 'static,
        G: FnOnce() + Send + 'static,
    {
        let (complete, receiver) = oneshot::channel();
        let request = FlushRequest {
            flush: Box::new(flush),
            on_success: Box::new(on_success),
            complete,
        };
        if let Err(err) = self.tx.send(request) {
            error!(%err, "telemetry flush worker stopped before accepting request");
            return None;
        }
        Some(receiver)
    }
}

async fn request_flush<F, G>(worker: &FlushWorker, flush: F, on_success: G) -> FlushOutcome
where
    F: FnOnce() -> bool + Send + 'static,
    G: FnOnce() + Send + 'static,
{
    let Some(completion) = worker.flush(flush, on_success) else {
        return FlushOutcome::NotAccepted;
    };
    match completion.await {
        Ok(true) => FlushOutcome::Succeeded,
        Ok(false) => FlushOutcome::Failed,
        Err(err) => {
            error!(%err, "telemetry flush worker stopped before reporting completion");
            FlushOutcome::Abandoned
        },
    }
}

struct PendingRecovery {
    acknowledgement: DeferredRunReceiptAcknowledgement,
    records: Vec<kiro_telemetry::MetricRecord>,
}

type SharedRecovery = Arc<std::sync::Mutex<Option<PendingRecovery>>>;

fn acknowledge_recovery(recovery: Option<&SharedRecovery>) -> impl FnOnce() + Send + 'static {
    let recovery = recovery.cloned();
    move || {
        let Some(recovery) = recovery else {
            return;
        };
        let Ok(mut recovery) = recovery.lock() else {
            return;
        };
        let acknowledgement = recovery.take().map(|pending| pending.acknowledgement);
        drop(recovery);
        if let Some(acknowledgement) = acknowledgement {
            acknowledgement.acknowledge();
        }
    }
}

fn export_recovery(
    config: kiro_telemetry::TelemetryConfig,
    recovery: Option<&SharedRecovery>,
) -> impl FnOnce() -> bool + Send + 'static {
    let recovery = recovery.cloned();
    move || {
        let records = recovery.and_then(|recovery| {
            let recovery = recovery.lock().ok()?;
            recovery.as_ref().map(|recovery| recovery.records.clone())
        });
        let Some(records) = records else {
            return true;
        };
        match export_metric_records_once(&config, records) {
            Ok(()) => true,
            Err(err) => {
                error!(%err, "failed to export recovered telemetry");
                false
            },
        }
    }
}

impl TelemetryThread {
    pub async fn new(config: HostConfig) -> Result<Self, TelemetryError> {
        Self::new_with_flush_worker(config, FlushWorker::spawn).await
    }

    async fn new_with_flush_worker(
        config: HostConfig,
        spawn_flush_worker: impl FnOnce() -> Result<FlushWorker, std::io::Error>,
    ) -> Result<Self, TelemetryError> {
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

        let identity_epochs = Arc::new(IdentityEpochs::from_persisted(otel_config.user_id.as_deref()));
        let worker_identity_epochs = Arc::clone(&identity_epochs);
        let otel = OtelEmitter::from_config(&otel_config, otel_translator);
        let flush_worker = if otel.exports_enabled() {
            match spawn_flush_worker() {
                Ok(worker) => Some(worker),
                Err(err) => {
                    error!(%err, "failed to start telemetry flush worker");
                    None
                },
            }
        } else {
            None
        };
        let receipt_store = RunReceiptStore::new(otel_config.state_dir.clone());
        let (run_receipt_store, run_receipt, pending_recovery) = if telemetry_enabled && otel.exports_enabled() {
            let recovery = receipt_store.recover();
            let recovered_records = recovery.records().collect::<Vec<_>>();
            let pending_recovery = if recovered_records.is_empty() {
                recovery.acknowledge();
                None
            } else {
                Some(PendingRecovery {
                    acknowledgement: recovery.defer_acknowledgement(),
                    records: recovered_records,
                })
            };
            (
                Some(receipt_store.clone()),
                process_identity.and_then(|identity| receipt_store.start(identity).ok()),
                pending_recovery,
            )
        } else {
            receipt_store.clear_unlocked();
            (None, None, None)
        };
        let process_identity = Arc::new(Mutex::new(process_identity));
        let worker_process_identity = Arc::clone(&process_identity);
        let mut process_sampler = otel.exports_enabled().then(ProcessSampler::new).flatten();
        let mut process_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(60),
            Duration::from_secs(60),
        );

        let (tx, mut rx) = mpsc::unbounded_channel();
        let tx = TelemetrySender::strong_with_identity(tx, Arc::clone(&identity_epochs));

        // Legacy delivery performs network I/O. Keep it on a separate FIFO so
        // an identity acknowledgement depends only on local OTel emission for
        // events ahead of the control message, never on a telemetry endpoint.
        let (legacy_tx, legacy_handle) = if let Some(sink) = legacy_sink {
            let (legacy_tx, mut legacy_rx) = mpsc::unbounded_channel::<Event>();
            let legacy_handle = tokio::spawn(async move {
                while let Some(event) = legacy_rx.recv().await {
                    if let Some(partition) = govcloud_partition {
                        sink.send_event_govcloud(event, partition).await;
                    } else {
                        sink.send_event(event).await;
                    }
                }
            });
            (Some(legacy_tx), Some(legacy_handle))
        } else {
            (None, None)
        };

        let pending_recovery = pending_recovery.map(|recovery| Arc::new(std::sync::Mutex::new(Some(recovery))));
        let recovery_flush = pending_recovery.as_ref().and_then(|recovery| {
            flush_worker.as_ref().and_then(|worker| {
                worker.flush(
                    export_recovery(otel_config.clone(), Some(recovery)),
                    acknowledge_recovery(Some(recovery)),
                )
            })
        });
        let has_recovery = recovery_flush.is_some();
        let recovery_monitor = async move {
            if let Some(recovery_flush) = recovery_flush {
                match recovery_flush.await {
                    Ok(true) => {},
                    Ok(false) => {
                        error!("recovered telemetry flush failed");
                    },
                    Err(err) => {
                        error!(%err, "recovered telemetry flush worker stopped before completion");
                    },
                }
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
                    message = rx.recv() => {
                        let Some(message) = message else {
                            break;
                        };
                        match message {
                            TelemetryMessage::Event { mut event, epoch } => {
                                apply_event_defaults(&mut event, engine, client_application);
                                trace!(epoch, "TelemetryThread received new telemetry event: {:?}", event);
                                let user_id = worker_identity_epochs.resolve(epoch);
                                otel.emit_metric_records(&event, user_id.as_deref());
                                if let Some(legacy_tx) = legacy_tx.as_ref()
                                    && legacy_tx.send(event).is_err()
                                {
                                    trace!("legacy telemetry worker exited before accepting event");
                                }
                            },
                        }
                    },
                    _ = process_interval.tick(), if process_sampler.is_some() => {
                        let identity = *worker_process_identity.lock().expect("process identity mutex poisoned");
                        if let (Some(identity), Some(sampler)) = (identity, process_sampler.as_mut()) {
                            let user_id = worker_identity_epochs
                                .resolve(worker_identity_epochs.current());
                            otel.emit_records(sampler.sample(identity), user_id.as_deref());
                        }
                    },
                }
            }

            if !recovery_complete {
                recovery_monitor.await;
            }
            let identity = *worker_process_identity.lock().expect("process identity mutex poisoned");
            if let (Some(identity), Some(sampler)) = (identity, process_sampler.as_mut()) {
                let user_id = worker_identity_epochs.resolve(worker_identity_epochs.current());
                otel.emit_records(sampler.final_sample(identity), user_id.as_deref());
            }
            let recovery_outcome = match flush_worker.as_ref() {
                Some(flush_worker) => {
                    request_flush(
                        flush_worker,
                        export_recovery(otel_config.clone(), pending_recovery.as_ref()),
                        acknowledge_recovery(pending_recovery.as_ref()),
                    )
                    .await
                },
                None => FlushOutcome::NotAccepted,
            };
            let recovery_outcome = if matches!(recovery_outcome, FlushOutcome::NotAccepted | FlushOutcome::Abandoned) {
                match FlushWorker::spawn() {
                    Ok(flush_worker) => {
                        request_flush(
                            &flush_worker,
                            export_recovery(otel_config.clone(), pending_recovery.as_ref()),
                            acknowledge_recovery(pending_recovery.as_ref()),
                        )
                        .await
                    },
                    Err(err) => {
                        error!(%err, "failed to start fallback recovery export worker");
                        FlushOutcome::NotAccepted
                    },
                }
            } else {
                recovery_outcome
            };
            if recovery_outcome != FlushOutcome::Succeeded {
                error!(
                    ?recovery_outcome,
                    "recovered telemetry export did not complete successfully"
                );
            }

            let flush_outcome = match flush_worker.as_ref() {
                Some(flush_worker) => {
                    let otel = otel.clone();
                    request_flush(flush_worker, move || otel.flush(), || {}).await
                },
                None => FlushOutcome::NotAccepted,
            };
            let flush_outcome = if matches!(flush_outcome, FlushOutcome::NotAccepted | FlushOutcome::Abandoned) {
                match FlushWorker::spawn() {
                    Ok(flush_worker) => {
                        let otel = otel.clone();
                        request_flush(&flush_worker, move || otel.flush(), || {}).await
                    },
                    Err(err) => {
                        error!(%err, "failed to start fallback telemetry flush worker");
                        FlushOutcome::NotAccepted
                    },
                }
            } else {
                flush_outcome
            };
            if flush_outcome != FlushOutcome::Succeeded {
                error!(?flush_outcome, "final telemetry flush did not complete successfully");
            }
        });

        Ok(Self {
            handle: Some(handle),
            legacy_handle,
            tx,
            identity_epochs,
            process_identity,
            run_receipt_store,
            run_receipt: Some(Arc::new(Mutex::new(run_receipt))),
        })
    }

    pub async fn finish(self) -> Result<(), TelemetryError> {
        self.finish_with_timeout(Duration::from_millis(1000)).await
    }

    pub async fn finish_with_timeout(mut self, timeout: Duration) -> Result<(), TelemetryError> {
        drop(self.tx);
        let mut pending = [self.handle.take(), self.legacy_handle.take()]
            .into_iter()
            .flatten()
            .collect::<futures::stream::FuturesUnordered<_>>();
        let mut first_error = None;
        let deadline = tokio::time::Instant::now() + timeout;
        let post_abort_budget = std::cmp::min(MAX_POST_ABORT_DRAIN_TIMEOUT, timeout / 2);
        let graceful_deadline = deadline - post_abort_budget;
        let timed_out = tokio::time::timeout_at(graceful_deadline, async {
            while let Some(outcome) = futures::StreamExt::next(&mut pending).await {
                if let Err(err) = outcome
                    && first_error.is_none()
                {
                    first_error = Some(err);
                }
            }
        })
        .await
        .is_err();
        if timed_out {
            for handle in pending.iter() {
                handle.abort();
            }
            let _ = tokio::time::timeout_at(deadline, async {
                while let Some(outcome) = futures::StreamExt::next(&mut pending).await {
                    if let Err(err) = outcome
                        && !err.is_cancelled()
                        && first_error.is_none()
                    {
                        first_error = Some(err);
                    }
                }
            })
            .await;
        }
        let result = first_error.map_or(Ok(()), |err| Err(TelemetryError::Join(err)));
        if let Some(run_receipt) = self.run_receipt.take()
            && let Some(receipt) = run_receipt.lock().expect("run receipt mutex poisoned").take()
        {
            receipt.complete();
        }
        result
    }

    pub fn set_process_identity(&self, engine: metric::Engine, role: metric::ProcessRole) {
        let identity = ProcessIdentity::new(engine, role);
        *self.process_identity.lock().expect("process identity mutex poisoned") = Some(identity);
        if let (Some(store), Some(run_receipt)) = (self.run_receipt_store.as_ref(), self.run_receipt.as_ref()) {
            let mut run_receipt = run_receipt.lock().expect("run receipt mutex poisoned");
            if run_receipt.is_none() {
                *run_receipt = store.start(identity).ok();
            }
        }
    }

    /// Send a pre-built telemetry event directly.
    pub fn send_event(&self, event: Event) -> Result<(), TelemetryError> {
        self.tx.send(event)
    }

    /// Shared identity epoch state used by event producers and auth transitions.
    pub fn identity_epochs(&self) -> Arc<IdentityEpochs> {
        Arc::clone(&self.identity_epochs)
    }

    /// Expose the underlying sender for harness-internal use.
    pub fn sender(&self) -> &TelemetrySender {
        &self.tx
    }

    pub fn send_user_logged_in(&self) -> Result<(), TelemetryError> {
        self.tx.send(Event::new(EventType::UserLoggedIn {}))
    }

    pub fn send_auth_failed(
        &self,
        auth_method: &str,
        oauth_flow: &str,
        error_type: &str,
        error_code: Option<String>,
    ) -> Result<(), TelemetryError> {
        self.tx.send(Event::new(EventType::AuthFailed {
            auth_method: auth_method.to_string(),
            oauth_flow: oauth_flow.to_string(),
            error_type: error_type.to_string(),
            error_code,
        }))
    }

    pub fn send_did_select_profile(
        &self,
        source: crate::event::QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: crate::event::TelemetryResult,
        sso_region: Option<String>,
        profile_count: Option<i64>,
    ) -> Result<(), TelemetryError> {
        self.tx.send(Event::new(EventType::DidSelectProfile {
            source,
            amazonq_profile_region,
            result,
            sso_region,
            profile_count,
        }))
    }

    pub fn send_profile_state(
        &self,
        source: crate::event::QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: crate::event::TelemetryResult,
        sso_region: Option<String>,
    ) -> Result<(), TelemetryError> {
        self.tx.send(Event::new(EventType::ProfileState {
            source,
            amazonq_profile_region,
            result,
            sso_region,
        }))
    }

    pub fn send_subagent_invocation(
        &self,
        parent_conversation_id: String,
        subagent_name: String,
        builtin_tool_uses: u32,
        mcp_tool_uses: u32,
        parent_tool_use_id: String,
    ) -> Result<(), TelemetryError> {
        self.tx.send(Event::new(EventType::SubagentInvocation {
            parent_conversation_id,
            subagent_name,
            builtin_tool_uses,
            mcp_tool_uses,
            parent_tool_use_id,
        }))
    }

    pub fn send_subagent_record_user_turn_completion(
        &self,
        conversation_id: String,
        result: crate::event::TelemetryResult,
        args: crate::event::RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        self.tx.send(Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        }))
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
        self.tx.send(Event::new(EventType::GoalCompleted {
            conversation_id,
            terminal_state,
            iterations,
            max_iterations,
            duration_sec,
        }))
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
        self.tx.send(Event::new(EventType::ProcessHealthMetric {
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
        }))
    }

    pub fn send_mode_changed(
        &self,
        from_mode: String,
        to_mode: String,
        source: crate::event::ModeChangeSource,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        self.tx.send(Event::new(EventType::ModeChanged {
            from_mode,
            to_mode,
            source,
            session_id,
        }))
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
            path_scope: event.path_scope,
            approval_path: event.approval_path,
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
        mcp_tools_token_count_estimate: Option<u64>,
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
            mcp_tools_token_count_estimate,
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
#[path = "thread_tests.rs"]
mod tests;
