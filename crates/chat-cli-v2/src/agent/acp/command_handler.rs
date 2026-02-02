//! TUI command execution via extension method

use std::sync::Arc;

use agent::AgentHandle;
use agent::tui_commands::{
    CommandOptionsResponse,
    CommandResult,
    TuiCommand,
};

use super::commands::{
    self,
    CommandContext,
};
use super::schema::TuiCommandKind;
use crate::agent::rts::RtsState;
use crate::api_client::ApiClient;

/// Execute a TUI command and return the result
pub async fn execute_command(
    command: TuiCommand,
    api_client: &ApiClient,
    rts_state: &Arc<RtsState>,
    agent: &AgentHandle,
) -> CommandResult {
    let ctx = CommandContext {
        api_client,
        rts_state,
        agent,
    };
    commands::execute(command, &ctx).await
}

/// Get options for a command (for autocomplete)
pub async fn get_command_options(
    command: TuiCommandKind,
    partial: &str,
    api_client: &ApiClient,
    rts_state: &Arc<RtsState>,
    agent: &AgentHandle,
) -> CommandOptionsResponse {
    let ctx = CommandContext {
        api_client,
        rts_state,
        agent,
    };

    match command {
        TuiCommandKind::Model => commands::model::get_options(partial, &ctx).await,
        TuiCommandKind::Context | TuiCommandKind::Compact | TuiCommandKind::Clear | TuiCommandKind::Exit => {
            CommandOptionsResponse::default()
        },
    }
}
