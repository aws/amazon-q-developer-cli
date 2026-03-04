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

/// Minimum similarity score (0.0-1.0) for suggesting a model name
const MODEL_SIMILARITY_THRESHOLD: f64 = 0.6;

pub async fn execute(args: &ModelArgs, ctx: &CommandContext<'_>) -> CommandResult {
    match &args.model_name {
        None => list_models(ctx).await,
        Some(name) => switch_model(name, ctx).await,
    }
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
        context_window_tokens: m.context_window.unwrap_or(200_000) as usize,
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
