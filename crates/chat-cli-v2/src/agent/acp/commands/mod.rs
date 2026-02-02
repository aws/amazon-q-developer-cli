//! Slash command execution - each command has its own module with execute fn

pub mod clear;
pub mod compact;
pub mod context;
pub mod exit;
pub mod model;

use std::sync::Arc;

use agent::AgentHandle;
use agent::tui_commands::{
    CommandResult,
    TuiCommand,
};

use crate::agent::rts::RtsState;
use crate::api_client::ApiClient;

/// Context passed to command executors
pub struct CommandContext<'a> {
    pub api_client: &'a ApiClient,
    pub rts_state: &'a Arc<RtsState>,
    pub agent: &'a AgentHandle,
}

/// Execute a slash command by dispatching to the appropriate module
pub async fn execute(command: TuiCommand, ctx: &CommandContext<'_>) -> CommandResult {
    match command {
        TuiCommand::Model(ref args) => model::execute(args, ctx).await,
        TuiCommand::Context(ref args) => context::execute(args, ctx).await,
        TuiCommand::Compact(ref args) => compact::execute(args, ctx).await,
        TuiCommand::Clear(ref args) => clear::execute(args, ctx).await,
        TuiCommand::Exit(ref args) => exit::execute(args, ctx).await,
    }
}
