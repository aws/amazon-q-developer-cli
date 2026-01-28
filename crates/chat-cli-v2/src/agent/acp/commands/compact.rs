//! /compact command execution

use agent::tui_commands::{
    CommandResult,
    CompactArgs,
};

use super::CommandContext;

pub async fn execute(args: &CompactArgs, _ctx: &CommandContext<'_>) -> CommandResult {
    match args.target_tokens {
        Some(target) => CommandResult::success(format!("Compaction to {} tokens not yet implemented", target)),
        None => CommandResult::success("Compaction not yet implemented"),
    }
}
