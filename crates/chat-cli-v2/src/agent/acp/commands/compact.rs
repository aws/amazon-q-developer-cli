//! /compact command execution

use agent::tui_commands::{
    CommandResult,
    CompactArgs,
};
use tracing::error;

use super::CommandContext;

pub async fn execute(_args: &CompactArgs, ctx: &CommandContext<'_>) -> CommandResult {
    // Validate conversation has enough content to compact
    let mut snapshot = match ctx.agent.create_snapshot().await {
        Ok(s) => s,
        Err(e) => return CommandResult::error(format!("Failed to check conversation state: {e}")),
    };

    if snapshot.conversation_state.messages().is_empty() {
        return CommandResult::error("Conversation too short to compact.");
    }

    let agent = ctx.agent.clone();
    tokio::spawn(async move {
        if let Err(e) = agent.compact_conversation().await {
            error!("Compaction failed: {e}");
        }
    });
    CommandResult::success("Compacting conversation...")
}
