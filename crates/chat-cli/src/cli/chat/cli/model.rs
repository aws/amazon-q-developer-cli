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
use dialoguer::Select;
use serde::{
    Deserialize,
    Serialize,
};

use crate::api_client::Endpoint;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::theme::StyledText;

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
    #[command(subcommand)]
    pub subcommand: Option<ModelSubcommand>,
}

impl ModelArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self.subcommand {
            Some(subcommand) => subcommand.execute(os, session).await,
            None => Ok(select_model(os, session).await?.unwrap_or(ChatState::PromptUser {
                skip_printing_tools: false,
            })),
        }
    }
}

#[derive(Debug, PartialEq, Subcommand)]
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

    let labels: Vec<String> = models
        .iter()
        .map(|model| {
            let display_name = model.display_name();
            let description = model.description();
            let is_current = Some(model.model_id.as_str()) == active_model_id;

            // Format rate multiplier if available
            let rate_info = model.rate_multiplier.map(|multiplier| {
                let unit = model.rate_unit.as_deref().unwrap_or("credit");
                // Format as whole number if it's a whole number, otherwise show one decimal place
                let multiplier_str = if multiplier.fract().abs() < f64::EPSILON {
                    format!("{multiplier:.0}x")
                } else {
                    format!("{multiplier:.1}x")
                };
                format!(
                    " {} {}",
                    StyledText::secondary("|"),
                    StyledText::secondary(&format!("{multiplier_str} {unit}"))
                )
            });

            let current_marker = if is_current {
                Some(format!(" {}", StyledText::current_item("(current)")))
            } else {
                None
            };

            if let Some(desc) = description {
                format!(
                    "{display_name}{}{} {} {}",
                    current_marker.as_deref().unwrap_or(""),
                    rate_info.as_deref().unwrap_or(""),
                    StyledText::secondary("|"),
                    StyledText::secondary(desc)
                )
            } else {
                format!(
                    "{display_name}{}{}",
                    current_marker.as_deref().unwrap_or(""),
                    rate_info.as_deref().unwrap_or("")
                )
            }
        })
        .collect();

    let default_index = active_model_id
        .and_then(|id| models.iter().position(|m| m.model_id == id))
        .unwrap_or(0);

    let selection: Option<_> = match Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt(format!(
            "{}({}) {} · {}({}) {}",
            StyledText::secondary("Press "),
            StyledText::current_item("↑↓"),
            StyledText::secondary("to navigate"),
            StyledText::secondary("Enter"),
            StyledText::current_item("⏎"),
            StyledText::secondary("to select model")
        ))
        .items(&labels)
        .default(default_index)
        .interact_on_opt(&dialoguer::console::Term::stdout())
    {
        Ok(sel) => {
            let _ = crossterm::execute!(std::io::stdout(), StyledText::emphasis_fg());
            sel
        },
        // Ctrl‑C -> Err(Interrupted)
        Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => return Ok(None),
        Err(e) => return Err(ChatError::Custom(format!("Failed to choose model: {e}").into())),
    };

    queue!(session.stderr, StyledText::reset())?;

    if let Some(index) = selection {
        let selected = models[index].clone();
        session.conversation.model_info = Some(selected.clone());
        let display_name = selected.display_name();

        queue!(
            session.stderr,
            style::Print("\n"),
            style::Print(format!(" Using {display_name}\n\n")),
            StyledText::reset(),
            StyledText::reset(),
            StyledText::reset(),
        )?;
    }

    execute!(session.stderr, StyledText::reset())?;

    Ok(Some(ChatState::PromptUser {
        skip_printing_tools: false,
    }))
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
