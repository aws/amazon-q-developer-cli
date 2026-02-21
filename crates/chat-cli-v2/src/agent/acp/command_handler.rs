//! TUI command execution via extension method

use ::agent::tui_commands::{
    CommandOptionsResponse,
    CommandResult,
    TuiCommand,
};

use super::commands;
use super::schema::TuiCommandKind;

/// Execute a TUI command using the session's command context
/// This is a legacy interface - prefer using AcpSession::command_context() directly
pub async fn execute_command_legacy(command: TuiCommand, ctx: &super::commands::CommandContext<'_>) -> CommandResult {
    commands::execute(command, ctx).await
}

/// Get options for a command using the session's command context
/// This is a legacy interface - prefer using AcpSession::command_context() directly
pub async fn get_command_options_legacy(
    command: TuiCommandKind,
    partial: &str,
    ctx: &super::commands::CommandContext<'_>,
) -> CommandOptionsResponse {
    match command {
        TuiCommandKind::Model => commands::model::get_options(partial, ctx).await,
        TuiCommandKind::Agent => commands::agent::get_options(partial, ctx),
        TuiCommandKind::Context
        | TuiCommandKind::Compact
        | TuiCommandKind::Clear
        | TuiCommandKind::Quit
        | TuiCommandKind::Usage
        | TuiCommandKind::Mcp
        | TuiCommandKind::Tools => CommandOptionsResponse::default(),
    }
}
