//! Slash command execution - each command has its own module with execute fn

pub mod agent;
pub mod clear;
pub mod compact;
pub mod context;
pub mod exit;
pub mod model;

use std::path::PathBuf;
use std::sync::Arc;

use ::agent::AgentHandle;
use ::agent::agent_config::LoadedAgentConfig;
use ::agent::tui_commands::{
    CommandResult,
    TuiCommand,
};

use crate::agent::acp::session_manager::{
    AgentInfo,
    SessionManagerHandle,
};
use crate::agent::rts::RtsState;
use crate::api_client::ApiClient;

/// Context passed to command executors
pub struct CommandContext<'a> {
    pub api_client: &'a ApiClient,
    pub rts_state: &'a Arc<RtsState>,
    pub agent: &'a AgentHandle,
    pub session_tx: &'a SessionManagerHandle,
    pub available_agents: &'a [AgentInfo],
    pub agent_configs: &'a [LoadedAgentConfig],
    pub local_mcp_path: Option<&'a PathBuf>,
    pub global_mcp_path: Option<&'a PathBuf>,
    pub session_id: &'a str,
    pub current_agent_name: &'a str,
}

/// Execute a slash command by dispatching to the appropriate module
pub async fn execute(command: TuiCommand, ctx: &CommandContext<'_>) -> CommandResult {
    match command {
        TuiCommand::Model(ref args) => model::execute(args, ctx).await,
        TuiCommand::Agent(ref args) => agent::execute(args, ctx).await,
        TuiCommand::Context(ref args) => context::execute(args, ctx).await,
        TuiCommand::Compact(ref args) => compact::execute(args, ctx).await,
        TuiCommand::Clear(ref args) => clear::execute(args, ctx).await,
        TuiCommand::Quit(ref args) => exit::execute(args, ctx).await,
    }
}
