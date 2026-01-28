//! Legacy model types and functions.
//! Used by ACP agent for model selection.

use amzn_codewhisperer_client::types::Model;
use serde::{
    Deserialize,
    Serialize,
};

use crate::api_client::Endpoint;
use crate::os::Os;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub model_id: String,
    #[serde(default = "default_context_window")]
    pub context_window_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_multiplier: Option<f64>,
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

#[derive(Debug)]
pub struct ModelError(pub String);

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ModelError {}

pub async fn get_available_models(os: &Os) -> Result<(Vec<ModelInfo>, ModelInfo), ModelError> {
    let endpoint = Endpoint::configured_value(&os.database);
    let region = endpoint.region().as_ref();

    match os.client.get_available_models(region).await {
        Ok(api_res) => {
            let models: Vec<ModelInfo> = api_res.models.iter().map(ModelInfo::from_api_model).collect();
            let default_model = ModelInfo::from_api_model(&api_res.default_model);
            Ok((models, default_model))
        },
        Err(e) => {
            tracing::error!("Failed to fetch models from API: {}, using fallback list", e);
            let models = get_fallback_models();
            let default_model = models[0].clone();
            Ok((models, default_model))
        },
    }
}

pub fn context_window_tokens(model_info: Option<&ModelInfo>) -> usize {
    model_info.map_or_else(default_context_window, |m| m.context_window_tokens)
}

fn default_context_window() -> usize {
    200_000
}

fn get_fallback_models() -> Vec<ModelInfo> {
    vec![
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
    ]
}

pub fn normalize_model_name(name: &str) -> &str {
    match name {
        "claude-4-sonnet" => "claude-sonnet-4",
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
