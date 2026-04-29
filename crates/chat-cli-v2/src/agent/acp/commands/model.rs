//! /model command execution

use agent::tui_commands::{
    CommandOption,
    CommandOptionsResponse,
    CommandResult,
    ModelArgs,
    ModelInfo,
};
use strsim::jaro_winkler;

use super::CommandContext;
use crate::database::settings::Setting;

/// Minimum similarity score (0.0-1.0) for suggesting a model name
const MODEL_SIMILARITY_THRESHOLD: f64 = 0.6;

pub async fn execute(args: &ModelArgs, ctx: &CommandContext<'_>) -> CommandResult {
    match &args.model_name {
        None => list_models(ctx).await,
        Some(name) if name == "set-current-as-default" => set_current_as_default(ctx).await,
        Some(name) => switch_model(name, ctx).await,
    }
}

async fn set_current_as_default(ctx: &CommandContext<'_>) -> CommandResult {
    let Some(model_id) = ctx.rts_state.model_id() else {
        return CommandResult::error("No model currently selected".to_string());
    };

    let display_name = match fetch_models(ctx).await {
        Ok(models) => models
            .iter()
            .find(|m| m.id == model_id)
            .map_or_else(|| model_id.clone(), |m| m.display_name.clone()),
        Err(_) => model_id.clone(),
    };

    // Persist via the session manager — this updates its in-memory settings (so new sessions
    // within this process pick up the change) and writes to disk (so future process starts
    // pick it up too).
    if let Err(e) = ctx
        .session_tx
        .update_setting(Setting::ChatDefaultModel, serde_json::Value::String(model_id))
        .await
    {
        return CommandResult::error(format!("Failed to set default model: {e}"));
    }

    CommandResult::success(format!("Set {} as default model", display_name))
}

pub async fn get_options(_partial: &str, ctx: &CommandContext<'_>) -> CommandOptionsResponse {
    match fetch_models(ctx).await {
        Ok(models) => {
            let options: Vec<CommandOption> = models.into_iter().map(to_command_option).collect();
            CommandOptionsResponse {
                options,
                has_more: false,
            }
        },
        Err(_) => CommandOptionsResponse::default(),
    }
}

fn to_command_option(m: ModelInfo) -> CommandOption {
    let credits = m
        .rate_multiplier
        .map_or_else(|| "----- credits".to_string(), |r| format!("{r:.2}x credits"));
    CommandOption {
        value: m.id.clone(),
        label: m.display_name,
        description: m.description,
        group: Some(credits),
        hint: None,
    }
}

async fn list_models(ctx: &CommandContext<'_>) -> CommandResult {
    match fetch_models(ctx).await {
        Ok(models) => {
            let current = ctx.rts_state.model_id().unwrap_or_default();
            let message = models
                .iter()
                .map(|m| {
                    let marker = if m.id == current { "→ " } else { "  " };
                    format!("{}{} ({})", marker, m.display_name, m.id)
                })
                .collect::<Vec<_>>()
                .join("\n");
            CommandResult::success_with_data(message, serde_json::json!({ "models": models, "current": current }))
        },
        Err(e) => CommandResult::error(e),
    }
}

async fn switch_model(name: &str, ctx: &CommandContext<'_>) -> CommandResult {
    let models = match fetch_models(ctx).await {
        Ok(m) => m,
        Err(e) => return CommandResult::error(format!("Failed to fetch models: {}", e)),
    };

    // Exact match — switch immediately
    if let Some(m) = models.iter().find(|m| m.id == name) {
        let model = to_legacy_model_info(m);
        let display_name = m.display_name.clone();
        let id = m.id.clone();
        ctx.rts_state.set_model_info(Some(model));
        return CommandResult::success_with_data(
            format!("Model changed to {}", display_name),
            serde_json::json!({ "model": { "id": id, "name": display_name } }),
        );
    }

    // Fuzzy match — suggest, don't switch
    if let Some(m) = find_similar_model(&models, name) {
        return CommandResult::error(format!(
            "Model '{}' not found. Did you mean {}? Run /model to browse available models.",
            name, m.display_name
        ));
    }

    CommandResult::error(format!(
        "Unknown model: {}. Run /model to browse available models.",
        name
    ))
}

