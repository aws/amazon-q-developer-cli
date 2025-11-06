use crossterm::style::Color;
use crossterm::{
    execute,
    style,
};

use crate::cli::chat::{
    ChatError,
    ChatSession,
};

/// Render billing information section
pub async fn render_billing_info(
    billing_data: &super::BillingUsageData,
    session: &mut ChatSession,
    show_unsupported_message: bool,
) -> Result<(), ChatError> {
    match &billing_data.status {
        super::BillingDataStatus::Available => render_available_billing(billing_data, session).await,
        super::BillingDataStatus::FeatureNotSupported => {
            if show_unsupported_message {
                execute!(
                    session.stderr,
                    style::Print("Credit based usage is not supported for your subscription\n"),
                )?;
            }
            Ok(())
        },
        super::BillingDataStatus::BackendError(error_msg) => {
            execute!(
                session.stderr,
                style::SetForegroundColor(style::Color::Yellow),
                style::Print("⚠️  Warning: Could not retrieve usage information from backend\n"),
                style::SetForegroundColor(style::Color::DarkGrey),
                style::Print(format!("Error: {error_msg}\n\n")),
                style::ResetColor,
            )?;
            Ok(())
        },
    }
}

/// Render available billing information
async fn render_available_billing(
    billing_data: &super::BillingUsageData,
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    // Header
    execute!(
        session.stderr,
        style::SetAttribute(style::Attribute::Bold),
        style::Print("Usage details\n"),
        style::SetAttribute(style::Attribute::Reset),
        style::Print("To manage your account, upgrade your plan or configure overages use "),
        style::SetForegroundColor(Color::Blue),
        style::Print("/subscribe"),
        style::SetForegroundColor(Color::Reset),
        style::Print(" to open admin hub\n\n"),
    )?;

    // Bonus credits
    if !billing_data.bonus_credits.is_empty() {
        execute!(
            session.stderr,
            style::SetForegroundColor(Color::Red),
            style::Print("🎁 "),
            style::SetForegroundColor(Color::Reset),
            style::SetAttribute(style::Attribute::Bold),
            style::Print("Bonus credits: "),
            style::SetAttribute(style::Attribute::Reset),
            style::Print(
                "You have bonus credits applied to your account, we will use these first, then your plan credits.\n"
            ),
        )?;

        for bonus in &billing_data.bonus_credits {
            execute!(
                session.stderr,
                style::Print(format!(
                    "{}: {:.2}/{:.0} credits used, expires in {} days\n",
                    bonus.name, bonus.used, bonus.total, bonus.days_until_expiry
                )),
            )?;
        }

        execute!(session.stderr, style::Print("\n"))?;
    }

    // Plan information
    execute!(
        session.stderr,
        style::Print(format!("Current plan: {}\n", billing_data.plan_name)),
    )?;

    // Overage information
    execute!(
        session.stderr,
        style::Print(format!(
            "Overages: {}\n",
            if billing_data.overages_enabled { "On" } else { "Off" }
        )),
    )?;

    execute!(
        session.stderr,
        style::Print(format!("{}\n\n", billing_data.billing_cycle_reset)),
    )?;

    // Display all usage breakdowns
    for breakdown in &billing_data.usage_breakdowns {
        execute!(
            session.stderr,
            style::Print(format!(
                "Current {} usage ({:.2} of {:.0} used)\n",
                breakdown.display_name.to_lowercase(),
                breakdown.used,
                breakdown.limit
            )),
        )?;

        // Progress bar
        let bar_width = 60;
        let filled_width = (breakdown.percentage as f32 / 100.0 * bar_width as f32) as usize;
        let empty_width = bar_width - filled_width;

        execute!(
            session.stderr,
            style::SetForegroundColor(Color::Magenta),
            style::Print("█".repeat(filled_width)),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("█".repeat(empty_width)),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(" {}%\n\n", breakdown.percentage)),
        )?;
    }

    Ok(())
}
