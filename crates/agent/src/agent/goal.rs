use serde::{
    Deserialize,
    Serialize,
};

/// Definition of a goal set by the user via /goal command.
/// Persisted to session state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalDefinition {
    pub description: String,
    pub max_iterations: u32,
}
