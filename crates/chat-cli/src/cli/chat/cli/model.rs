use amzn_codewhisperer_client::types::Model;
use clap::Args;
use crossterm::style::{
    self,
    Color,
};
use crossterm::{
    execute,
    queue,
};
use dialoguer::Select;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
};

use crate::api_client::Endpoint;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    /// Display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// Actual model id to send in the API
    pub model_id: String,
    /// Size of the model's context window, in tokens
    #[serde(default = "default_context_window")]
    pub context_window_tokens: usize,
}

impl ModelInfo {
    pub fn from_api_model(model: &Model) -> Self {
        let context_window_tokens = model
            .token_limits()
            .and_then(|limits| limits.max_input_tokens())
            .map_or(default_context_window(), |tokens| tokens as usize);
        Self {
            model_id: model.model_id().to_string(),
            model_name: model.model_name().map(|s| s.to_string()),
            context_window_tokens,
        }
    }

    /// create a default model with only valid model_id（be compatoble with old stored model data）
    pub fn from_id(model_id: String) -> Self {
        Self {
            model_id,
            model_name: None,
            context_window_tokens: 200_000,
        }
    }

    pub fn display_name(&self) -> &str {
        self.model_name.as_deref().unwrap_or(&self.model_id)
    }
}
impl<'de> Deserialize<'de> for ModelInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use std::fmt;

        use serde::de::{
            self,
            MapAccess,
            Visitor,
        };

        struct ModelInfoVisitor;

        impl<'de> Visitor<'de> for ModelInfoVisitor {
            type Value = ModelInfo;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string or a ModelInfo object")
            }

            // old version: modelid string
            fn visit_str<E>(self, value: &str) -> Result<ModelInfo, E>
            where
                E: de::Error,
            {
                Ok(ModelInfo {
                    model_id: value.to_string(),
                    model_name: None,
                    context_window_tokens: default_context_window(),
                })
            }

            // new version: modelInfo object
            fn visit_map<M>(self, mut map: M) -> Result<ModelInfo, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut model_id = None;
                let mut model_name = None;
                let mut context_window_tokens = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "model_id" => model_id = Some(map.next_value()?),
                        "model_name" => model_name = map.next_value()?,
                        "context_window_tokens" => context_window_tokens = Some(map.next_value()?),
                        _ => {
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        },
                    }
                }

                Ok(ModelInfo {
                    model_id: model_id.ok_or_else(|| de::Error::missing_field("model_id"))?,
                    model_name,
                    context_window_tokens: context_window_tokens.unwrap_or_else(default_context_window),
                })
            }
        }

        deserializer.deserialize_any(ModelInfoVisitor)
    }
}
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct ModelArgs;

impl ModelArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        Ok(select_model(os, session).await?.unwrap_or(ChatState::PromptUser {
            skip_printing_tools: false,
        }))
    }
}

pub async fn select_model(os: &Os, session: &mut ChatSession) -> Result<Option<ChatState>, ChatError> {
    queue!(session.stderr, style::Print("\n"))?;

    // Fetch available models from service
    let (models, _default_model) = get_available_models(os).await?;

    if models.is_empty() {
        queue!(
            session.stderr,
            style::SetForegroundColor(Color::Red),
            style::Print("No models available\n"),
            style::ResetColor
        )?;
        return Ok(None);
    }

    let active_model_id = session.conversation.model.as_ref().map(|m| m.model_id.as_str());

    let labels: Vec<String> = models
        .iter()
        .map(|model| {
            let display_name = model.display_name();
            if Some(model.model_id.as_str()) == active_model_id {
                format!("{} (active)", display_name)
            } else {
                display_name.to_owned()
            }
        })
        .collect();

    let selection: Option<_> = match Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt("Select a model for this chat session")
        .items(&labels)
        .default(0)
        .interact_on_opt(&dialoguer::console::Term::stdout())
    {
        Ok(sel) => {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::style::SetForegroundColor(crossterm::style::Color::Magenta)
            );
            sel
        },
        // Ctrl‑C -> Err(Interrupted)
        Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => return Ok(None),
        Err(e) => return Err(ChatError::Custom(format!("Failed to choose model: {e}").into())),
    };

    queue!(session.stderr, style::ResetColor)?;

    if let Some(index) = selection {
        let selected = models[index].clone();
        session.conversation.model = Some(selected.clone());
        let display_name = selected.display_name();

        queue!(
            session.stderr,
            style::Print("\n"),
            style::Print(format!(" Using {}\n\n", display_name)),
            style::ResetColor,
            style::SetForegroundColor(Color::Reset),
            style::SetBackgroundColor(Color::Reset),
        )?;
    }

    execute!(session.stderr, style::ResetColor)?;

    Ok(Some(ChatState::PromptUser {
        skip_printing_tools: false,
    }))
}

pub async fn get_model_info(model_id: &str, os: &Os) -> Result<ModelInfo, ChatError> {
    let (models, _) = get_available_models(os).await?;

    models
        .into_iter()
        .find(|m| m.model_id == model_id)
        .ok_or_else(|| ChatError::Custom(format!("Model '{}' not found", model_id).into()))
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
    vec![
        ModelInfo {
            model_name: Some("claude-3.7-sonnet".to_string()),
            model_id: "claude-3.7-sonnet".to_string(),
            context_window_tokens: 200_000,
        },
        ModelInfo {
            model_name: Some("claude-4-sonnet".to_string()),
            model_id: "claude-4-sonnet".to_string(),
            context_window_tokens: 200_000,
        },
    ]
}
