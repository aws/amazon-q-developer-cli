use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

/// Canonical bounded terminal cause of a failed user turn.
#[typeshare]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnFailureReason {
    ContextLimit,
    Timeout,
    ModelError,
    ToolError,
    ExecutionLimit,
    InternalError,
    #[default]
    Unknown,
}

impl TurnFailureReason {
    pub const ALL: &'static [Self] = &[
        Self::ContextLimit,
        Self::Timeout,
        Self::ModelError,
        Self::ToolError,
        Self::ExecutionLimit,
        Self::InternalError,
        Self::Unknown,
    ];

    pub fn from_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "context_limit" => Self::ContextLimit,
            "timeout" => Self::Timeout,
            "model_error" => Self::ModelError,
            "tool_error" => Self::ToolError,
            "execution_limit" => Self::ExecutionLimit,
            "internal_error" => Self::InternalError,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContextLimit => "context_limit",
            Self::Timeout => "timeout",
            Self::ModelError => "model_error",
            Self::ToolError => "tool_error",
            Self::ExecutionLimit => "execution_limit",
            Self::InternalError => "internal_error",
            Self::Unknown => "unknown",
        }
    }
}
