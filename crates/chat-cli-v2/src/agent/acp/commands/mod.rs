//! Slash command execution - each command has its own module with execute fn

pub mod agent;
pub mod clear;
pub mod compact;
pub mod context;
pub mod exit;
pub mod help;
pub mod issue;
pub mod knowledge;
pub mod mcp;
pub mod model;
pub mod paste_image;
pub mod plan;
pub mod prompts;
pub mod tools;
pub mod usage;

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
    pub os: &'a crate::os::Os,
}

/// Execute a slash command by dispatching to the appropriate module
pub async fn execute(command: TuiCommand, ctx: &CommandContext<'_>) -> CommandResult {
    match command {
        TuiCommand::Help(_args) => help::execute(ctx).await,
        TuiCommand::Model(ref args) => model::execute(args, ctx).await,
        TuiCommand::Agent(ref args) => agent::execute(args, ctx).await,
        TuiCommand::Context(ref args) => context::execute(args, ctx).await,
        TuiCommand::Compact(ref args) => compact::execute(args, ctx).await,
        TuiCommand::Clear(ref args) => clear::execute(args, ctx).await,
        TuiCommand::Quit(ref args) => exit::execute(args, ctx).await,
        TuiCommand::Usage(_args) => usage::execute(ctx).await,
        TuiCommand::PasteImage(_) => paste_image::execute().await,
        TuiCommand::Mcp(_) => mcp::execute(ctx).await,
        TuiCommand::Tools(_) => tools::execute(ctx).await,
        TuiCommand::Plan(ref args) => plan::execute(args.prompt.as_deref(), ctx).await,
        TuiCommand::Issue(_) => issue::execute().await,
        TuiCommand::Knowledge(ref args) => knowledge::execute(args, ctx).await,
        TuiCommand::Prompts(ref args) => prompts::execute(args).await,
    }
}
