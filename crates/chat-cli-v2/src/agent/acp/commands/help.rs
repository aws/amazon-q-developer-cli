//! /help command execution

use agent::tui_commands::{
    CommandResult,
    TuiCommand,
};

use super::CommandContext;

pub async fn execute(_ctx: &CommandContext<'_>) -> CommandResult {
    let commands: Vec<TuiCommand> = TuiCommand::all_commands()
        .into_iter()
        .filter(|cmd| {
            // Hide /voice from help when rollout is not enabled (matches advertise_commands)
            if cmd.name() == "/voice" {
                return crate::rollout::Rollout::is_enabled(crate::rollout::Feature::Voice);
            }
            true
        })
        .collect();

    let commands_json: Vec<serde_json::Value> = commands
        .iter()
        .map(|cmd| {
            let mut obj = serde_json::json!({
                "name": cmd.name(),
                "description": cmd.description(),
                "usage": cmd.usage(),
            });
            let subs = cmd.subcommands();
            if !subs.is_empty() {
                obj["subcommands"] = serde_json::json!(subs);
            }
            obj
        })
        .collect();

    let help_text = commands
        .iter()
        .map(|cmd| format!("  {:<25} {}\n    Usage: {}", cmd.name(), cmd.description(), cmd.usage()))
        .collect::<Vec<_>>()
        .join("\n\n");

    let message = format!("Available Commands:\n\n{}", help_text);

    CommandResult::success_with_data(message, serde_json::json!({ "commands": commands_json }))
}
