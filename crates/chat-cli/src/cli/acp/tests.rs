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
    pub fn set_mock_llm<F>(mut self, script: F) -> Self
    where
        F: Fn(MockLLMContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = eyre::Result<()>> + Send>> + Send + Sync + 'static,
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
        .set_mock_llm(|mut ctx: MockLLMContext| Box::pin(async move {
            // Use declarative pattern matching API - much cleaner!
            ctx.try_patterns(&[
                // First exchange: Greet and ask for name
                (&[], r"Hi, Claude", "Hi, you! What's your name?"),
                
                // Second exchange: Capture name and respond personally  
                (&[r"^assistant:.*What's your name"], r"(?P<name>\w+)", "Hi $name, I'm Q!"),
                
                // Fallback for any unrecognized input
                (&[], r".*", "I didn't understand that."),
            ]).await
        }))
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
