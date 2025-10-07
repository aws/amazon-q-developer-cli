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
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::database::settings::Setting;
use crate::os::Os;

mod server;
mod server_session;
mod server_connection;
pub(crate) mod util;
#[cfg(test)]
mod client_connection;
#[cfg(test)]
mod client_session;
#[cfg(test)]
mod client_dispatch;

pub use server::AcpServerHandle;
pub use server_connection::AcpServerConnectionHandle;

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

        LocalSet::new().run_until(async move {
            // Set up ACP connection with stdio
            let stdin = tokio::io::stdin().compat();
            let stdout = tokio::io::stdout().compat_write();

            // Create transport actor (will receive connection later)
            AcpServerConnectionHandle::execute(agent_name, os, stdout, stdin).await?;
            Ok(ExitCode::SUCCESS)
        }).await
    }
}
