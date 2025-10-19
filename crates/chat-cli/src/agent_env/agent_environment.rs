//! AgentEnvironment Coordinator
//!
//! Top-level coordinator that manages event multicasting, UI coordination, and command processing.
//! Supports multiple concurrent UIs (one main interactive UI + multiple headless UIs).

use async_trait::async_trait;
use eyre::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

use super::{
    commands::{AgentEnvironmentCommand, PromptResult},
    event_bus::EventBus,
    events::AgentEnvironmentEvent,
    session::Session,
};

/// Main interactive UI interface
#[async_trait]
pub trait UserInterface: Send + Sync {
    /// Start UI (spawns tasks, returns immediately)
    async fn start(&self) -> Result<()>;

    /// Get receiver for commands from this UI
    fn command_receiver(&self) -> mpsc::Receiver<PromptResult>;

    /// Handle event from EventBus (called by AgentEnvironment)
    async fn handle_event(&self, event: AgentEnvironmentEvent);
}

/// Headless UI interface (non-interactive)
#[async_trait]
pub trait HeadlessInterface: Send + Sync {
    /// Handle event from EventBus
    async fn handle_event(&self, event: AgentEnvironmentEvent);
}

/// AgentEnvironment coordinator
///
/// Manages event multicasting, UI coordination, and command processing.
/// Supports one main interactive UI and multiple headless UIs.
pub struct AgentEnvironment {
    session: Arc<Session>,
    event_bus: EventBus,
    main_ui: Option<Arc<dyn UserInterface>>,
    headless_uis: Vec<Arc<dyn HeadlessInterface>>,
    shutdown_signal: Arc<Notify>,
    interactive: bool,
}

impl AgentEnvironment {
    /// Create new AgentEnvironment
    pub fn new(
        session: Arc<Session>,
        event_bus: EventBus,
        main_ui: Option<Arc<dyn UserInterface>>,
        headless_uis: Vec<Arc<dyn HeadlessInterface>>,
        interactive: bool,
    ) -> Self {
        Self {
            session,
            event_bus,
            main_ui,
            headless_uis,
            shutdown_signal: Arc::new(Notify::new()),
            interactive,
        }
    }

