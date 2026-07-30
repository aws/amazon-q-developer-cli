use std::future::Future;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::time::Duration;

use eyre::Result;
use kiro_telemetry::metric::{
    Engine,
    ExitReason,
    ProcessRole,
    RunOutcome,
    SessionInterface,
    StartupFailureStage,
};
use tracing::debug;

use super::{
    AgentEngine,
    emit_cli_invocation_telemetry,
    emit_cli_session_completed,
    emit_cli_session_started,
};
use crate::cli::chat::{
    ChatArgs,
    V1PostExecutionAction,
};
use crate::database::Database;
use crate::os::Os;
use crate::telemetry::TelemetryThread;

const SETUP_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(2);
const SESSION_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(8);
const TRACKED_CLEANUP_GRACE_PERIOD: Duration = Duration::from_secs(7);

pub async fn launch(args: ChatArgs, os: &mut Os, telemetry_name: String) -> Result<ExitCode> {
    let session_interface = if args.no_interactive {
        SessionInterface::NoninteractiveCli
    } else {
        SessionInterface::InteractiveCli
    };
    emit_cli_invocation_telemetry(&os.telemetry, &os.database, Some(telemetry_name), Engine::V1).await;
    os.telemetry.set_process_identity(Engine::V1, ProcessRole::Host);
    let mut lifecycle = V1LaunchLifecycle::new(os, session_interface);
    let session_ready = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    let shutdown = {
        use tokio::signal::unix::{
            SignalKind,
            signal,
        };
        wait_for_shutdown(
            Arc::clone(&session_ready),
            signal(SignalKind::interrupt()).ok(),
            signal(SignalKind::terminate()).ok(),
            signal(SignalKind::hangup()).ok(),
        )
    };
    #[cfg(not(unix))]
    let shutdown = wait_for_early_ctrl_c(Arc::clone(&session_ready));
    let cleanup_tracker = crate::cli::chat::tool_manager::McpCleanupTracker::default();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let execution = args.execute(os, Arc::clone(&session_ready), shutdown_rx, cleanup_tracker.clone());
    let execution = run_until_shutdown(
        execution,
        shutdown,
        shutdown_tx,
        session_ready,
        SETUP_SHUTDOWN_GRACE_PERIOD,
        SESSION_SHUTDOWN_GRACE_PERIOD,
    );
    let execution = async move {
        let result = execution.await;
        cleanup_tracker.wait_bounded(TRACKED_CLEANUP_GRACE_PERIOD).await;
        result
    };
    let action = run_lifecycle(&mut lifecycle, execution).await?;
    if matches!(action, V1PostExecutionAction::ResumeInTuiLite { .. })
        && let Err(error) = os.telemetry.finish().await
    {
        debug!(%error, "failed to flush telemetry before resuming in TUI Lite");
    }
    dispatch(action)
}

async fn run_until_shutdown<F, S>(
    execution: F,
    shutdown: S,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    session_ready: Arc<AtomicBool>,
    setup_grace_period: Duration,
    session_grace_period: Duration,
) -> V1RunOutcome
where
    F: Future<Output = Result<crate::cli::chat::V1ExecutionOutcome>>,
    S: Future<Output = ShutdownCause>,
{
    tokio::pin!(execution);
    tokio::select! {
        result = &mut execution => V1RunOutcome::from_execution(result),
        cause = shutdown => {
            let _ = shutdown_tx.send(());
            let initially_ready = session_ready.load(Ordering::Acquire);
            let first_grace_period = if initially_ready {
                session_grace_period
            } else {
                setup_grace_period
            };
            let result = match tokio::time::timeout(first_grace_period, &mut execution).await {
                Ok(result) => Some(result),
                Err(_) if !initially_ready && session_ready.load(Ordering::Acquire) => {
                    tokio::time::timeout(session_grace_period, &mut execution).await.ok()
                },
                Err(_) => None,
            };
            V1RunOutcome::from_shutdown(cause, result)
        },
    }
}

