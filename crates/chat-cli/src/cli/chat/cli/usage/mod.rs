use clap::Args;
use crossterm::style::Color;
use crossterm::{execute, style};

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

/// Billing usage data from API
#[derive(Debug)]
pub struct BillingUsageData {
    pub status: BillingDataStatus,
    pub plan_name: String,
    pub overages_enabled: bool,
    pub billing_cycle_reset: String,
    pub usage_breakdowns: Vec<UsageBreakdownInfo>,
    pub bonus_credits: Vec<BonusCredit>,
}

#[derive(Debug)]
pub enum BillingDataStatus {
    Available,
    FeatureNotSupported,
    BackendError(String),
}

/// Individual usage breakdown information
#[derive(Debug)]
pub struct UsageBreakdownInfo {
    #[allow(dead_code)]
    pub resource_type: String,
    pub display_name: String,
    pub used: f64,
    pub limit: f64,
    pub percentage: i32,
}

/// Individual bonus credit information
#[derive(Debug)]
pub struct BonusCredit {
    pub name: String,
    pub used: f64,
    pub total: f64,
    pub days_until_expiry: i64,
}

/// Arguments for the usage command that displays token usage statistics and context window
/// information.
///
/// This command shows how many tokens are being used by different components (context files, tools,
/// assistant responses, and user prompts) within the current chat session's context window.
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct UsageArgs {
    /// Show only context window usage
    #[arg(long)]
    context: bool,
    /// Show only credits and billing information
    #[arg(long)]
    credits: bool,
}

impl UsageArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match (self.context, self.credits) {
            (true, false) => {
                // Show only context window usage
                self.show_context_usage(os, session).await
            },
            (false, true) => {
                // Show only credits/billing information
                self.show_credits_info(os, session).await
            },
            (false, false) => {
                // Show both (default behavior)
                self.show_full_usage(os, session).await
            },
            (true, true) => {
                // Both flags specified - show error
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print("Error: Cannot specify both --context and --credits flags\n"),
                    style::SetForegroundColor(Color::Reset),
                )?;
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            }
        }
    }

    async fn show_context_usage(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let usage_data = usage_data_provider::get_detailed_usage_data(session, os).await?;
        usage_renderer::render_context_window(&usage_data, session).await?;
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }

    async fn show_credits_info(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let billing_data = usage_data_provider::get_billing_usage_data(os).await?;
        usage_renderer::render_billing_info(&billing_data, session, true).await?;
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }

    async fn show_full_usage(&self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Get both billing and context data
        let billing_data = usage_data_provider::get_billing_usage_data(os).await?;
        let usage_data = usage_data_provider::get_detailed_usage_data(session, os).await?;

        // Render billing information
        usage_renderer::render_billing_info(&billing_data, session, false).await?;

        // Render context window information
        usage_renderer::render_context_window(&usage_data, session).await?;
        
        Ok(ChatState::PromptUser { skip_printing_tools: true })
    }
}