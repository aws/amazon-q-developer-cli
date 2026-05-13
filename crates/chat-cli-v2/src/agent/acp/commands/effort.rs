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
            let display = capitalize(v);
            CommandOption {
                value: v.clone(),
                label: if is_current {
                    format!("{display}  [active]")
                } else {
                    display
                },
                description: None,
                group: None,
                hint: None,
            }
        })
        .collect();

    CommandOptionsResponse {
        options,
        has_more: false,
    }
}

pub fn execute(args: &EffortArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let Some(level) = &args.level else {
        return match get_options(ctx) {
            r if r.options.is_empty() => {
                let model_name = ctx
                    .rts_state
                    .model_info()
                    .map_or_else(|| "the current model".to_string(), |m| m.display_name().to_string());
                CommandResult::error(format!(
                    "Effort configuration is currently not available on {model_name}. Select a /model that supports effort (like claude-opus-4.7) to configure."
                ))
            },
            r => {
                let levels: Vec<&str> = r.options.iter().map(|o| o.value.as_str()).collect();
                CommandResult::success(format!("Available effort levels: {}", levels.join(", ")))
            },
        };
    };

    match ctx.rts_state.set_additional_field(EFFORT_PATH, level) {
        Ok(()) => CommandResult::success(format!("Effort set to {level}")),
        Err(e) if e.contains("does not support") => {
            let model_name = ctx
                .rts_state
                .model_info()
                .map_or_else(|| "the current model".to_string(), |m| m.display_name().to_string());
            CommandResult::error(format!(
                "Effort configuration is currently not available on {model_name}. Select a /model that supports effort (like claude-opus-4.7) to configure."
            ))
        },
        Err(e) => CommandResult::error(e),
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => {
            let mut result = f.to_uppercase().collect::<String>();
            result.push_str(c.as_str());
            result
        },
    }
}
