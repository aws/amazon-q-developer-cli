//! /effort command — set reasoning effort level

use agent::tui_commands::{
    CommandOption,
    CommandOptionsResponse,
    CommandResult,
    EffortArgs,
};

use super::CommandContext;

const EFFORT_PATH: &str = "output_config.effort";

pub fn get_options(ctx: &CommandContext<'_>) -> CommandOptionsResponse {
    let Some(af) = ctx.rts_state.additional_fields() else {
        return CommandOptionsResponse::default();
    };

    let fields = af.flatten_schema();
    let Some((_, values)) = fields.iter().find(|(p, _)| p == EFFORT_PATH) else {
        return CommandOptionsResponse::default();
    };

    let current = af
        .overrides()
        .and_then(|o| o.get("output_config"))
        .and_then(|o| o.get("effort"))
        .and_then(|v| v.as_str());

    let options = values
        .iter()
        .map(|v| {
            let is_current = current.is_some_and(|c| c == v);
            CommandOption {
                value: v.clone(),
                label: if is_current { format!("{v} (current)") } else { v.clone() },
                description: None,
                group: None,
                hint: None,
            }
        })
        .collect();

    CommandOptionsResponse { options, has_more: false }
}

pub fn execute(args: &EffortArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let Some(level) = &args.level else {
        return match get_options(ctx) {
            r if r.options.is_empty() => CommandResult::error("Current model does not support effort levels".to_string()),
            r => {
                let levels: Vec<&str> = r.options.iter().map(|o| o.value.as_str()).collect();
                CommandResult::success(format!("Available effort levels: {}", levels.join(", ")))
            },
        };
    };

    match ctx.rts_state.set_additional_field(EFFORT_PATH, level) {
        Ok(()) => CommandResult::success(format!("Effort set to {level}")),
        Err(e) => CommandResult::error(e),
    }
}