struct V1RunOutcome {
    result: Result<V1PostExecutionAction>,
    exit_reason: ExitReason,
}

impl V1RunOutcome {
    fn from_execution(result: Result<crate::cli::chat::V1ExecutionOutcome>) -> Self {
        match result {
            Ok(outcome) => Self {
                result: Ok(outcome.action),
                exit_reason: outcome.exit_reason,
            },
            Err(error) => Self {
                result: Err(error),
                exit_reason: ExitReason::Crash,
            },
        }
    }

    fn from_shutdown(cause: ShutdownCause, result: Option<Result<crate::cli::chat::V1ExecutionOutcome>>) -> Self {
        match result {
            Some(result) => Self {
                result: result.map(|outcome| {
                    let exit_code = match outcome.action {
                        V1PostExecutionAction::Exit(exit_code) if exit_code != ExitCode::SUCCESS => exit_code,
                        V1PostExecutionAction::Exit(_) | V1PostExecutionAction::ResumeInTuiLite { .. } => {
                            cause.exit_code()
                        },
                    };
                    V1PostExecutionAction::Exit(exit_code)
                }),
                exit_reason: cause.exit_reason(),
            },
            None => Self {
                result: Ok(V1PostExecutionAction::Exit(ExitCode::FAILURE)),
                exit_reason: ExitReason::HangTimeout,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShutdownCause {
    Interrupt,
    #[cfg(unix)]
    Terminate,
    #[cfg(unix)]
    Hangup,
}

impl ShutdownCause {
    fn exit_code(self) -> ExitCode {
        match self {
            Self::Interrupt => ExitCode::from(130),
            #[cfg(unix)]
            Self::Terminate => ExitCode::from(143),
            #[cfg(unix)]
            Self::Hangup => ExitCode::from(129),
        }
    }

    fn exit_reason(self) -> ExitReason {
        match self {
            Self::Interrupt => ExitReason::UserInterrupt,
            #[cfg(unix)]
            Self::Terminate | Self::Hangup => ExitReason::UserInterrupt,
        }
    }
}

#[cfg(unix)]
async fn wait_for_shutdown(
    session_ready: Arc<AtomicBool>,
    interrupt: Option<tokio::signal::unix::Signal>,
    terminate: Option<tokio::signal::unix::Signal>,
    hangup: Option<tokio::signal::unix::Signal>,
) -> ShutdownCause {
    tokio::select! {
        () = wait_for_early_interrupt(session_ready, interrupt) => ShutdownCause::Interrupt,
        () = wait_for_signal(terminate) => ShutdownCause::Terminate,
        () = wait_for_signal(hangup) => ShutdownCause::Hangup,
    }
}

#[cfg(unix)]
async fn wait_for_signal(signal: Option<tokio::signal::unix::Signal>) {
    match signal {
        Some(mut signal) => {
            signal.recv().await;
        },
        None => std::future::pending().await,
    }
}

#[cfg(unix)]
async fn wait_for_early_interrupt(session_ready: Arc<AtomicBool>, mut interrupt: Option<tokio::signal::unix::Signal>) {
    loop {
        match interrupt.as_mut() {
            Some(interrupt) => {
                interrupt.recv().await;
            },
            None => std::future::pending().await,
        }
        if !session_ready.load(Ordering::Acquire) {
            return;
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_early_ctrl_c(session_ready: Arc<AtomicBool>) -> ShutdownCause {
    loop {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
        if !session_ready.load(Ordering::Acquire) {
            return ShutdownCause::Interrupt;
        }
    }
}

async fn run_lifecycle<L, F>(lifecycle: &mut L, execution: F) -> Result<V1PostExecutionAction>
where
    L: V1Lifecycle,
    F: Future<Output = V1RunOutcome>,
{
    lifecycle.start().await;
    let outcome = execution.await;
    lifecycle.stop_monitor().await;
    lifecycle.complete(outcome.exit_reason).await;
    outcome.result
}

fn dispatch(action: V1PostExecutionAction) -> Result<ExitCode> {
    match action {
        V1PostExecutionAction::Exit(exit_code) => Ok(exit_code),
        V1PostExecutionAction::ResumeInTuiLite { engine, resume_id } => relaunch_in_lite(engine, &resume_id),
    }
}

fn relaunch_in_lite(engine: AgentEngine, resume_id: &str) -> Result<ExitCode> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(["chat", "--agent-engine", engine.user_label(), "--resume-id", resume_id])
        .env("KIRO_UI_MODE", "lite");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        Err(cmd.exec().into())
    }
    #[cfg(not(unix))]
    {
        let status = cmd.status()?;
        Ok(status
            .code()
            .map_or(ExitCode::FAILURE, |exit_code| ExitCode::from(exit_code as u8)))
    }
}

trait V1Lifecycle {
    async fn start(&mut self);
    async fn stop_monitor(&mut self);
    async fn complete(&mut self, exit_reason: ExitReason);
}

struct V1LaunchLifecycle {
    telemetry: TelemetryThread,
    database: Database,
    session_interface: SessionInterface,
}

impl V1LaunchLifecycle {
    fn new(os: &Os, session_interface: SessionInterface) -> Self {
        Self {
            telemetry: os.telemetry.clone(),
            database: os.database.clone(),
            session_interface,
        }
    }
}

impl V1Lifecycle for V1LaunchLifecycle {
    async fn start(&mut self) {
        emit_cli_session_started(&self.telemetry, &self.database, AgentEngine::V1, self.session_interface).await;
    }

    async fn stop_monitor(&mut self) {}

    async fn complete(&mut self, exit_reason: ExitReason) {
        let startup_succeeded = self.telemetry.startup_succeeded();
        if exit_reason != ExitReason::UserInterrupt
            && !startup_succeeded
            && let Err(error) =
                self.telemetry
                    .send_startup_failure(self.session_interface, Engine::V1, StartupFailureStage::Unknown)
        {
            debug!(%error, "failed to emit V1 startup-failure telemetry");
        }
        emit_cli_session_completed(
            &self.telemetry,
            &self.database,
            AgentEngine::V1,
            self.session_interface,
            exit_reason,
            v1_run_outcome(exit_reason, startup_succeeded),
        )
        .await;
    }
}

fn v1_run_outcome(exit_reason: ExitReason, startup_succeeded: bool) -> RunOutcome {
    match exit_reason {
        ExitReason::UserInterrupt => RunOutcome::UserInterrupt,
        _ if !startup_succeeded => RunOutcome::Failure,
        ExitReason::Clean => RunOutcome::Success,
        ExitReason::Crash
        | ExitReason::Oom
        | ExitReason::HangTimeout
        | ExitReason::AuthFailure
        | ExitReason::UpstreamOutage => RunOutcome::Failure,
        ExitReason::Other => RunOutcome::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        Mutex,
    };

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum LifecycleEvent {
        Start,
        Execute,
        StopMonitor,
        Complete(ExitReason),
    }

    struct RecordingLifecycle {
        events: Arc<Mutex<Vec<LifecycleEvent>>>,
    }

    fn execution(exit_reason: ExitReason) -> Result<crate::cli::chat::V1ExecutionOutcome> {
        Ok(crate::cli::chat::V1ExecutionOutcome {
            action: V1PostExecutionAction::Exit(ExitCode::SUCCESS),
            exit_reason,
        })
    }

    fn assert_exit(action: V1PostExecutionAction, expected: ExitCode) {
        assert_eq!(action, V1PostExecutionAction::Exit(expected));
    }

    impl V1Lifecycle for RecordingLifecycle {
        async fn start(&mut self) {
            self.events.lock().unwrap().push(LifecycleEvent::Start);
        }

        async fn stop_monitor(&mut self) {
            self.events.lock().unwrap().push(LifecycleEvent::StopMonitor);
        }

        async fn complete(&mut self, exit_reason: ExitReason) {
            self.events.lock().unwrap().push(LifecycleEvent::Complete(exit_reason));
        }
    }

    #[tokio::test]
    async fn completes_lifecycle_before_returning_relaunch() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut lifecycle = RecordingLifecycle { events: events.clone() };
        let execution_events = events.clone();

        let action = run_lifecycle(&mut lifecycle, async move {
            execution_events.lock().unwrap().push(LifecycleEvent::Execute);
            V1RunOutcome {
                result: Ok(V1PostExecutionAction::ResumeInTuiLite {
                    engine: AgentEngine::V2,
                    resume_id: "session-id".to_string(),
                }),
                exit_reason: ExitReason::Clean,
            }
        })
        .await
        .unwrap();

        assert_eq!(action, V1PostExecutionAction::ResumeInTuiLite {
            engine: AgentEngine::V2,
            resume_id: "session-id".to_string(),
        });
        assert_eq!(*events.lock().unwrap(), [
            LifecycleEvent::Start,
            LifecycleEvent::Execute,
            LifecycleEvent::StopMonitor,
            LifecycleEvent::Complete(ExitReason::Clean),
        ]);
    }

    #[tokio::test]
    async fn early_shutdown_notifies_execution_and_waits_for_cleanup() {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let execution = async move {
            shutdown_rx.await.unwrap();
            execution(ExitReason::Clean)
        };
        let outcome = run_until_shutdown(
            execution,
            async { ShutdownCause::Interrupt },
            shutdown_tx,
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .await;

        assert_exit(outcome.result.unwrap(), ExitCode::from(130));
        assert_eq!(outcome.exit_reason, ExitReason::UserInterrupt);
    }

    #[tokio::test]
    async fn shutdown_notifies_a_started_execution_to_clean_up() {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let execution = async move {
            shutdown_rx.await.unwrap();
            execution(ExitReason::Clean)
        };

        let outcome = run_until_shutdown(
            execution,
            async { ShutdownCause::Interrupt },
            shutdown_tx,
            Arc::new(AtomicBool::new(true)),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .await;

        assert_exit(outcome.result.unwrap(), ExitCode::from(130));
        assert_eq!(outcome.exit_reason, ExitReason::UserInterrupt);
    }

    #[tokio::test]
    async fn grace_exhaustion_fails_and_completes_lifecycle_as_hang_timeout() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut lifecycle = RecordingLifecycle { events: events.clone() };
        let execution_events = events.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let execution = async move {
            execution_events.lock().unwrap().push(LifecycleEvent::Execute);
            let _ = started_tx.send(());
            let _shutdown_rx = shutdown_rx;
            std::future::pending::<Result<crate::cli::chat::V1ExecutionOutcome>>().await
        };
        let execution = run_until_shutdown(
            execution,
            async move {
                let _ = started_rx.await;
                ShutdownCause::Interrupt
            },
            shutdown_tx,
            Arc::new(AtomicBool::new(false)),
            Duration::from_millis(10),
            Duration::from_millis(100),
        );

        let action = tokio::time::timeout(Duration::from_secs(1), run_lifecycle(&mut lifecycle, execution))
            .await
            .expect("early shutdown should be bounded")
            .unwrap();

        assert_exit(action, ExitCode::FAILURE);
        assert_eq!(*events.lock().unwrap(), [
            LifecycleEvent::Start,
            LifecycleEvent::Execute,
            LifecycleEvent::StopMonitor,
            LifecycleEvent::Complete(ExitReason::HangTimeout),
        ]);
    }

    #[tokio::test]
    async fn shutdown_error_is_still_classified_as_an_interrupt() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut lifecycle = RecordingLifecycle { events: events.clone() };
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let execution = async move {
            shutdown_rx.await.unwrap();
            eyre::bail!("cleanup failed")
        };
        let execution = run_until_shutdown(
            execution,
            async { ShutdownCause::Interrupt },
            shutdown_tx,
            Arc::new(AtomicBool::new(true)),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );

        assert!(run_lifecycle(&mut lifecycle, execution).await.is_err());
        assert_eq!(*events.lock().unwrap(), [
            LifecycleEvent::Start,
            LifecycleEvent::StopMonitor,
            LifecycleEvent::Complete(ExitReason::UserInterrupt),
        ]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_remains_bounded_while_input_pipe_is_open() {
        use std::os::unix::net::UnixStream;

        let (reader, writer) = UnixStream::pair().unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel();
        let execution = async move {
            let _ = started_tx.send(());
            crate::cli::chat::read_to_string_in_background(reader).await?;
            execution(ExitReason::Clean)
        };
        let shutdown = async move {
            let _ = started_rx.await;
            ShutdownCause::Interrupt
        };

        let outcome = tokio::time::timeout(
            Duration::from_secs(1),
            run_until_shutdown(
                execution,
                shutdown,
                shutdown_tx,
                Arc::new(AtomicBool::new(false)),
                Duration::from_millis(10),
                Duration::from_millis(100),
            ),
        )
        .await
        .expect("open input pipe should not block shutdown");
        drop(writer);

        assert_exit(outcome.result.unwrap(), ExitCode::FAILURE);
        assert_eq!(outcome.exit_reason, ExitReason::HangTimeout);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn distinguishes_interrupt_from_termination_signal_outcomes() {
        async fn complete_after_shutdown(cause: ShutdownCause) -> V1RunOutcome {
            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
            let execution = async move {
                shutdown_rx.await.unwrap();
                execution(ExitReason::Clean)
            };
            run_until_shutdown(
                execution,
                async move { cause },
                shutdown_tx,
                Arc::new(AtomicBool::new(true)),
                Duration::from_secs(1),
                Duration::from_secs(1),
            )
            .await
        }

        let interrupt = complete_after_shutdown(ShutdownCause::Interrupt).await;
        assert_exit(interrupt.result.unwrap(), ExitCode::from(130));
        assert_eq!(interrupt.exit_reason, ExitReason::UserInterrupt);

        for (cause, exit_code) in [
            (ShutdownCause::Terminate, ExitCode::from(143)),
            (ShutdownCause::Hangup, ExitCode::from(129)),
        ] {
            let outcome = complete_after_shutdown(cause).await;
            assert_exit(outcome.result.unwrap(), exit_code);
            assert_eq!(outcome.exit_reason, ExitReason::UserInterrupt);
        }
    }

    #[test]
    fn maps_execution_outcomes_to_completion_reasons() {
        let clean = V1RunOutcome::from_execution(execution(ExitReason::Clean));
        assert_exit(clean.result.unwrap(), ExitCode::SUCCESS);
        assert_eq!(clean.exit_reason, ExitReason::Clean);

        let interrupted = V1RunOutcome::from_execution(execution(ExitReason::UserInterrupt));
        assert_eq!(interrupted.exit_reason, ExitReason::UserInterrupt);

        let crashed = V1RunOutcome::from_execution(Err(eyre::eyre!("failed")));
        assert!(crashed.result.is_err());
        assert_eq!(crashed.exit_reason, ExitReason::Crash);
    }

    #[test]
    fn classifies_v1_run_outcomes_by_startup_state() {
        assert_eq!(v1_run_outcome(ExitReason::Crash, false), RunOutcome::Failure);
        assert_eq!(v1_run_outcome(ExitReason::Crash, true), RunOutcome::Failure);
        assert_eq!(v1_run_outcome(ExitReason::Clean, true), RunOutcome::Success);
        assert_eq!(
            v1_run_outcome(ExitReason::UserInterrupt, false),
            RunOutcome::UserInterrupt
        );
    }
}
