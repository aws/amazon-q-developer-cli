use amzn_codewhisperer_client::types::Model;
use clap::{
    Args,
    Subcommand,
};
use crossterm::style::{
    self,
};
use crossterm::{
    execute,
    queue,
};
use serde::{
    Deserialize,
    Serialize,
};
use strsim::jaro_winkler;

use crate::api_client::Endpoint;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::theme::StyledText;

/// Display information for model list
struct ModelListDisplayInfo {
    model_info: ModelInfo,
    is_current: bool,
}

impl ModelListDisplayInfo {
    fn new(model_info: ModelInfo, is_current: bool) -> Self {
        Self { model_info, is_current }
    }

    /// Sort models: current first, then maintain API order
    fn sort_list(list: &mut [Self]) {
        list.sort_by(|a, b| match (a.is_current, b.is_current) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });
    }

    /// Format items for fuzzy selector display
    fn format_for_selector(list: &[Self]) -> Vec<String> {
        let max_name_length = list
            .iter()
            .map(|m| m.model_info.display_name().len())
            .max()
            .unwrap_or(0);

        list.iter()
            .map(|info| {
                let prefix = if info.is_current { "* " } else { "  " };
                let display_name = info.model_info.display_name();

                // Format rate multiplier
                let rate_info = if let Some(multiplier) = info.model_info.rate_multiplier {
                    format!("{multiplier:.2}x credits")
                } else {
                    "----- credits".to_string()
                };

                if let Some(desc) = info.model_info.description() {
                    format!(
                        "{prefix}{:<max_name_length$}    {:<15}    {}",
                        display_name, rate_info, desc
                    )
                } else {
                    format!("{prefix}{:<max_name_length$}    {}", display_name, rate_info)
                }
            })
            .collect()
    }
}

/// Minimum similarity score (0.0-1.0) for suggesting a model name
const MODEL_SIMILARITY_THRESHOLD: f64 = 0.6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// Description of the model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Actual model id to send in the API
    pub model_id: String,
    /// Size of the model's context window, in tokens
    #[serde(default = "default_context_window")]
    pub context_window_tokens: usize,
    /// Rate multiplier of the model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_multiplier: Option<f64>,
    /// Unit for the rate multiplier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_unit: Option<String>,
}

impl ModelInfo {
    pub fn from_api_model(model: &Model) -> Self {
        let context_window_tokens = model
            .token_limits()
            .and_then(|limits| limits.max_input_tokens())
            .map_or(default_context_window(), |tokens| tokens as usize);
        Self {
            model_id: model.model_id().to_string(),
            description: model.description.clone(),
            model_name: model.model_name().map(|s| s.to_string()),
            context_window_tokens,
            rate_multiplier: model.rate_multiplier(),
            rate_unit: model.rate_unit().map(|s| s.to_string()),
        }
    }

    /// create a default model with only valid model_id（be compatoble with old stored model data）
    pub fn from_id(model_id: String) -> Self {
        Self {
            model_id,
            description: None,
            model_name: None,
            context_window_tokens: 200_000,
            rate_multiplier: None,
            rate_unit: None,
        }
    }

    pub fn display_name(&self) -> &str {
        self.model_name.as_deref().unwrap_or(&self.model_id)
    }

    pub fn description(&self) -> Option<&str> {
        self.description
            .as_deref()
            .and_then(|d| if d.is_empty() { None } else { Some(d) })
    }
}

/// Command-line arguments for model selection operations
#[derive(Debug, PartialEq, Args)]
pub struct ModelArgs {
    /// Model name to select directly (e.g., claude-sonnet-4)
    pub model_name: Option<String>,

    #[command(subcommand)]
    pub subcommand: Option<ModelSubcommand>,
}

impl ModelArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match (&self.model_name, &self.subcommand) {
            (_, Some(subcommand)) => subcommand.clone().execute(os, session).await,
            (Some(model_name), None) => select_model_by_name(os, session, model_name).await,
            (None, None) => Ok(select_model(os, session).await?.unwrap_or(ChatState::PromptUser {
                skip_printing_tools: false,
            })),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Subcommand)]
pub enum ModelSubcommand {
    /// Set the current model as the default for new conversations
    #[command(name = "set-current-as-default")]
    SetCurrentAsDefault,
}

impl ModelSubcommand {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self {
            Self::SetCurrentAsDefault => set_current_as_default(os, session).await,
        }
    }
}

