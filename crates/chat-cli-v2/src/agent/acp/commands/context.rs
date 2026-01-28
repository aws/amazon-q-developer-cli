//! /context command execution

use agent::tui_commands::{
    CommandResult,
    ContextArgs,
};

use super::CommandContext;

pub async fn execute(args: &ContextArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let model = ctx.rts_state.model_id().unwrap_or_else(|| "default".to_string());
    let context_usage = ctx.rts_state.context_usage_percentage();

    if args.verbose {
        CommandResult::success_with_data(
            format!(
                "Current model: {}\nVerbose mode: detailed stats not yet implemented",
                model
            ),
            serde_json::json!({ "model": model, "contextUsagePercentage": context_usage, "verbose": true }),
        )
    } else {
        CommandResult::success_with_data(
            format!("Current model: {}", model),
            serde_json::json!({ "model": model, "contextUsagePercentage": context_usage }),
        )
    }
}
