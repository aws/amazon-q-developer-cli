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

use crate::auth::AuthError;
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

const OPENAI_MODEL_OPTIONS: [ModelOption; 2] = [
    ModelOption {
        name: "experimental-gpt-oss-120b",
        model_id: "OPENAI_GPT_OSS_120B_1_0",
        context_window_tokens: 128_000,
    },
    ModelOption {
        name: "experimental-gpt-oss-20b",
        model_id: "OPENAI_GPT_OSS_20B_1_0",
        context_window_tokens: 128_000,
    },
];

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
                crossterm::style::SetForegroundColor(session.colors.action())
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
pub async fn get_model_options(os: &Os) -> Result<Vec<ModelOption>, ChatError> {
    let is_amzn_user = BuilderIdToken::load(&os.database)
        .await?
        .ok_or(AuthError::NoToken)?
        .is_amzn_user();

    let mut model_options = MODEL_OPTIONS.into_iter().collect::<Vec<_>>();
    if is_amzn_user {
        for opt in OPENAI_MODEL_OPTIONS {
            model_options.push(opt);
        }
    }

    Ok(model_options)
}

/// Returns the context window length in tokens for the given model_id.
pub fn context_window_tokens(model_id: Option<&str>) -> usize {
    const DEFAULT_CONTEXT_WINDOW_LENGTH: usize = 200_000;

    let Some(model_id) = model_id else {
        return DEFAULT_CONTEXT_WINDOW_LENGTH;
    };

    MODEL_OPTIONS
        .iter()
        .chain(OPENAI_MODEL_OPTIONS.iter())
        .find(|m| m.model_id == model_id)
        .map_or(DEFAULT_CONTEXT_WINDOW_LENGTH, |m| m.context_window_tokens)
}
