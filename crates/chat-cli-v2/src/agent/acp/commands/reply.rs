//! /reply command execution

use agent::tui_commands::CommandResult;

use super::CommandContext;

pub async fn execute(ctx: &CommandContext<'_>) -> CommandResult {
    match ctx.agent.get_last_assistant_message().await {
        Ok(Some(msg)) => {
            let quoted = msg
                .lines()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            CommandResult::success_with_data(
                "Opening editor with last assistant message",
                serde_json::json!({ "initialContent": quoted }),
            )
        },
        Ok(None) => CommandResult::error("No assistant message found to reply to"),
        Err(e) => CommandResult::error(format!("Failed to get last message: {e}")),
    }
}
