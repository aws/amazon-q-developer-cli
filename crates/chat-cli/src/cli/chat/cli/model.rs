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

use crate::auth::builder_id::{
    BuilderIdToken,
    TokenType,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

pub struct ModelOption {
    /// Display name
    pub name: &'static str,
    /// Actual model id to send in the API
    pub model_id: &'static str,
    /// Size of the model's context window, in tokens
    pub context_window_tokens: usize,
}

const MODEL_OPTIONS: [ModelOption; 2] = [
    ModelOption {
        name: "claude-4-sonnet",
        model_id: "CLAUDE_SONNET_4_20250514_V1_0",
        context_window_tokens: 200_000,
    },
    ModelOption {
        name: "claude-3.7-sonnet",
        model_id: "CLAUDE_3_7_SONNET_20250219_V1_0",
        context_window_tokens: 200_000,
    },
];

/// Parse custom model format: custom:<region>:<actual-model-id>
/// Example: custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0
/// Or: custom:us-east-1:CLAUDE_SONNET_4_20250514_V1_0
pub fn parse_custom_model(model_id: &str) -> Option<(String, String)> {
    if !model_id.starts_with("custom:") {
        return None;
    }

    // Remove "custom:" prefix
    let without_prefix = &model_id[7..];

    // Find the first colon to separate region from model ID
    if let Some(colon_pos) = without_prefix.find(':') {
        let region = without_prefix[..colon_pos].to_string();
        let mut actual_model_id = without_prefix[colon_pos + 1..].to_string();

        // Map common Bedrock model IDs to Q Developer format
        actual_model_id = map_bedrock_to_q_model(&actual_model_id);

        return Some((region, actual_model_id));
    }

    None
}

/// Map Bedrock model IDs to Q Developer model IDs
fn map_bedrock_to_q_model(model_id: &str) -> String {
    match model_id {
        // Claude 3.5 Sonnet mappings
        "us.anthropic.claude-3-5-sonnet-20241022-v2:0"
        | "anthropic.claude-3-5-sonnet-20241022-v2:0"
        | "claude-3-5-sonnet-20241022" => "CLAUDE_3_7_SONNET_20250219_V1_0".to_string(),

        // Claude 4 Sonnet mappings
        "anthropic.claude-4-sonnet:0" | "claude-4-sonnet" => "CLAUDE_SONNET_4_20250514_V1_0".to_string(),

        // If already in Q Developer format or unknown, pass through
        _ => model_id.to_string(),
    }
}

const GPT_OSS_120B: ModelOption = ModelOption {
    name: "openai-gpt-oss-120b-preview",
    model_id: "OPENAI_GPT_OSS_120B_1_0",
    context_window_tokens: 128_000,
};

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
    let active_model_id = session.conversation.model.as_deref();
    let model_options = get_model_options(os).await?;

    let labels: Vec<String> = model_options
        .iter()
        .map(|opt| {
            if (opt.model_id.is_empty() && active_model_id.is_none()) || Some(opt.model_id) == active_model_id {
                format!("{} (active)", opt.name)
            } else {
                opt.name.to_owned()
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
        let selected = &model_options[index];
        let model_id_str = selected.model_id.to_string();
        session.conversation.model = Some(model_id_str);

        queue!(
            session.stderr,
            style::Print("\n"),
            style::Print(format!(" Using {}\n\n", selected.name)),
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

/// Returns a default model id to use if none has been otherwise provided.
///
/// Returns Claude 3.7 for: Amazon IDC users, FRA region users
/// Returns Claude 4.0 for: Builder ID users, other regions
pub async fn default_model_id(os: &Os) -> &'static str {
    // Check FRA region first
    if let Ok(Some(profile)) = os.database.get_auth_profile() {
        if profile.arn.split(':').nth(3) == Some("eu-central-1") {
            return "CLAUDE_3_7_SONNET_20250219_V1_0";
        }
    }

    // Check if Amazon IDC user
    if let Ok(Some(token)) = BuilderIdToken::load(&os.database).await {
        if matches!(token.token_type(), TokenType::IamIdentityCenter) && token.is_amzn_user() {
            return "CLAUDE_3_7_SONNET_20250219_V1_0";
        }
    }

    // Default to 4.0
    "CLAUDE_SONNET_4_20250514_V1_0"
}

/// Returns the available models for use.
#[allow(unused_variables)]
pub async fn get_model_options(os: &Os) -> Result<Vec<ModelOption>, ChatError> {
    Ok(MODEL_OPTIONS.into_iter().collect::<Vec<_>>())
    // TODO: Once we have access to gpt-oss, add back.
    // let mut model_options = MODEL_OPTIONS.into_iter().collect::<Vec<_>>();
    //
    // // GPT OSS is only accessible in IAD.
    // let endpoint = Endpoint::configured_value(&os.database);
    // if endpoint.region().as_ref() != "us-east-1" {
    //     return Ok(model_options);
    // }
    //
    // model_options.push(GPT_OSS_120B);
    // Ok(model_options)
}

/// Returns the context window length in tokens for the given model_id.
pub fn context_window_tokens(model_id: Option<&str>) -> usize {
    const DEFAULT_CONTEXT_WINDOW_LENGTH: usize = 200_000;

    let Some(model_id) = model_id else {
        return DEFAULT_CONTEXT_WINDOW_LENGTH;
    };

    MODEL_OPTIONS
        .iter()
        .chain(std::iter::once(&GPT_OSS_120B))
        .find(|m| m.model_id == model_id)
        .map_or(DEFAULT_CONTEXT_WINDOW_LENGTH, |m| m.context_window_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_custom_model_bedrock_format() {
        // Test Bedrock format with Claude 3.5
        let result = parse_custom_model("custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0");
        assert!(result.is_some());
        let (region, model) = result.unwrap();
        assert_eq!(region, "us-east-1");
        assert_eq!(model, "CLAUDE_3_7_SONNET_20250219_V1_0");
    }

    #[test]
    fn test_parse_custom_model_q_format() {
        // Test Q Developer format directly
        let result = parse_custom_model("custom:us-west-2:CLAUDE_3_7_SONNET_20250219_V1_0");
        assert!(result.is_some());
        let (region, model) = result.unwrap();
        assert_eq!(region, "us-west-2");
        assert_eq!(model, "CLAUDE_3_7_SONNET_20250219_V1_0");
    }

    #[test]
    fn test_parse_custom_model_claude_4() {
        // Test Claude 4 format
        let result = parse_custom_model("custom:eu-central-1:anthropic.claude-4-sonnet:0");
        assert!(result.is_some());
        let (region, model) = result.unwrap();
        assert_eq!(region, "eu-central-1");
        assert_eq!(model, "CLAUDE_SONNET_4_20250514_V1_0");
    }

    #[test]
    fn test_parse_custom_model_invalid_format() {
        // Test invalid formats
        assert!(parse_custom_model("us-east-1:model").is_none());
        assert!(parse_custom_model("custom:").is_none());
        assert!(parse_custom_model("custom:us-east-1").is_none());
        assert!(parse_custom_model("").is_none());
    }

    #[test]
    fn test_map_bedrock_to_q_model() {
        // Test various Bedrock model mappings
        assert_eq!(
            map_bedrock_to_q_model("us.anthropic.claude-3-5-sonnet-20241022-v2:0"),
            "CLAUDE_3_7_SONNET_20250219_V1_0"
        );

        assert_eq!(
            map_bedrock_to_q_model("anthropic.claude-3-5-sonnet-20241022-v2:0"),
            "CLAUDE_3_7_SONNET_20250219_V1_0"
        );

        assert_eq!(
            map_bedrock_to_q_model("claude-3-5-sonnet-20241022"),
            "CLAUDE_3_7_SONNET_20250219_V1_0"
        );

        assert_eq!(
            map_bedrock_to_q_model("anthropic.claude-4-sonnet:0"),
            "CLAUDE_SONNET_4_20250514_V1_0"
        );

        assert_eq!(
            map_bedrock_to_q_model("claude-4-sonnet"),
            "CLAUDE_SONNET_4_20250514_V1_0"
        );

        // Test passthrough for unknown models
        assert_eq!(map_bedrock_to_q_model("some-unknown-model"), "some-unknown-model");

        // Test passthrough for already-formatted Q models
        assert_eq!(
            map_bedrock_to_q_model("CLAUDE_3_7_SONNET_20250219_V1_0"),
            "CLAUDE_3_7_SONNET_20250219_V1_0"
        );
    }

    #[test]
    fn test_region_extraction() {
        // Test various region formats
        let test_cases = vec![
            ("custom:us-east-1:model", "us-east-1"),
            ("custom:eu-west-1:model", "eu-west-1"),
            ("custom:ap-southeast-2:model", "ap-southeast-2"),
            ("custom:ca-central-1:model", "ca-central-1"),
        ];

        for (input, expected_region) in test_cases {
            let result = parse_custom_model(input);
            assert!(result.is_some());
            let (region, _) = result.unwrap();
            assert_eq!(region, expected_region);
        }
    }

    #[test]
    fn test_complex_model_ids() {
        // Test model IDs with multiple colons
        let result = parse_custom_model("custom:us-east-1:vendor:model:version:0");
        assert!(result.is_some());
        let (region, model) = result.unwrap();
        assert_eq!(region, "us-east-1");
        // Should pass through unknown format as-is
        assert_eq!(model, "vendor:model:version:0");
    }
}
