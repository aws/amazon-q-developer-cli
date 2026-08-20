use clap::Args;
use crossterm::{
    execute,
    style,
};

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;
use crate::theme::StyledText;

pub mod usage_data_provider;
pub mod usage_renderer;

/// Billing usage data from API
#[derive(Debug)]
pub struct BillingUsageData {
    pub status: BillingDataStatus,
    pub plan_name: String,
    /// Retained from the legacy post-paid overages model; no longer rendered (overages retired).
    #[allow(dead_code)]
    pub overages_enabled: bool,
    pub billing_cycle_reset: String,
    pub usage_breakdowns: Vec<UsageBreakdownInfo>,
    pub bonus_credits: Vec<BonusCredit>,
    /// Purchased prepaid add-on credit packs (individual prepaid-overages model).
    pub add_on_credits: Vec<AddOnCreditPack>,
    /// Whether the user can use/purchase add-on credits (subscription_info.overage_capability ==
    /// OVERAGE_CAPABLE). FREE users are OVERAGE_INCAPABLE and must not see the purchase prompt.
    pub overage_capable: bool,
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
    pub percentage: f64,
    /// Legacy post-paid overage fields; retained from the API but no longer rendered.
    #[allow(dead_code)]
    pub current_overages: f64,
    #[allow(dead_code)]
    pub overage_rate: f64,
    #[allow(dead_code)]
    pub overage_charges: f64,
    #[allow(dead_code)]
    pub currency: String,
    /// Whether this dimension has a real usage limit. False when the backend returns the
    /// "no limit" sentinel (e.g. credit-pooling users with no per-user cap) — the progress
    /// bar is hidden in that case.
    pub has_limit: bool,
}

/// Individual bonus credit information
#[derive(Debug)]
pub struct BonusCredit {
    pub name: String,
    pub used: f64,
    pub total: f64,
    pub days_until_expiry: i64,
}

/// A purchased prepaid add-on credit pack (a.k.a. "Additional credits").
#[derive(Debug)]
pub struct AddOnCreditPack {
    pub used: f64,
    pub total: f64,
    /// Formatted expiry date, e.g. "Jun 18, 2027". `None` if the backend omitted it.
    pub expires_at: Option<String>,
    /// Raw expiry timestamp (epoch seconds) used for chronological FIFO ordering. Display uses
    /// `expires_at`; sorting MUST use this (never compare the formatted string).
    pub expires_at_secs: Option<i64>,
    /// Derived: the single pack currently being consumed (earliest-expiry pack with remaining
    /// credits, FIFO). Only one pack is active at a time; the rest are summed into one line.
    pub is_active: bool,
}

/// Arguments for the usage command that displays credits and billing information.
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct UsageArgs {}

/// Get the current plan name
pub async fn get_plan_name(os: &Os) -> String {
    match usage_data_provider::get_billing_usage_data(os).await {
        Ok(billing_data) => billing_data.plan_name,
        Err(_) => "Unknown".to_string(),
    }
}

impl UsageArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Only show credits/billing information
        let billing_data = usage_data_provider::get_billing_usage_data(os).await?;
        usage_renderer::render_billing_info(&billing_data, session, os, true).await?;
        // Add hint about /context command
        execute!(
            session.stderr,
            style::Print("\n"),
            style::Print("Tip: to see context window usage, run "),
            StyledText::brand_fg(),
            style::Print("/context"),
            StyledText::reset(),
        )?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
