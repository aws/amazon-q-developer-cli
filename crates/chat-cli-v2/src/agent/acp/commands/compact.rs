//! /compact command execution

use agent::tui_commands::{
    CommandResult,
    CompactArgs,
};
use tracing::error;

use super::CommandContext;

pub async fn execute(_args: &CompactArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let agent = ctx.agent.clone();
    // Spawn compaction in background to avoid blocking
    tokio::spawn(async move {
        if let Err(e) = agent.compact_conversation().await {
            error!("Compaction failed: {e}");
        }
    });
    CommandResult::success("Compacting conversation...")
}
