//! /exit command execution

use agent::tui_commands::{
    CommandResult,
    ExitArgs,
};

use super::CommandContext;

pub async fn execute(_args: &ExitArgs, _ctx: &CommandContext<'_>) -> CommandResult {
    // Backend acknowledges - TUI handles actual exit
    CommandResult::success("Exiting")
}
