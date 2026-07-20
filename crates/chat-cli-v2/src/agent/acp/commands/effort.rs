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
use crate::database::settings::Setting;

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

pub async fn execute(args: &EffortArgs, ctx: &CommandContext<'_>) -> CommandResult {
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
        Ok(()) => {
            // Persist as per-model default via merge (avoids stale-read clobber)
            // unless the user opted out via `chat.disableAutoDefaultEffort`. When
            // opted out we neither write the setting nor show the "(saved for ...)"
            // suffix. Build the delta at the schema-resolved effort path (e.g.
            // `output_config.effort` for Claude, `reasoning.effort` for GPT).
            let persisted = if ctx
                .os
                .database
                .settings
                .get_bool(Setting::ChatDisableAutoDefaultEffort)
                .unwrap_or(false)
            {
                false
            } else if let (Some(model_id), Some(path)) = (
                ctx.rts_state.model_id(),
                ctx.rts_state.additional_fields().and_then(|af| af.effort_path()),
            ) {
                let mut node = serde_json::json!(level);
                for seg in path.rsplit('.') {
                    node = serde_json::json!({ seg: node });
                }
                ctx.os
                    .database
                    .settings
                    .merge(Setting::ChatModelDefaults, serde_json::json!({ model_id: node }))
                    .await
                    .is_ok()
            } else {
                false
            };
            let model_name = ctx
                .rts_state
                .model_info()
                .map_or_else(|| "current model".to_string(), |m| m.display_name().to_string());
            let suffix = if persisted {
                format!(
                    " (saved for {model_name}; disable with kiro-cli settings {} true)",
                    Setting::ChatDisableAutoDefaultEffort
                )
            } else {
                String::new()
            };
            CommandResult::success(format!("Effort set to {level}{suffix}"))
        },
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
