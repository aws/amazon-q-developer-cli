//! ACP Actor System Tests using the new clean client-side actors

use std::path::PathBuf;

use crate::{
    cli::acp::{
        client_connection::AcpClientConnectionHandle, server_connection::AcpServerConnectionHandle,
    },
    mock_llm::MockLLMContext,
    os::Os,
};
use agent_client_protocol as acp;
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Clean test harness that mirrors the main ACP setup pattern
pub struct AcpTestHarness {
    os: Os,
}

impl AcpTestHarness {
    /// Create a new test harness with a mock OS
    pub async fn new() -> eyre::Result<Self> {
        Ok(Self { os: Os::new().await? })
    }

    /// Set up a mock LLM script for deterministic testing
    pub fn set_mock_llm<F, Fut>(mut self, script: F) -> Self
    where
        F: Fn(MockLLMContext) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = eyre::Result<()>> + Send + Sync + 'static,
    {
        self.os.client.set_mock_llm(script);
        self
    }

    /// Spawn the test system and return a client handle
    /// This mirrors the clean pattern from the main ACP setup
    pub async fn run(self, test: impl AsyncFnOnce(AcpClientConnectionHandle) -> eyre::Result<()>) -> eyre::Result<()> {
        LocalSet::new().run_until(async move {
            // Create duplex streams for communication (like the main setup uses stdio)
            let (client_write, agent_read) = tokio::io::duplex(1024);
            let (agent_write, client_read) = tokio::io::duplex(1024);

            // Spawn the server side
            tokio::task::spawn_local(async move {
                if let Err(e) = AcpServerConnectionHandle::execute(
                    "test-agent".to_string(),
                    &self.os,
                    agent_write.compat_write(),
                    agent_read.compat(),
                )
                .await
                {
                    tracing::error!("ACP server failed: {}", e);
                }
            });

            // Spawn the client side and return the handle
            let handle =
                AcpClientConnectionHandle::spawn_local(client_write.compat_write(), client_read.compat()).await?;

            // For now initialize with no capabilities
            handle
                .initialize(acp::InitializeRequest {
                    protocol_version: acp::ProtocolVersion::default(),
                    client_capabilities: acp::ClientCapabilities::default(),
                    meta: None,
                })
                .await?;

            test(handle).await
        })
        .await
    }
}

#[tokio::test]
async fn test_acp_actor_system_conversation() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::stderr)
        .try_init()
        .ok();

    AcpTestHarness::new()
        .await?
        .set_mock_llm(|mut ctx: MockLLMContext| async move {
            // Use declarative pattern matching API - much cleaner!
            ctx.try_patterns(&[
                // First exchange: Greet and ask for name
                (&[], r"Hi, Claude", "Hi, you! What's your name?"),
                
                // Second exchange: Capture name and respond personally  
                (&[r"^assistant:.*What's your name"], r"--- USER MESSAGE BEGIN ---\s*(?P<name>\w+)", "Hi $name, I'm Q!"),
                
                // Fallback for any unrecognized input
                (&[], r".*", "I didn't understand that."),
            ]).await
        })
        .run(async |client| {
            let mut session = client
                .new_session(acp::NewSessionRequest {
                    cwd: PathBuf::new(),
                    mcp_servers: vec![],
                    meta: None,
                })
                .await?;

            // First turn: User says "Hi, Claude"
            let response = session.prompt("Hi, Claude").await?;
            assert_eq!(response, "Hi, you! What's your name?");

            // Second turn: User says "Ferris"
            let response = session.prompt("Ferris").await?;
            assert_eq!(response, "Hi Ferris, I'm Q!");

            Ok(())
        })
        .await
}

#[tokio::test]
async fn test_acp_cancel_during_prompt() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::stderr)
        .try_init()
        .ok();

    // Create a coordination channel for mock LLM and test coordination
    let (coordination_tx, mut coordination_rx) = tokio::sync::mpsc::channel::<String>(1);

    AcpTestHarness::new()
        .await?
        .set_mock_llm(move |mut ctx: MockLLMContext| {
            let coordination_tx = coordination_tx.clone();
            async move {
                // Send coordination signal and then block until canceled
                coordination_tx.send("ready".to_string()).await.ok();
                ctx.block_until_canceled("Starting response...").await
            }
        })
        .run(async |client| {
            let mut session = client
                .new_session(acp::NewSessionRequest {
                    cwd: PathBuf::new(),
                    mcp_servers: vec![],
                    meta: None,
                })
                .await?;

            let session_id = session.session_id().clone();

            // Start prompt and cancellation concurrently
            let (prompt_result, _) = tokio::join!(
                session.prompt("test message"),
                async {
                    // Wait for mock LLM to signal it's ready
                    tracing::debug!("waiting for coordination rx");
                    if let Some(signal) = coordination_rx.recv().await {
                        tracing::debug!("received: {signal:?}");
                        assert_eq!(signal, "ready");

                        // Now send the cancel
                        let result = client.cancel(acp::CancelNotification {
                            session_id,
                            meta: None,
                        }).await;

                        tracing::debug!("cancellation method invoked: {result:?}");

                        // Cancel should succeed
                        assert!(result.is_ok());
                    } else {
                        panic!("never got coordination signal");
                    }
                }
            );

            // The prompt should fail with cancellation
            match prompt_result {
                Err(e) => {
                    // Should be canceled error
                    assert!(e.to_string().contains("canceled"), "Expected cancellation error, got: {}", e);
                },
                Ok(response) => {
                    panic!("Expected prompt to be canceled, but got response: {}", response);
                }
            }

            Ok(())
        })
        .await
}

