use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

use super::hooks::LegacyHook;

/// Configuration for context files, containing paths to include in the context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LegacyContextConfig {
    /// List of file paths or glob patterns to include in the context.
    pub paths: Vec<String>,

    /// Map of Hook Name to [`Hook`]. The hook name serves as the hook's ID.
    pub hooks: HashMap<String, LegacyHook>,
}