pub async fn select_model(os: &Os, session: &mut ChatSession) -> Result<Option<ChatState>, ChatError> {
    // Fetch available models from service
    let (models, _default_model) = get_available_models(os).await?;

    if models.is_empty() {
        queue!(
            session.stderr,
            StyledText::error_fg(),
            style::Print("No models available\n"),
            StyledText::reset(),
        )?;
        return Ok(None);
    }

    let active_model_id = session.conversation.model_info.as_ref().map(|m| m.model_id.as_str());

    // Build display info list
    let mut model_infos: Vec<ModelListDisplayInfo> = models
        .into_iter()
        .map(|model| {
            let is_current = Some(model.model_id.as_str()) == active_model_id;
            ModelListDisplayInfo::new(model, is_current)
        })
        .collect();

    ModelListDisplayInfo::sort_list(&mut model_infos);
    let formatted_items = ModelListDisplayInfo::format_for_selector(&model_infos);

    // Launch fuzzy selector (inline mode)
    let selected = super::super::skim_integration::launch_skim_selector_inline(
        &formatted_items,
        "Select model (type to search): ",
        false,
    )
    .map_err(|e| ChatError::Custom(format!("Failed to launch model selector: {e}").into()))?;

    if let Some(selections) = selected
        && let Some(selected_line) = selections.first()
    {
        // Find the index of the selected line in formatted_items
        let selected_idx = formatted_items
            .iter()
            .position(|item| item == selected_line)
            .ok_or_else(|| ChatError::Custom("Selected item not found".into()))?;

        // Use that index to get the actual model
        let selected = model_infos[selected_idx].model_info.clone();
        session.conversation.model_info = Some(selected.clone());
        let display_name = selected.display_name();

        queue!(
            session.stderr,
            style::Print("\n"),
            style::Print(format!(" Using {display_name}\n\n")),
            StyledText::reset(),
        )?;
    }

    execute!(session.stderr, StyledText::reset())?;

    Ok(Some(ChatState::PromptUser {
        skip_printing_tools: false,
    }))
}

/// Select a model directly by name without interactive selection
async fn select_model_by_name(os: &Os, session: &mut ChatSession, name: &str) -> Result<ChatState, ChatError> {
    let (models, _) = get_available_models(os).await?;

    if let Some(model) = find_model(&models, name) {
        session.conversation.model_info = Some(model.clone());
        let display_name = model.display_name();

        queue!(
            session.stderr,
            style::Print("\n"),
            style::Print(format!(" Using {display_name}\n\n")),
            StyledText::reset(),
        )?;
        execute!(session.stderr, StyledText::reset())?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    } else {
        // Try fuzzy matching to suggest a model
        let suggestion = find_similar_model(&models, name);

        queue!(
            session.stderr,
            StyledText::warning_fg(),
            style::Print(format!("Model '{}' not found.", name)),
            StyledText::reset(),
        )?;

        if let Some(suggested_name) = suggestion {
            queue!(
                session.stderr,
                style::Print(" Did you mean "),
                StyledText::info_fg(),
                style::Print(&suggested_name),
                StyledText::reset(),
                style::Print("?"),
            )?;
        }

        queue!(
            session.stderr,
            style::Print(format!(
                " Run {} to browse available models.\n",
                StyledText::command("/model")
            )),
        )?;

        execute!(session.stderr, StyledText::reset())?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    }
}

async fn set_current_as_default(os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
    if let Some(model_info) = &session.conversation.model_info {
        os.database
            .settings
            .set(Setting::ChatDefaultModel, model_info.model_id.clone(), None)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to set default model: {e}").into()))?;

        queue!(
            session.stderr,
            style::Print("\n"),
            style::Print(format!(" Set {} as default model\n\n", model_info.display_name())),
            StyledText::reset(),
        )?;
    } else {
        queue!(
            session.stderr,
            StyledText::error_fg(),
            style::Print("No model currently selected\n"),
            StyledText::reset(),
        )?;
    }

    Ok(ChatState::PromptUser {
        skip_printing_tools: false,
    })
}

pub async fn get_model_info(model_id: &str, os: &Os) -> Result<ModelInfo, ChatError> {
    let (models, _) = get_available_models(os).await?;

    models
        .into_iter()
        .find(|m| m.model_id == model_id)
        .ok_or_else(|| ChatError::Custom(format!("Model '{model_id}' not found").into()))
}

/// Get available models with caching support
pub async fn get_available_models(os: &Os) -> Result<(Vec<ModelInfo>, ModelInfo), ChatError> {
    let endpoint = Endpoint::configured_value(&os.database);
    let region = endpoint.region().as_ref();

    match os.client.get_available_models(region).await {
        Ok(api_res) => {
            let models: Vec<ModelInfo> = api_res.models.iter().map(ModelInfo::from_api_model).collect();
            let default_model = ModelInfo::from_api_model(&api_res.default_model);

            tracing::debug!("Successfully fetched {} models from API", models.len());
            Ok((models, default_model))
        },
        // In case of API throttling or other errors, fall back to hardcoded models
        Err(e) => {
            tracing::error!("Failed to fetch models from API: {}, using fallback list", e);

            let models = get_fallback_models();
            let default_model = models[0].clone();

            Ok((models, default_model))
        },
    }
}

