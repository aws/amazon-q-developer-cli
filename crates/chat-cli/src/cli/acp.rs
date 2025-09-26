//! Agent Client Protocol (ACP) implementation using actor pattern
//!
//! This module implements ACP server functionality using Alice Ryhl's actor pattern
//! for clean separation of concerns and message passing instead of shared state.
//!
//! ## Architecture Flow
//!
//! When an ACP client sends a prompt request:
//!
//! ```text
//! ACP Client                 AcpAgentForward           AcpServerActor           AcpSessionActor
//!     │                           │                         │                        │
//!     │ acp.prompt("Hi")          │                         │                        │
//!     ├──────JSON-RPC────────────→│                         │                        │
//!     │                           │ ServerMethod::Prompt    │                        │
//!     │                           ├────────channel─────────→│                        │
//!     │                           │                         │ SessionMethod::Prompt │
//!     │                           │                         ├───────channel────────→│
//!     │                           │                         │                        │ ConversationState
//!     │                           │                         │                        │ processes prompt
//!     │                           │                         │                        │ with LLM
//!     │                           │                         │                        │
//!     │                           │                         │ ←──────response───────│
//!     │                           │ ←──────response─────────│                        │
//!     │ ←────JSON-RPC─────────────│                         │                        │
//! ```
//!
//! ## Key Benefits
//!
//! - **No shared state**: Each actor owns its data (no RwLocks)
//! - **Natural backpressure**: Bounded channels prevent unbounded queuing
//! - **Clean separation**: Protocol handling, session management, and conversation processing are separate
//! - **Easy testing**: Each actor can be tested independently

use std::process::ExitCode;

use clap::Parser;
use eyre::Result;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::database::settings::Setting;
use crate::os::Os;

mod server;
mod session;
mod forward;
mod transport;

pub use server::AcpServerHandle;
pub use session::AcpSessionHandle;
pub use forward::AcpAgentForward;
pub use transport::AcpTransportHandle;

#[cfg(test)]
mod tests;

#[derive(Debug, Parser, PartialEq)]
pub struct AcpArgs {
    /// Agent to use for ACP sessions
    #[arg(long)]
    pub agent: Option<String>,
}

impl AcpArgs {
    pub async fn run(self, os: &mut Os) -> Result<ExitCode> {
        // Check feature flag
        if !os.database.settings.get_bool(Setting::EnabledAcp).unwrap_or(false) {
            eprintln!("ACP is disabled. Enable with: q settings acp.enabled true");
            return Ok(ExitCode::FAILURE);
        }

        let agent_name = self.agent.unwrap_or_else(|| "default".to_string());
        
        tracing::info!("Starting ACP server with agent: {}", agent_name);
        
        // Create ACP server with LocalSet for non-Send futures
        let local_set = tokio::task::LocalSet::new();
        local_set.run_until(async move {
            // Create transport actor (will receive connection later)
            let transport_handle = AcpTransportHandle::new();
            
            // Spawn the server actor with transport handle
            let server_handle = AcpServerHandle::spawn(agent_name, os.clone(), transport_handle.clone());
            
            // Create forwarding agent
            let agent = AcpAgentForward::new(server_handle);
            
            // Set up ACP connection with stdio
            let stdin = tokio::io::stdin().compat();
            let stdout = tokio::io::stdout().compat_write();
            
            let (connection, handle_io) = agent_client_protocol::AgentSideConnection::new(
                agent,
                stdout,
                stdin,
                |fut| {
                    tokio::task::spawn_local(fut);
                }
            );
            
            // Give the connection to the transport actor
            transport_handle.set_connection(connection).await;
            
            tracing::info!("ACP server started, waiting for client connections...");
            
            // Run the connection (this will block until the client disconnects)
            if let Err(e) = handle_io.await {
                tracing::error!("ACP connection error: {}", e);
            }
            
            tracing::info!("ACP server shutting down gracefully");
            Ok::<(), eyre::Error>(())
        }).await?;
        
        Ok(ExitCode::SUCCESS)
    }
}
