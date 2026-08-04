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
use crate::cli::chat::legacy::additional_fields::KNOWN_EFFORT_PATHS;
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

    if level == "set-current-as-default" {
        return set_current_as_default(ctx).await;
    }

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

/// Remove the leaf at the dotted-path `segs` from `node`, then prune any
/// ancestor objects left empty by the removal. No-op if the path is absent or
/// crosses a non-object value.
fn prune_effort_path(node: &mut serde_json::Value, segs: &[&str]) {
    let Some((first, rest)) = segs.split_first() else {
        return;
    };
    let Some(obj) = node.as_object_mut() else {
        return;
    };
    if rest.is_empty() {
        obj.remove(*first);
        return;
    }
    let Some(child) = obj.get_mut(*first) else {
        return;
    };
    prune_effort_path(child, rest);
    if child.as_object().is_some_and(serde_json::Map::is_empty) {
        obj.remove(*first);
    }
}

async fn set_current_as_default(ctx: &CommandContext<'_>) -> CommandResult {
    let Some(model_id) = ctx.rts_state.model_id() else {
        return CommandResult::error("No model currently selected".to_string());
    };
    let model_name = ctx
        .rts_state
        .model_info()
        .map_or_else(|| model_id.clone(), |m| m.display_name().to_string());
    let Some((path, level)) = ctx.rts_state.additional_fields().and_then(|af| {
        let path = af.effort_path()?;
        let level = af.get_override_str(path)?.to_string();
        Some((path.to_string(), level))
    }) else {
        return CommandResult::error(format!(
            "No effort level is currently set. Effort may not be available on {model_name}."
        ));
    };

    // Build the per-model delta at the schema-resolved effort path (e.g.
    // `output_config.effort` for Claude, `reasoning.effort` for GPT).
    let mut node = serde_json::json!(level);
    for seg in path.rsplit('.') {
        node = serde_json::json!({ seg: node });
    }
    // Read-modify-write so other models' defaults and non-effort keys on this
    // model survive, while stale effort leaves at other schema paths for this
    // model are deleted (a merge alone cannot remove them).
    let write = ctx
        .os
        .database
        .settings
        .update(Setting::ChatModelDefaults, |existing| {
            let mut root = match existing {
                Some(serde_json::Value::Object(map)) => map,
                _ => serde_json::Map::new(),
            };
            let mut model_node = match root.remove(&model_id) {
                Some(v @ serde_json::Value::Object(_)) => v,
                _ => serde_json::json!({}),
            };
            crate::database::settings::deep_merge(&mut model_node, node);
            for other in KNOWN_EFFORT_PATHS.iter().copied() {
                if other != path {
                    prune_effort_path(&mut model_node, &other.split('.').collect::<Vec<_>>());
                }
            }
            root.insert(model_id.clone(), model_node);
            serde_json::Value::Object(root)
        })
        .await;
    if let Err(e) = write {
        return CommandResult::error(format!("Failed to set default effort: {e}"));
    }

    CommandResult::success(format!(
        "Set {} as default effort for {model_name}",
        format_effort(&level)
    ))
}

fn format_effort(s: &str) -> String {
    s.to_lowercase()
}
