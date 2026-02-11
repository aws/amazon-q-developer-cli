//! TUI command execution via extension method

use std::path::PathBuf;
use std::sync::Arc;

use ::agent::AgentHandle;
use ::agent::agent_config::LoadedAgentConfig;
use ::agent::tui_commands::{
    CommandOptionsResponse,
    CommandResult,
    TuiCommand,
};

use super::commands::{
    self,
    CommandContext,
};
use super::schema::TuiCommandKind;
use super::session_manager::{
    AgentInfo,
    SessionManagerHandle,
};
use crate::agent::rts::RtsState;
use crate::api_client::ApiClient;

/// Execute a TUI command and return the result
#[allow(clippy::too_many_arguments)]
pub async fn execute_command(
    command: TuiCommand,
    api_client: &ApiClient,
    rts_state: &Arc<RtsState>,
    agent: &AgentHandle,
    session_tx: &SessionManagerHandle,
    available_agents: &[AgentInfo],
    agent_configs: &[LoadedAgentConfig],
    local_mcp_path: Option<&PathBuf>,
    global_mcp_path: Option<&PathBuf>,
    session_id: &str,
    current_agent_name: &str,
) -> CommandResult {
    let ctx = CommandContext {
        api_client,
        rts_state,
        agent,
        session_tx,
        available_agents,
        agent_configs,
        local_mcp_path,
        global_mcp_path,
        session_id,
        current_agent_name,
    };
    commands::execute(command, &ctx).await
}

/// Get options for a command (for autocomplete)
#[allow(clippy::too_many_arguments)]
pub async fn get_command_options(
    command: TuiCommandKind,
    partial: &str,
    api_client: &ApiClient,
    rts_state: &Arc<RtsState>,
    agent: &AgentHandle,
    session_tx: &SessionManagerHandle,
    available_agents: &[AgentInfo],
    agent_configs: &[LoadedAgentConfig],
    local_mcp_path: Option<&PathBuf>,
    global_mcp_path: Option<&PathBuf>,
    session_id: &str,
    current_agent_name: &str,
) -> CommandOptionsResponse {
    let ctx = CommandContext {
        api_client,
        rts_state,
        agent,
        session_tx,
        available_agents,
        agent_configs,
        local_mcp_path,
        global_mcp_path,
        session_id,
        current_agent_name,
    };

    match command {
        TuiCommandKind::Model => commands::model::get_options(partial, &ctx).await,
        TuiCommandKind::Agent => commands::agent::get_options(partial, &ctx),
        TuiCommandKind::Context | TuiCommandKind::Compact | TuiCommandKind::Clear | TuiCommandKind::Quit => {
            CommandOptionsResponse::default()
        },
    }
}
