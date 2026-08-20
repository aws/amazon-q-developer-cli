use crossterm::{
    execute,
    style,
};

use crate::auth::builder_id::is_enterprise_user;
use crate::cli::chat::{
    ChatError,
    ChatSession,
};
use crate::constants::KIRO_APP_URL;
use crate::theme::StyledText;

fn format_limited_usage(used: f64, limit: f64) -> String {
    format!(" ({:.2} of {limit:.0} covered in plan)\n", used.max(0.0))
}

fn rounded_usage_percentage(percentage: f64) -> f64 {
    (percentage.max(0.0) * 10.0).round() / 10.0
}

fn format_usage_percentage(percentage: f64) -> String {
    format!(" {:.1}%\n", percentage.max(0.0))
}

fn bar_fill_width(percentage: f64, bar_width: usize) -> usize {
    (percentage.clamp(0.0, 100.0) / 100.0 * bar_width as f64) as usize
}

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
                    style::Print("Plan: "),
                    StyledText::brand_fg(),
                    style::Print("Q Developer Pro"),
                    StyledText::reset(),
                    style::Print("\n"),
                    style::Print("Your plan is managed by admin\n"),
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
        if billing_data.bonus_credits.len() > 1 {
            execute!(
                session.stderr,
                style::Print("\n"),
                style::SetAttribute(style::Attribute::Bold),
                style::Print("🎁 Bonus Credits:"),
                style::SetAttribute(style::Attribute::Reset),
                style::Print("\n"),
            )?;
            for bonus in &billing_data.bonus_credits {
                execute!(
                    session.stderr,
                    style::Print(format!(
                        "   {} - {:.2}/{:.0} used ({} days left)\n",
                        bonus.name, bonus.used, bonus.total, bonus.days_until_expiry
                    )),
                )?;
            }
            execute!(session.stderr, style::Print("\n"))?;
        } else {
            for bonus in &billing_data.bonus_credits {
                execute!(
                    session.stderr,
                    style::Print("\n"),
                    style::SetAttribute(style::Attribute::Bold),
                    style::Print("🎁 Bonus credits:"),
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
    }

    let is_enterprise = is_enterprise_user(&os.database).await;

    // Display all usage breakdowns
    for breakdown in &billing_data.usage_breakdowns {
        if breakdown.has_limit {
            execute!(
                session.stderr,
                style::SetAttribute(style::Attribute::Bold),
                style::Print(&breakdown.display_name),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(format_limited_usage(breakdown.used, breakdown.limit)),
            )?;

            // Progress bar (only shown when the user has a real limit)
            let percentage = rounded_usage_percentage(breakdown.percentage);
            let window_width = session.terminal_width();
            let bar_width = std::cmp::min(window_width, 80);
            let filled_width = bar_fill_width(percentage, bar_width);
            let empty_width = bar_width.saturating_sub(filled_width);

            // Determine bar color based on percentage
            let bar_color = if percentage >= 100.0 {
                StyledText::error_fg()
            } else if percentage > 90.0 {
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
                style::Print(format_usage_percentage(percentage)),
            )?;
        } else {
            // No limit (e.g. credit-pooling with no per-user cap): show consumption only,
            // no upper bound and no progress bar. The pool balance is never exposed to the user.
            execute!(
                session.stderr,
                style::SetAttribute(style::Attribute::Bold),
                style::Print(&breakdown.display_name),
                style::SetAttribute(style::Attribute::Reset),
                style::Print(format!(" ({:.2} used)\n", breakdown.used.max(0.0))),
            )?;
        }

        execute!(session.stderr, style::Print("\n"))?;
    }

    // Additional credits (individual prepaid-overages model). Only shown to users who can use
    // add-on credits (overage_capability == OVERAGE_CAPABLE) or who already own packs. FREE /
    // OVERAGE_INCAPABLE users and enterprise credit-pooling users do not see this section
    // (enterprise billing is org-managed, so a purchase CTA would contradict the admin message).
    if !is_enterprise && (billing_data.overage_capable || !billing_data.add_on_credits.is_empty()) {
        execute!(
            session.stderr,
            style::SetAttribute(style::Attribute::Bold),
            style::Print("Additional credits"),
            style::SetAttribute(style::Attribute::Reset),
            style::Print("\n"),
        )?;

        if billing_data.add_on_credits.is_empty() {
            execute!(
                session.stderr,
                StyledText::secondary_fg(),
                style::Print("Add-on credits available for purchase\n"),
                StyledText::reset(),
            )?;
        } else {
            // Active pack: detailed line with usage and expiry.
            if let Some(active) = billing_data.add_on_credits.iter().find(|p| p.is_active) {
                let expiry = active
                    .expires_at
                    .as_deref()
                    .map(|d| format!(", expires {d}"))
                    .unwrap_or_default();
                execute!(
                    session.stderr,
                    style::Print(format!("{:.2} of {:.0} used{}\n", active.used, active.total, expiry)),
                )?;
            }

            // Remaining (inactive) packs summed into a single line.
            let inactive: Vec<_> = billing_data.add_on_credits.iter().filter(|p| !p.is_active).collect();
            if !inactive.is_empty() {
                let used: f64 = inactive.iter().map(|p| p.used).sum();
                let total: f64 = inactive.iter().map(|p| p.total).sum();
                execute!(
                    session.stderr,
                    StyledText::secondary_fg(),
                    style::Print(format!(
                        "{:.2} of {:.0} credits across {} pack{}\n",
                        used,
                        total,
                        inactive.len(),
                        if inactive.len() == 1 { "" } else { "s" }
                    )),
                    StyledText::reset(),
                )?;
            }
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
        // Only mention purchasing add-on credits when the user can actually buy them
        // (overage_capability == OVERAGE_CAPABLE). FREE / incapable users just see "manage your plan".
        let cta = if billing_data.overage_capable {
            "To manage your plan or purchase add-on credits navigate to "
        } else {
            "To manage your plan navigate to "
        };
        execute!(
            session.stderr,
            style::Print(cta),
            StyledText::brand_fg(),
            style::Print(KIRO_APP_URL),
            StyledText::reset(),
            style::Print("\n"),
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        bar_fill_width,
        format_limited_usage,
        format_usage_percentage,
        rounded_usage_percentage,
    };

    #[test]
    fn preserves_over_limit_usage() {
        assert_eq!(
            format_limited_usage(1117.31, 1000.0),
            " (1117.31 of 1000 covered in plan)\n"
        );
        assert_eq!(format_usage_percentage(111.731), " 111.7%\n");
    }

    #[test]
    fn clamps_negative_usage_to_zero() {
        assert_eq!(format_limited_usage(-5.0, 1000.0), " (0.00 of 1000 covered in plan)\n");
        assert_eq!(format_usage_percentage(-0.5), " 0.0%\n");
    }

    #[test]
    fn rounds_percentage_once_for_display_and_progress_bar() {
        let percentage = rounded_usage_percentage(99.95);
        assert_eq!(percentage, 100.0);
        assert_eq!(format_usage_percentage(percentage), " 100.0%\n");
        assert_eq!(bar_fill_width(percentage, 80), 80);

        assert_eq!(rounded_usage_percentage(89.97), 90.0);
        assert_eq!(rounded_usage_percentage(90.04), 90.0);
        assert_eq!(rounded_usage_percentage(-0.5), 0.0);
    }

    #[test]
    fn bounds_progress_bar_fill_width() {
        assert_eq!(bar_fill_width(111.731, 80), 80);
        assert_eq!(bar_fill_width(45.0, 80), 36);
        assert_eq!(bar_fill_width(-0.5, 80), 0);
    }
}