fn to_legacy_model_info(m: &ModelInfo) -> crate::cli::chat::legacy::model::ModelInfo {
    crate::cli::chat::legacy::model::ModelInfo {
        model_id: m.id.clone(),
        model_name: Some(m.display_name.clone()),
        description: None,
        context_window_tokens: m.context_window.map_or_else(
            || crate::cli::chat::legacy::model::default_context_window_for_model(&m.id),
            |cw| cw as usize,
        ),
        rate_multiplier: None,
        rate_unit: None,
    }
}

/// Find the closest matching model using fuzzy string matching.
fn find_similar_model<'a>(models: &'a [ModelInfo], query: &str) -> Option<&'a ModelInfo> {
    let query_lower = query.to_lowercase();
    models
        .iter()
        .map(|m| {
            let name_score = jaro_winkler(&query_lower, &m.display_name.to_lowercase());
            let id_score = jaro_winkler(&query_lower, &m.id.to_lowercase());
            (name_score.max(id_score), m)
        })
        .filter(|(score, _)| *score >= MODEL_SIMILARITY_THRESHOLD)
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, m)| m)
}

async fn fetch_models(ctx: &CommandContext<'_>) -> Result<Vec<ModelInfo>, String> {
    let result = ctx
        .api_client
        .list_available_models_cached()
        .await
        .map_err(|e| e.to_string())?;

    Ok(result
        .models
        .into_iter()
        .map(|m| ModelInfo {
            id: m.model_id().to_string(),
            display_name: m.model_name().unwrap_or(m.model_id()).to_string(),
            provider: None,
            context_window: m.token_limits().and_then(|l| l.max_input_tokens()).map(|t| t as u32),
            description: m.description().map(|s| s.to_string()),
            rate_multiplier: m.rate_multiplier(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_similar_model_exact_id() {
        let models = vec![
            ModelInfo {
                id: "claude-sonnet-4".into(),
                display_name: "Claude Sonnet 4".into(),
                provider: None,
                context_window: None,
                description: None,
                rate_multiplier: None,
            },
            ModelInfo {
                id: "claude-opus-4".into(),
                display_name: "Claude Opus 4".into(),
                provider: None,
                context_window: None,
                description: None,
                rate_multiplier: None,
            },
        ];
        let result = find_similar_model(&models, "claude-sonnet-4");
        assert_eq!(result.unwrap().id, "claude-sonnet-4");
    }

    #[test]
    fn test_find_similar_model_fuzzy() {
        let models = vec![ModelInfo {
            id: "claude-sonnet-4".into(),
            display_name: "Claude Sonnet 4".into(),
            provider: None,
            context_window: None,
            description: None,
            rate_multiplier: None,
        }];
        let result = find_similar_model(&models, "claude-sonet-4");
        assert!(result.is_some(), "should fuzzy-match a close typo");
    }

    #[test]
    fn test_find_similar_model_no_match() {
        let models = vec![ModelInfo {
            id: "claude-sonnet-4".into(),
            display_name: "Claude Sonnet 4".into(),
            provider: None,
            context_window: None,
            description: None,
            rate_multiplier: None,
        }];
        let result = find_similar_model(&models, "totally-different");
        assert!(result.is_none());
    }

    #[test]
    fn test_to_command_option_with_rate_multiplier() {
        let model = ModelInfo {
            id: "claude-sonnet-4".into(),
            display_name: "Claude Sonnet 4".into(),
            provider: None,
            context_window: Some(200000),
            description: Some("Fast and capable".into()),
            rate_multiplier: Some(1.0),
        };
        let option = to_command_option(model);
        assert_eq!(option.value, "claude-sonnet-4");
        assert_eq!(option.label, "Claude Sonnet 4");
        assert_eq!(option.description, Some("Fast and capable".into()));
        assert_eq!(option.group, Some("1.00x credits".into()));
    }

    #[test]
    fn test_to_command_option_without_rate_multiplier() {
        let model = ModelInfo {
            id: "custom-model".into(),
            display_name: "Custom Model".into(),
            provider: None,
            context_window: None,
            description: None,
            rate_multiplier: None,
        };
        let option = to_command_option(model);
        assert_eq!(option.group, Some("----- credits".into()));
    }
}
