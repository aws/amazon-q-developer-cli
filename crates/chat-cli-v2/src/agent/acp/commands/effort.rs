//! /effort command — set reasoning effort level
//!
//! The schema path that holds the effort field varies by model family
//! (Claude uses `output_config.effort`; GPT uses `reasoning.effort`). The path
//! is resolved from the model's schema via
//! [`AdditionalModelFields::effort_path`]; this command never hardcodes it.

use agent::tui_commands::{
    CommandOption,
    CommandOptionsResponse,
    CommandResult,
    EffortArgs,
};

use super::CommandContext;

pub fn get_options(ctx: &CommandContext<'_>) -> CommandOptionsResponse {
    let Some(af) = ctx.rts_state.additional_fields() else {
        return CommandOptionsResponse::default();
    };

    // Resolve which schema path this model uses for effort.
    let Some(path) = af.effort_path() else {
        return CommandOptionsResponse::default();
    };

    let fields = af.flatten_schema();
    let Some((_, values)) = fields.iter().find(|(p, _)| p == path) else {
        return CommandOptionsResponse::default();
    };

    let current = af.get_override_str(path);

    let options = values
        .iter()
        .map(|v| {
            let is_current = current.is_some_and(|c| c == v);
            let display = format_effort(v);
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

    match ctx.rts_state.set_effort(level) {
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

fn format_effort(s: &str) -> String {
    if s == "xhigh" {
        return "xHigh".to_string();
    }
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