/// Returns the context window length in tokens for the given model_id.
/// Uses cached model data when available
pub fn context_window_tokens(model_info: Option<&ModelInfo>) -> usize {
    model_info.map_or_else(default_context_window, |m| m.context_window_tokens)
}

fn default_context_window() -> usize {
    200_000
}

fn get_fallback_models() -> Vec<ModelInfo> {
    vec![ModelInfo {
        model_name: Some("auto".to_string()),
        model_id: "auto".to_string(),
        description: None,
        context_window_tokens: 200_000,
        rate_multiplier: None,
        rate_unit: None,
    }]
}

pub fn normalize_model_name(name: &str) -> &str {
    match name {
        "claude-4-sonnet" => "claude-sonnet-4",
        // can add more mapping for backward compatibility
        _ => name,
    }
}

pub fn find_model<'a>(models: &'a [ModelInfo], name: &str) -> Option<&'a ModelInfo> {
    let normalized = normalize_model_name(name);
    models.iter().find(|m| {
        m.model_name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case(normalized))
            || m.model_id.eq_ignore_ascii_case(normalized)
    })
}

/// Find the closest matching model name using fuzzy string matching.
/// Returns the best match if it exceeds the similarity threshold.
fn find_similar_model(models: &[ModelInfo], query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();

    let mut scored: Vec<(f64, &ModelInfo)> = models
        .iter()
        .map(|model| {
            // Score against both model_name and model_id
            let name_score = model
                .model_name
                .as_ref()
                .map_or(0.0, |n| jaro_winkler(&query_lower, &n.to_lowercase()));
            let id_score = jaro_winkler(&query_lower, &model.model_id.to_lowercase());
            (name_score.max(id_score), model)
        })
        .filter(|(score, _)| *score >= MODEL_SIMILARITY_THRESHOLD)
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    scored.first().map(|(_, model)| model.display_name().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_similar_model_exact_match_not_needed() {
        let models = vec![
            ModelInfo {
                model_name: Some("claude-sonnet-4".to_string()),
                model_id: "claude-sonnet-4".to_string(),
                description: None,
                context_window_tokens: 200_000,
                rate_multiplier: None,
                rate_unit: None,
            },
            ModelInfo {
                model_name: Some("claude-3.7-sonnet".to_string()),
                model_id: "claude-3.7-sonnet".to_string(),
                description: None,
                context_window_tokens: 200_000,
                rate_multiplier: None,
                rate_unit: None,
            },
        ];

        // Typo: "claud-sonnet" should suggest "claude-sonnet-4"
        let suggestion = find_similar_model(&models, "claud-sonnet");
        assert!(suggestion.is_some());
        assert_eq!(suggestion.unwrap(), "claude-sonnet-4");

        // Typo: "claude-sonet-4" should suggest "claude-sonnet-4"
        let suggestion = find_similar_model(&models, "claude-sonet-4");
        assert!(suggestion.is_some());
        assert_eq!(suggestion.unwrap(), "claude-sonnet-4");

        // Partial match with more context: "claude-sonnet" should match
        let suggestion = find_similar_model(&models, "claude-sonnet");
        assert!(suggestion.is_some(), "Expected 'claude-sonnet' to fuzzy match a model");
    }

    #[test]
    fn test_find_similar_model_no_match() {
        let models = vec![ModelInfo {
            model_name: Some("claude-sonnet-4".to_string()),
            model_id: "claude-sonnet-4".to_string(),
            description: None,
            context_window_tokens: 200_000,
            rate_multiplier: None,
            rate_unit: None,
        }];

        // Completely unrelated string should not match
        let suggestion = find_similar_model(&models, "xyz123");
        assert!(suggestion.is_none());
    }

    #[test]
    fn test_find_similar_model_case_insensitive() {
        let models = vec![ModelInfo {
            model_name: Some("Claude-Sonnet-4".to_string()),
            model_id: "claude-sonnet-4".to_string(),
            description: None,
            context_window_tokens: 200_000,
            rate_multiplier: None,
            rate_unit: None,
        }];

        // Should match regardless of case
        let suggestion = find_similar_model(&models, "CLAUD-SONNET");
        assert!(suggestion.is_some());
    }
}
