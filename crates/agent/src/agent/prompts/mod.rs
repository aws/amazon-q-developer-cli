use std::sync::LazyLock;

use regex::Regex;

mod file_prompts;
mod template_args;

/// Regex for validating prompt names (alphanumeric, hyphens, underscores only).
pub static PROMPT_NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());

pub use file_prompts::discover;
pub use template_args::{
    PromptTemplateArgs,
    resolve_file_prompt,
};
