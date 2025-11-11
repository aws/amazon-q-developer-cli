use crossterm::style::Color;
use crossterm::{
    execute,
    style,
};

use crate::auth::builder_id::is_idc_user;
use crate::cli::chat::{
    ChatError,
    ChatSession,
};
use crate::constants::{
    KIRO_APP_URL,
    KIRO_WEBSITE_URL,
};
use crate::theme::StyledText;

/// Render billing information section
pub async fn render_billing_info(
    billing_data: &super::BillingUsageData,
    session: &mut ChatSession,
    os: &crate::os::Os,
    show_unsupported_message: bool,
) -> Result<(), ChatError> {
    match &billing_data.status {
        super::BillingDataStatus::Available => render_available_billing(billing_data, session, os).await,
        super::BillingDataStatus::FeatureNotSupported => {
            if show_unsupported_message {
                execute!(
                    session.stderr,
                    style::Print(format!("Plan: {}\n", billing_data.plan_name)),
                    style::Print("Upgrade to Kiro for better usage insights through "),
                    StyledText::brand_fg(),
                    style::Print(KIRO_WEBSITE_URL),
                    StyledText::reset(),
                    style::Print("\n"),
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
    os: &crate::os::Os,
) -> Result<(), ChatError> {
    // Header
    execute!(
        session.stderr,
        style::SetAttribute(style::Attribute::Bold),
        style::Print("Estimated Usage"),
        style::SetAttribute(style::Attribute::Reset),
        style::Print(format!(
            " | resets on {} | {}\n",
            billing_data.billing_cycle_reset, billing_data.plan_name
        )),
    )?;

    // Bonus credits
    if !billing_data.bonus_credits.is_empty() {
        for bonus in &billing_data.bonus_credits {
            execute!(
                session.stderr,
                style::Print("\n"),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(format!("🎁 {}:", bonus.name)),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(" "),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(format!("{:.2}/{:.0}", bonus.used, bonus.total)),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(" credits used, expires in "),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(format!("{}", bonus.days_until_expiry)),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(" days\n"),
            )?;
        }

        execute!(session.stderr, style::Print("\n"))?;
    }

    // Display all usage breakdowns
    for breakdown in &billing_data.usage_breakdowns {
        execute!(
            session.stderr,
            style::SetAttribute(style::Attribute::Bold),
            style::Print(&breakdown.display_name),
            style::SetAttribute(style::Attribute::Reset),
            style::Print(format!(
                " ({:.2} of {:.0} covered in plan)\n",
                breakdown.used, breakdown.limit
            )),
        )?;

        // Progress bar
        let bar_width = 60;
        let filled_width = (breakdown.percentage as f32 / 100.0 * bar_width as f32) as usize;
        let empty_width = bar_width - filled_width;

        // Determine bar color based on percentage
        let bar_color = if breakdown.percentage >= 100 {
            StyledText::error_fg()
        } else if breakdown.percentage > 90 {
            StyledText::warning_fg()
        } else {
            StyledText::brand_fg()
        };

        execute!(
            session.stderr,
            bar_color,
            style::Print("█".repeat(filled_width)),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("█".repeat(empty_width)),
            style::SetForegroundColor(Color::Reset),
            style::Print(format!(" {}%\n\n", breakdown.percentage)),
        )?;
    }

    let is_enterprise = is_idc_user(&os.database).await;

    // Overage information (after usage bars)
    execute!(
        session.stderr,
        style::Print("Overages: "),
        style::SetAttribute(style::Attribute::Bold),
        style::Print(if billing_data.overages_enabled {
            "Enabled"
        } else {
            "Disabled"
        }),
        style::SetAttribute(style::Attribute::Reset),
    )?;

    if is_enterprise {
        execute!(
            session.stderr,
            style::Print(" "),
            StyledText::secondary_fg(),
            style::Print("(managed by your organization)"),
            StyledText::reset(),
        )?;
    }

    execute!(session.stderr, style::Print("\n"))?;

    if is_enterprise {
        execute!(
            session.stderr,
            style::Print(
                "\nSince your account is through your organization, for account management please contact your account administrator.\n"
            ),
        )?;
    } else {
        execute!(
            session.stderr,
            style::Print("\nTo manage your plan or configure overages navigate to "),
            StyledText::brand_fg(),
            style::Print(KIRO_APP_URL),
            StyledText::reset(),
            style::Print("\n"),
        )?;
    }

    Ok(())
}
