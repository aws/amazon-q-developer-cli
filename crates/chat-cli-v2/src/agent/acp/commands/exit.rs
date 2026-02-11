//! /quit command execution

use agent::tui_commands::{
    CommandResult,
    QuitArgs,
};

use super::CommandContext;

pub async fn execute(_args: &QuitArgs, _ctx: &CommandContext<'_>) -> CommandResult {
    // Backend acknowledges - TUI handles actual quit
    CommandResult::success("Quitting")
}
