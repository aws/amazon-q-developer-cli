//! /clear command execution

use agent::tui_commands::{
    ClearArgs,
    CommandResult,
};

use super::CommandContext;

pub async fn execute(_args: &ClearArgs, ctx: &CommandContext<'_>) -> CommandResult {
    match ctx.agent.clear_conversation().await {
        Ok(()) => CommandResult::success("Conversation cleared"),
        Err(e) => CommandResult::error(format!("Failed to clear: {e}")),
    }
}
