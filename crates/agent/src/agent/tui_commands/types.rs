//! Slash command types for ACP protocol integration

use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

/// Option displayed in autocomplete/dropdown UI
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOption {
    pub value: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Hint text shown when this option requires additional input (e.g. "<repositoryName>").
    /// When set, selecting this option prefills the command input instead of executing immediately.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Response from options request
#[typeshare]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOptionsResponse {
    pub options: Vec<CommandOption>,
    #[serde(default)]
    pub has_more: bool,
}

/// Result of command execution
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl CommandResult {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
        }
    }

    pub fn success_with_data(message: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

/// Model information for /model command
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_multiplier: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_result_success() {
        let r = CommandResult::success("ok");
        assert!(r.success);
        assert_eq!(r.message, "ok");
        assert!(r.data.is_none());
    }

    #[test]
    fn test_command_result_success_with_data() {
        let r = CommandResult::success_with_data("done", serde_json::json!({"x": 1}));
        assert!(r.success);
        assert_eq!(r.message, "done");
        assert_eq!(r.data.unwrap(), serde_json::json!({"x": 1}));
    }

    #[test]
    fn test_command_result_error() {
        let r = CommandResult::error("failed");
        assert!(!r.success);
        assert_eq!(r.message, "failed");
        assert!(r.data.is_none());
    }

    #[test]
    fn test_command_result_serde() {
        let r = CommandResult::success("hi");
        let json = serde_json::to_string(&r).unwrap();
        let parsed: CommandResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.success, r.success);
        assert_eq!(parsed.message, r.message);
    }

    #[test]
    fn test_command_option_default() {
        let o = CommandOption::default();
        assert_eq!(o.value, "");
        assert_eq!(o.label, "");
        assert!(o.description.is_none());
        assert!(o.group.is_none());
        assert!(o.hint.is_none());
    }

    #[test]
    fn test_command_option_serde() {
        let o = CommandOption {
            value: "v".to_string(),
            label: "L".to_string(),
            description: Some("d".to_string()),
            group: Some("g".to_string()),
            hint: Some("<arg>".to_string()),
        };
        let json = serde_json::to_string(&o).unwrap();
        let parsed: CommandOption = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.value, o.value);
        assert_eq!(parsed.label, o.label);
        assert_eq!(parsed.description, o.description);
        assert_eq!(parsed.hint, o.hint);
    }

    #[test]
    fn test_command_options_response_default() {
        let r = CommandOptionsResponse::default();
        assert!(r.options.is_empty());
        assert!(!r.has_more);
    }

    #[test]
    fn test_command_options_response_serde() {
        let r = CommandOptionsResponse {
            options: vec![CommandOption {
                value: "v".to_string(),
                label: "l".to_string(),
                description: None,
                group: None,
                hint: None,
            }],
            has_more: true,
        };
        let json = serde_json::to_string(&r).unwrap();
        let parsed: CommandOptionsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.options.len(), 1);
        assert!(parsed.has_more);
    }

    #[test]
    fn test_model_info_serde_minimal() {
        let m = ModelInfo {
            id: "m".to_string(),
            display_name: "Model".to_string(),
            provider: None,
            context_window: None,
            description: None,
            rate_multiplier: None,
        };
        let json = serde_json::to_string(&m).unwrap();
        let parsed: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "m");
        assert_eq!(parsed.display_name, "Model");
    }

    #[test]
    fn test_model_info_serde_full() {
        let m = ModelInfo {
            id: "claude-opus".to_string(),
            display_name: "Claude Opus".to_string(),
            provider: Some("anthropic".to_string()),
            context_window: Some(200000),
            description: Some("d".to_string()),
            rate_multiplier: Some(2.5),
        };
        let json = serde_json::to_string(&m).unwrap();
        let parsed: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, m.id);
        assert_eq!(parsed.context_window, Some(200000));
        assert_eq!(parsed.rate_multiplier, Some(2.5));
    }
}
