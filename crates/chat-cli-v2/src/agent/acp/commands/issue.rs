//! /feedback command execution

use agent::tui_commands::{
    CommandOption,
    CommandOptionsResponse,
    CommandResult,
};

const INTERNAL_GENERAL_URL: &str =
    "https://taskei.amazon.dev/tasks/create?template=c1d1a001-24af-4fd4-806c-46a9399dfe11";
const EXTERNAL_GENERAL_URL: &str = "https://github.com/kirodotdev/Kiro/issues/new/choose";

const INTERNAL_FEATURE_URL: &str =
    "https://taskei.amazon.dev/tasks/create?template=c1d1a001-24af-4fd4-806c-46a9399dfe11";
const EXTERNAL_FEATURE_URL: &str = "https://github.com/kirodotdev/Kiro/issues/new?template=feature_request.yml";

const INTERNAL_ISSUE_URL: &str = "https://taskei.amazon.dev/tasks/create?template=c0312360-3f55-432d-a6d2-e3060ad2cc59";
const EXTERNAL_ISSUE_URL: &str = "https://github.com/kirodotdev/Kiro/issues";

/// Return the 3 feedback options for the selection menu.
pub fn get_options() -> CommandOptionsResponse {
    CommandOptionsResponse {
        options: vec![
            CommandOption {
                value: "general".to_string(),
                label: "General feedback".to_string(),
                description: Some("Share general thoughts or suggestions".to_string()),
                group: None,
                hint: None,
            },
            CommandOption {
                value: "feature".to_string(),
                label: "Feature request".to_string(),
                description: Some("Request a new feature or improvement".to_string()),
                group: None,
                hint: None,
            },
            CommandOption {
                value: "issue".to_string(),
                label: "Report an issue".to_string(),
                description: Some("Report a bug or problem".to_string()),
                group: None,
                hint: None,
            },
        ],
        has_more: false,
    }
}

/// Execute /feedback command with a selected feedback type.
pub async fn execute(feedback_type: Option<&str>, is_amzn: bool) -> CommandResult {
    let kind = feedback_type.unwrap_or("general");
    let url = match kind {
        "general" if is_amzn => INTERNAL_GENERAL_URL,
        "general" => EXTERNAL_GENERAL_URL,
        "feature" if is_amzn => INTERNAL_FEATURE_URL,
        "feature" => EXTERNAL_FEATURE_URL,
        "issue" if is_amzn => INTERNAL_ISSUE_URL,
        _ => EXTERNAL_ISSUE_URL,
    };
    match crate::util::open::open_url_async(url).await {
        Ok(()) => CommandResult::success("Opening in browser..."),
        Err(_) => CommandResult {
            success: false,
            message: format!("Could not open browser. Copy the URL: {url}"),
            data: Some(serde_json::json!({ "url": url })),
        },
    }
}
