//! /model command execution

use agent::tui_commands::{
    CommandOption,
    CommandOptionsResponse,
    CommandResult,
    ModelArgs,
    ModelInfo,
};

use super::CommandContext;

pub async fn execute(args: &ModelArgs, ctx: &CommandContext<'_>) -> CommandResult {
    match &args.model_name {
        None => list_models(ctx).await,
        Some(name) => switch_model(name, ctx).await,
    }
}

pub async fn get_options(partial: &str, ctx: &CommandContext<'_>) -> CommandOptionsResponse {
    match fetch_models(ctx).await {
        Ok(models) => {
            let partial_lower = partial.to_lowercase();
            let options: Vec<CommandOption> = models
                .into_iter()
                .filter(|m| {
                    partial.is_empty()
                        || m.id.to_lowercase().contains(&partial_lower)
                        || m.display_name.to_lowercase().contains(&partial_lower)
                })
                .map(|m| CommandOption {
                    value: m.id.clone(),
                    label: m.display_name,
                    description: m.context_window.map(|w| format!("Context: {}k tokens", w / 1000)),
                    group: m.provider,
                })
                .collect();
            CommandOptionsResponse {
                options,
                has_more: false,
            }
        },
        Err(_) => CommandOptionsResponse::default(),
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

    let model = match models.iter().find(|m| m.id == name) {
        Some(m) => crate::cli::chat::legacy::model::ModelInfo {
            model_id: m.id.clone(),
            model_name: Some(m.display_name.clone()),
            description: None,
            context_window_tokens: m.context_window.unwrap_or(200_000) as usize,
            rate_multiplier: None,
            rate_unit: None,
        },
        None => return CommandResult::error(format!("Unknown model: {}", name)),
    };

    let display_name = model.model_name.clone().unwrap_or_else(|| name.to_string());
    ctx.rts_state.set_model_info(Some(model));
    CommandResult::success_with_data(
        format!("Model changed to {}", name),
        serde_json::json!({ "model": { "id": name, "name": display_name } }),
    )
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
            context_window: None,
        })
        .collect())
}
