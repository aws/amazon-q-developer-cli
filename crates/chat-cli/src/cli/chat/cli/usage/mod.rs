use clap::Args;

use crate::cli::chat::token_counter::TokenCount;
use crate::cli::chat::{ChatError, ChatSession, ChatState};
use crate::os::Os;

pub mod usage_data_provider;
pub mod usage_renderer;

/// Detailed usage data for context window analysis
#[derive(Debug)]
pub struct DetailedUsageData {
    pub total_tokens: TokenCount,
    pub context_tokens: TokenCount,
    pub assistant_tokens: TokenCount,
    pub user_tokens: TokenCount,
    pub tools_tokens: TokenCount,
    pub context_window_size: usize,
    pub dropped_context_files: Vec<(String, String)>,
}

/// Arguments for the usage command that displays token usage statistics and context window
/// information.
///
/// This command shows how many tokens are being used by different components (context files, tools,
/// assistant responses, and user prompts) within the current chat session's context window.
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct UsageArgs;

impl UsageArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let usage_data = usage_data_provider::get_detailed_usage_data(session, os).await?;
        usage_renderer::render_context_window(&usage_data, session).await?;
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}