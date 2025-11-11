use crossterm::{
    execute,
    style,
};

use crate::auth::builder_id::is_idc_user;
use crate::cli::chat::{
    ChatError,
    ChatSession,
};
use crate::constants::KIRO_APP_URL;

fn format_billing_rate(amount: f64) -> String {
    format!("${amount:.2}")
}

fn format_cost_with_currency(amount: f64, currency: &str) -> String {
    format!("${amount:.2} {currency}")
}
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
                if billing_data.plan_name != "Unknown" {
                    execute!(
                        session.stderr,
                        style::Print("Plan: "),
                        StyledText::brand_fg(),
                        style::Print(&billing_data.plan_name),
                        StyledText::reset(),
                        style::Print("\n"),
                    )?;
                }
                execute!(
                    session.stderr,
                    style::Print("Upgrade to Kiro for better usage insights through "),
                    StyledText::brand_fg(),
                    style::Print(KIRO_APP_URL),
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
        style::Print(format!(" | resets on {}", billing_data.billing_cycle_reset)),
    )?;

    if billing_data.plan_name != "Unknown" {
        execute!(
            session.stderr,
            style::Print(" | "),
            StyledText::brand_fg(),
            style::Print(&billing_data.plan_name),
            StyledText::reset(),
        )?;
    }

    execute!(session.stderr, style::Print("\n"))?;

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

    let is_enterprise = is_idc_user(&os.database).await;

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
        let window_width = session.terminal_width();
        let bar_width = std::cmp::min(window_width, 80);
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
            StyledText::secondary_fg(),
            style::Print("█".repeat(empty_width)),
            StyledText::reset(),
            style::Print(format!(" {}%\n", breakdown.percentage)),
        )?;

        // Overage information for this breakdown
        execute!(
            session.stderr,
            style::Print("\nOverages: "),
            style::SetAttribute(style::Attribute::Bold),
            style::Print(if billing_data.overages_enabled {
                "Enabled"
            } else {
                "Disabled"
            }),
            style::SetAttribute(style::Attribute::Reset),
        )?;

        // Add billing rate info if overages enabled
        if billing_data.overages_enabled {
            execute!(
                session.stderr,
                style::Print("  "),
                StyledText::secondary_fg(),
                style::Print(format!(
                    "billed at {} per request",
                    format_billing_rate(breakdown.overage_rate)
                )),
                StyledText::reset(),
            )?;
        }

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

        // Add overage usage details if overages enabled
        if billing_data.overages_enabled {
            execute!(
                session.stderr,
                style::Print(format!("Credits used: {:.2}\n", breakdown.current_overages)),
                style::Print(format!(
                    "Est. cost: {}\n",
                    format_cost_with_currency(breakdown.overage_charges, &breakdown.currency)
                )),
            )?;
        }

        execute!(session.stderr, style::Print("\n"))?;
    }

    if is_enterprise {
        execute!(
            session.stderr,
            style::Print(
                "Since your account is through your organization, for account management please contact your account administrator.\n"
            ),
        )?;
    } else {
        execute!(
            session.stderr,
            style::Print("To manage your plan or configure overages navigate to "),
            StyledText::brand_fg(),
            style::Print(KIRO_APP_URL),
            StyledText::reset(),
            style::Print("\n"),
        )?;
    }

    Ok(())
}