    /// Spawn event multicast task
    fn spawn_event_multicast(&self) -> JoinHandle<()> {
        let mut receiver = self.event_bus.subscribe();
        let headless_uis = self.headless_uis.clone();
        let main_ui = self.main_ui.clone();
        let shutdown = self.shutdown_signal.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = receiver.recv() => {
                        match result {
                            Ok(event) => {
                                // Forward to main UI
                                if let Some(ui) = &main_ui {
                                    ui.handle_event(event.clone()).await;
                                }

                                // Forward to headless UIs
                                for headless_ui in &headless_uis {
                                    headless_ui.handle_event(event.clone()).await;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!("Event bus lagged by {} events", n);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::info!("Event bus closed");
                                break;
                            }
                        }
                    }
                    _ = shutdown.notified() => {
                        tracing::info!("Event multicast shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Start event multicasting (call before creating workers to capture all events)
    pub fn start_event_multicasting(&self) {
        self.spawn_event_multicast();
    }

    /// Spawn job completion monitor for non-interactive mode
    /// 
    /// Monitors JobEvent::Completed events and triggers shutdown when all jobs complete.
    /// Only active in non-interactive mode (interactive=false).
    pub fn spawn_job_completion_monitor(&self) -> JoinHandle<()> {
        // Early return if interactive mode
        if self.interactive {
            return tokio::spawn(async {});
        }

        let mut receiver = self.event_bus.subscribe();
        let session = self.session.clone();
        let shutdown = self.shutdown_signal.clone();

        tokio::spawn(async move {
            use super::events::{AgentEnvironmentEvent, JobEvent, JobCompletionResult, UserInteractionRequired};

            loop {
                match receiver.recv().await {
                    Ok(AgentEnvironmentEvent::Job(JobEvent::Completed { result, .. })) => {
                        // Check if there are any active jobs remaining
                        if !session.has_active_jobs() {
                            // Check completion status
                            match &result {
                                JobCompletionResult::Success { user_interaction_required, .. } => {
                                    if *user_interaction_required == UserInteractionRequired::ToolApproval {
                                        eprintln!("Warning: Job completed with pending tool approval (non-clean exit)");
                                    }
                                }
                                JobCompletionResult::Failed { error } => {
                                    eprintln!("Error: Job failed: {}", error);
                                }
                                JobCompletionResult::Cancelled => {
                                    eprintln!("Warning: Job was cancelled");
                                }
                            }

                            // Trigger shutdown
                            shutdown.notify_waiters();
                            break;
                        }
                    }
                    Ok(_) => {
                        // Ignore other events
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Job completion monitor lagged by {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("Event bus closed in job completion monitor");
                        break;
                    }
                }
            }
        })
    }

    /// Handle command from UI
    async fn handle_command(&self, cmd: AgentEnvironmentCommand) -> Result<()> {
        match cmd {
            AgentEnvironmentCommand::Prompt { worker_id, text } => {
                let worker = self
                    .session
                    .get_worker(worker_id)
                    .ok_or_else(|| eyre::eyre!("Worker not found: {}", worker_id))?;

                // Add message to conversation history
                worker
                    .context_container
                    .conversation_history
                    .lock()
                    .unwrap()
                    .push_input_message(text);

                // Launch agent loop
                self.session
                    .run_task__agent_loop(worker, crate::agent_env::worker_tasks::AgentLoopInput {})?;
            }

            AgentEnvironmentCommand::Compact {
                worker_id,
                instruction,
            } => {
                let worker = self
                    .session
                    .get_worker(worker_id)
                    .ok_or_else(|| eyre::eyre!("Worker not found: {}", worker_id))?;

                // Launch compact task (stub for Phase 10)
                self.session.run_task__compact_conversation(
                    worker,
                    crate::agent_env::worker_tasks::CompactInput { instruction },
                )?;
            }

            AgentEnvironmentCommand::Quit => {
                self.shutdown_signal.notify_waiters();
            }
        }

        Ok(())
    }

    /// Trigger shutdown
    pub fn shutdown(&self) {
        self.shutdown_signal.notify_waiters();
    }
    
    /// Get shutdown signal for external coordination
    pub fn shutdown_signal(&self) -> Arc<Notify> {
        self.shutdown_signal.clone()
    }

    /// Main execution loop
    pub async fn run(&self) -> Result<()> {
        // Start Ctrl+C handler
        use crate::cli::chat::agent_env_ui::CtrlCHandler;
        let ctrl_c_handler = Arc::new(CtrlCHandler::new(
            self.shutdown_signal.clone(),
            self.session.clone(),
        ));
        ctrl_c_handler.start_listening();
        
        // Note: Event multicast task should already be started via start_event_multicasting()
        // before calling run(). If not started, start it now for backward compatibility.
        // This is a no-op if already started since we're using broadcast channels.

        // Run main UI if present
        if let Some(ui) = &self.main_ui {
            // Start UI (spawns its own tasks, returns immediately)
            ui.start().await?;

            // Get command receiver from UI
            let mut cmd_receiver = ui.command_receiver();

            // Main loop: process commands without blocking events
            loop {
                tokio::select! {
                    // Process UI commands
                    Some(result) = cmd_receiver.recv() => {
                        match result {
                            PromptResult::Command(cmd) => {
                                // Check for Quit command before handling
                                let is_quit = matches!(cmd, AgentEnvironmentCommand::Quit);
                                
                                if let Err(e) = self.handle_command(cmd).await {
                                    tracing::error!("Error handling command: {}", e);
                                }
                                
                                // Break immediately after Quit
                                if is_quit {
                                    tracing::info!("Quit command received");
                                    break;
                                }
                            }
                            PromptResult::Shutdown => {
                                tracing::info!("Shutdown requested by UI");
                                break;
                            }
                        }
                    }

                    // Handle shutdown signal
                    _ = self.shutdown_signal.notified() => {
                        tracing::info!("Shutdown signal received");
                        break;
                    }
                }
            }
        } else {
            // Headless mode - just wait for shutdown
            tracing::info!("Running in headless mode");
            self.shutdown_signal.notified().await;
        }

        // Cleanup
        tracing::info!("Shutting down AgentEnvironment");
        // Note: Event multicast task will shut down via shutdown_signal
        self.session.cancel_all_jobs();
        
        tracing::info!("AgentEnvironment cleanup complete, returning");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    // Mock UserInterface for testing
    struct MockUserInterface {
        events_received: Arc<StdMutex<Vec<AgentEnvironmentEvent>>>,
        cmd_sender: mpsc::Sender<PromptResult>,
        cmd_receiver: Arc<StdMutex<Option<mpsc::Receiver<PromptResult>>>>,
    }

    impl MockUserInterface {
        fn new() -> (Self, mpsc::Receiver<PromptResult>) {
            let (cmd_sender, cmd_receiver) = mpsc::channel(10);
            let ui = Self {
                events_received: Arc::new(StdMutex::new(Vec::new())),
                cmd_sender,
                cmd_receiver: Arc::new(StdMutex::new(Some(cmd_receiver))),
            };
            // Return a dummy receiver since we can't return the real one twice
            let (_, dummy_receiver) = mpsc::channel(1);
            (ui, dummy_receiver)
        }

        fn get_events(&self) -> Vec<AgentEnvironmentEvent> {
            self.events_received.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl UserInterface for MockUserInterface {
        async fn start(&self) -> Result<()> {
            Ok(())
        }

        fn command_receiver(&self) -> mpsc::Receiver<PromptResult> {
            self.cmd_receiver
                .lock()
                .unwrap()
                .take()
                .expect("command_receiver called more than once")
        }

        async fn handle_event(&self, event: AgentEnvironmentEvent) {
            self.events_received.lock().unwrap().push(event);
        }
    }

    // Mock HeadlessInterface for testing
    struct MockHeadlessInterface {
        events_received: Arc<StdMutex<Vec<AgentEnvironmentEvent>>>,
    }

    impl MockHeadlessInterface {
        fn new() -> Self {
            Self {
                events_received: Arc::new(StdMutex::new(Vec::new())),
            }
        }

        fn get_events(&self) -> Vec<AgentEnvironmentEvent> {
            self.events_received.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl HeadlessInterface for MockHeadlessInterface {
        async fn handle_event(&self, event: AgentEnvironmentEvent) {
            self.events_received.lock().unwrap().push(event);
        }
    }

    #[tokio::test]
    async fn test_event_multicast_to_main_ui() {
        let event_bus = EventBus::default();
        let model_providers = vec![];
        let session = Arc::new(Session::new(event_bus.clone(), model_providers));

        let (mock_ui, _) = MockUserInterface::new();
        let mock_ui = Arc::new(mock_ui);

        let agent_env = AgentEnvironment::new(
            session.clone(),
            event_bus.clone(),
            Some(mock_ui.clone()),
            vec![],
            true, // interactive mode for test
        );

        // Spawn multicast task
        let _handle = agent_env.spawn_event_multicast();

        // Give multicast task time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Publish test event
        let test_event = AgentEnvironmentEvent::System(
            crate::agent_env::events::SystemEvent::ShutdownInitiated {
                reason: "test".to_string(),
                timestamp: std::time::Instant::now(),
            },
        );
        event_bus.publish(test_event.clone());

        // Give time for event to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Verify mock UI received the event
        let events = mock_ui.get_events();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_system_event());
    }

    #[tokio::test]
    async fn test_event_multicast_to_headless_uis() {
        let event_bus = EventBus::default();
        let model_providers = vec![];
        let session = Arc::new(Session::new(event_bus.clone(), model_providers));

        let headless1 = Arc::new(MockHeadlessInterface::new());
        let headless2 = Arc::new(MockHeadlessInterface::new());

        let agent_env = AgentEnvironment::new(
            session.clone(),
            event_bus.clone(),
            None,
            vec![headless1.clone(), headless2.clone()],
            true, // interactive mode for test
        );

        // Spawn multicast task
        let _handle = agent_env.spawn_event_multicast();

        // Give multicast task time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Publish test event
        let test_event = AgentEnvironmentEvent::System(
            crate::agent_env::events::SystemEvent::ShutdownInitiated {
                reason: "test".to_string(),
                timestamp: std::time::Instant::now(),
            },
        );
        event_bus.publish(test_event.clone());

        // Give time for event to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Verify both headless UIs received the event
        let events1 = headless1.get_events();
        let events2 = headless2.get_events();
        assert_eq!(events1.len(), 1);
        assert_eq!(events2.len(), 1);
        assert!(events1[0].is_system_event());
        assert!(events2[0].is_system_event());
    }

    #[tokio::test]
    async fn test_shutdown_coordination() {
        let event_bus = EventBus::default();
        let model_providers = vec![];
        let session = Arc::new(Session::new(event_bus.clone(), model_providers));

        let agent_env = AgentEnvironment::new(session.clone(), event_bus.clone(), None, vec![], true);

        // Spawn run() in background
        let agent_env_clone = Arc::new(agent_env);
        let run_handle = {
            let agent_env = agent_env_clone.clone();
            tokio::spawn(async move { agent_env.run().await })
        };

        // Give run() time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Trigger shutdown
        agent_env_clone.shutdown();

        // Wait for run() to complete with timeout
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            run_handle,
        )
        .await;

        assert!(result.is_ok(), "run() should complete after shutdown");
        assert!(result.unwrap().is_ok(), "run() should not error");
    }

    #[tokio::test]
    async fn test_headless_mode() {
        let event_bus = EventBus::default();
        let model_providers = vec![];
        let session = Arc::new(Session::new(event_bus.clone(), model_providers));

        let agent_env = AgentEnvironment::new(session.clone(), event_bus.clone(), None, vec![], true);

        // Spawn run() in background
        let agent_env = Arc::new(agent_env);
        let agent_env_clone = agent_env.clone();
        let run_handle = tokio::spawn(async move { agent_env_clone.run().await });

        // Give run() time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Verify it's running (doesn't exit immediately)
        assert!(!run_handle.is_finished());

        // Trigger shutdown
        agent_env.shutdown();

        // Wait for completion
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            run_handle,
        )
        .await;

        assert!(result.is_ok(), "headless mode should exit cleanly");
    }
}
