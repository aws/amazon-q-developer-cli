use clap::Args;

use crate::cli::chat::token_counter::TokenCount;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
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

/// Arguments for the usage command that displays credits and billing information.
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct UsageArgs {}

impl UsageArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Only show credits/billing information
        let billing_data = usage_data_provider::get_billing_usage_data(os).await?;
        usage_renderer::render_billing_info(&billing_data, session, true).await?;
        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