#[tokio::test]
async fn test_acp_cancel_outside_prompt() -> eyre::Result<()> {
    AcpTestHarness::new()
        .await?
        .run(async |client| {
            let session = client
                .new_session(acp::NewSessionRequest {
                    cwd: PathBuf::new(),
                    mcp_servers: vec![],
                    meta: None,
                })
                .await?;

            let session_id = session.session_id().clone();

            // Cancel when no prompt is active - should succeed as a no-op
            let result = client.cancel(acp::CancelNotification {
                session_id,
                meta: None,
            }).await;

            // Should succeed even when there's nothing to cancel
            assert!(result.is_ok());

            Ok(())
        })
        .await
}

#[tokio::test]
async fn test_acp_cancel_cross_session_isolation() -> eyre::Result<()> {
    // Create coordination channels for both mock LLMs
    let (coordination_tx_a, mut coordination_rx_a) = tokio::sync::mpsc::channel::<String>(1);
    let (coordination_tx_b, mut coordination_rx_b) = tokio::sync::mpsc::channel::<String>(1);

    AcpTestHarness::new()
        .await?
        .set_mock_llm(move |mut ctx: MockLLMContext| {
            let coordination_tx_a = coordination_tx_a.clone();
            let coordination_tx_b = coordination_tx_b.clone();
            async move {
                // Determine which session this is based on the message content
                tracing::debug!("Mock LLM starting up: {:?}", ctx.current_user_message());
                if ctx.current_user_message().contains("session_a: will get canceled") {
                    coordination_tx_a.send("session_a_ready".to_string()).await.ok();
                    ctx.block_until_canceled("Session A processing...").await
                } else if ctx.current_user_message().contains("session_b: will get canceled") {
                    coordination_tx_b.send("session_b_ready".to_string()).await.ok();
                    ctx.block_until_canceled("Session B processing...").await
                } else if ctx.current_user_message().contains("session_a: after cancellation") {
                    ctx.respond("session_a recovered ok").await
                } else if ctx.current_user_message().contains("session_b: after cancellation") {
                    ctx.respond("session_b recovered ok").await
                } else {
                    ctx.respond("I don't understand").await
                }
            }
        })
        .run(async |client| {
            // Create two separate sessions
            let mut session_a = client
                .new_session(acp::NewSessionRequest {
                    cwd: PathBuf::new(),
                    mcp_servers: vec![],
                    meta: None,
                })
                .await?;

            let mut session_b = client
                .new_session(acp::NewSessionRequest {
                    cwd: PathBuf::new(),
                    mcp_servers: vec![],
                    meta: None,
                })
                .await?;

            let session_a_id = session_a.session_id().clone();
            let session_b_id = session_b.session_id().clone();

            // Verify sessions have different IDs
            assert_ne!(session_a_id, session_b_id);

            // Start prompts on both sessions concurrently, then test cross-session cancellation
            let (session_a_result, session_b_result, _) = tokio::join!(
                session_a.prompt("session_a: will get canceled"),
                session_b.prompt("session_b: will get canceled"),
                async {
                    // Wait for both sessions to be ready
                    tracing::debug!("Waiting for both sessions to be ready");
                    
                    let signal_a = coordination_rx_a.recv().await;
                    let signal_b = coordination_rx_b.recv().await;
                    
                    assert_eq!(signal_a, Some("session_a_ready".to_string()));
                    assert_eq!(signal_b, Some("session_b_ready".to_string()));

                    tracing::debug!("Both sessions ready, testing cross-session cancellation");

                    // Try to cancel session A using session B's ID - this should be treated as a no-op
                    // since session B isn't in the middle of processing a prompt from our perspective
                    // (it's processing, but the cancel is targeted at the wrong session)
                    let wrong_cancel_result = client.cancel(acp::CancelNotification {
                        session_id: session_b_id.clone(), // Wrong session ID
                        meta: None,
                    }).await;

                    // The cancel should succeed (as a no-op for the wrong session)
                    // but session A should continue processing
                    assert!(wrong_cancel_result.is_ok());

                    // Now cancel the correct session A
                    let correct_cancel_result = client.cancel(acp::CancelNotification {
                        session_id: session_a_id.clone(),
                        meta: None,
                    }).await;

                    assert!(correct_cancel_result.is_ok());
                    tracing::debug!("Sent cancellation for session A");

                    // Also cancel session B for cleanup
                    let cancel_b_result = client.cancel(acp::CancelNotification {
                        session_id: session_b_id.clone(),
                        meta: None,
                    }).await;

                    assert!(cancel_b_result.is_ok());
                    tracing::debug!("Sent cancellation for session B");
                }
            );

            // Both sessions should be canceled
            assert!(session_a_result.is_err());
            assert!(session_b_result.is_err());

            let session_a_error = session_a_result.unwrap_err();
            let session_b_error = session_b_result.unwrap_err();

            assert!(session_a_error.to_string().contains("canceled"), 
                   "Session A should be canceled, got: {}", session_a_error);
            assert!(session_b_error.to_string().contains("canceled"), 
                   "Session B should be canceled, got: {}", session_b_error);

            // Verify both sessions can still accept new prompts after cancellation
            let session_a_recovery = session_a.prompt("session_a: after cancellation").await?;
            let session_b_recovery = session_b.prompt("session_b: after cancellation").await?;

            // These should succeed (mock LLM will respond normally to non-blocking messages)
            // Note: Since we don't have coordination signals for these, they should complete normally
            assert_eq!(session_a_recovery, "session_a recovered ok");
            assert_eq!(session_b_recovery, "session_b recovered ok");

            Ok(())
        })
        .await
}
