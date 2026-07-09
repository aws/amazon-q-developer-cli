//! /usage command execution

use agent::tui_commands::CommandResult;
use serde_json::json;

use super::CommandContext;

/// Sentinel value the backend returns as usage_limit for users without a cap. Confirmed with
/// backend that 999999 is canonical: at/above this the user has no limit (progress bar hidden);
/// a value below it is a real cap.
const NO_LIMIT_SENTINEL: f64 = 999_999.0;

pub async fn execute(ctx: &CommandContext<'_>) -> CommandResult {
    match ctx.api_client.get_usage_limits().await {
        Ok(usage_limits) => {
            if let Some(info) = usage_limits.user_info() {
                let _ = ctx.os.database.set_telemetry_user_id(info.user_id());
            }

            // Extract plan info
            let plan_name = usage_limits
                .subscription_info()
                .map_or("Unknown", |si| si.subscription_title())
                .to_string();

            let billing_cycle_reset = usage_limits.next_date_reset().map_or_else(
                || "Unknown".to_string(),
                |next_reset| {
                    let reset_date =
                        chrono::DateTime::from_timestamp(next_reset.secs(), 0).unwrap_or_else(chrono::Utc::now);
                    reset_date.format("%Y-%m-%d").to_string()
                },
            );

            let overages_enabled = usage_limits
                .overage_configuration()
                .is_some_and(|config| config.overage_status().as_str() == "ENABLED");

            // Whether the user can use/purchase add-on credits. FREE users are OVERAGE_INCAPABLE.
            use amzn_codewhisperer_client::types::OverageCapability;
            let overage_capable = usage_limits
                .subscription_info()
                .is_some_and(|si| matches!(si.overage_capability(), OverageCapability::OverageCapable));

            // Process usage breakdowns + collect purchased add-on credit packs
            let mut breakdowns = Vec::new();
            let mut add_on_packs: Vec<(f64, f64, Option<i64>, Option<String>)> = Vec::new();
            for item in usage_limits.usage_breakdown_list() {
                let resource_type = item.resource_type().map_or("Unknown", |rt| rt.as_str()).to_string();
                let display_name = item
                    .display_name_plural()
                    .or_else(|| item.display_name())
                    .unwrap_or(&resource_type)
                    .to_string();
                let used = item.current_usage_with_precision().unwrap_or(0.0);
                let limit = item.usage_limit_with_precision().unwrap_or(0.0);
                let percentage = if limit > 0.0 { (used / limit * 100.0) as i32 } else { 0 };
                let current_overages = item.current_overages_with_precision().unwrap_or(0.0);
                let overage_rate = item.overage_rate().unwrap_or(0.0);
                let overage_charges = item.overage_charges();
                let currency = item.currency().as_str().to_string();

                // No real limit when the backend returns the no-limit sentinel.
                let has_limit = limit > 0.0 && limit < NO_LIMIT_SENTINEL;

                // Collect add-on credit packs (the single CREDIT breakdown carries them).
                for pack in item.overage_credits() {
                    let expires_secs = pack.expires_at().map(|ts| ts.secs());
                    let expires_str = expires_secs.and_then(|s| {
                        chrono::DateTime::from_timestamp(s, 0).map(|d| d.format("%b %d, %Y").to_string())
                    });
                    add_on_packs.push((pack.current_usage(), pack.usage_limit(), expires_secs, expires_str));
                }

                breakdowns.push(json!({
                    "resourceType": resource_type,
                    "displayName": display_name,
                    "used": used,
                    "limit": limit,
                    "percentage": percentage,
                    "currentOverages": current_overages,
                    "overageRate": overage_rate,
                    "overageCharges": overage_charges,
                    "currency": currency,
                    "hasLimit": has_limit
                }));
            }

            // Process bonus credits
            let mut bonuses = Vec::new();
            for item in usage_limits.usage_breakdown_list() {
                if let Some(free_trial_info) = item.free_trial_info()
                    && free_trial_info.free_trial_status().map(|s| s.as_str()) == Some("ACTIVE")
                {
                    let bonus_used = free_trial_info.current_usage_with_precision().unwrap_or(0.0);
                    let bonus_total = free_trial_info.usage_limit_with_precision().unwrap_or(0.0);
                    if let Some(expiry) = free_trial_info.free_trial_expiry() {
                        let days_left = ((expiry.secs() - chrono::Utc::now().timestamp()) / 86400).max(0);
                        bonuses.push(json!({
                            "name": "Welcome bonus",
                            "used": bonus_used,
                            "total": bonus_total,
                            "daysUntilExpiry": days_left
                        }));
                    }
                }

                for bonus in item.bonuses() {
                    use amzn_codewhisperer_client::types::BonusStatus;
                    if !matches!(bonus.status(), BonusStatus::Active | BonusStatus::Exhausted) {
                        continue;
                    }
                    let days_left = ((bonus.expires_at().secs() - chrono::Utc::now().timestamp()) / 86400).max(0);
                    bonuses.push(json!({
                        "name": bonus.display_name(),
                        "used": bonus.current_usage(),
                        "total": bonus.usage_limit(),
                        "daysUntilExpiry": days_left
                    }));
                }
            }

            // Designate the active pack only once add-on consumption has started (FIFO head with
            // remaining credits). If no add-on credits have been consumed yet, NO pack is active
            // and the UI shows a single combined summary of all packs (per design).
            let active_idx = active_pack_index(&add_on_packs);
            let add_on_credits: Vec<serde_json::Value> = add_on_packs
                .iter()
                .enumerate()
                .map(|(i, (used, total, _, expires_str))| {
                    json!({
                        "used": used,
                        "total": total,
                        "expiresAt": expires_str,
                        "isActive": Some(i) == active_idx
                    })
                })
                .collect();

            let is_enterprise = crate::auth::builder_id::is_enterprise_user(&ctx.os.database).await;

            let message = format!("Plan: {} | {} usage breakdowns", plan_name, breakdowns.len());

            CommandResult::success_with_data(
                message,
                json!({
                    "planName": plan_name,
                    "billingCycleReset": billing_cycle_reset,
                    "overagesEnabled": overages_enabled,
                    "isEnterprise": is_enterprise,
                    "usageBreakdowns": breakdowns,
                    "bonusCredits": bonuses,
                    "addOnCredits": add_on_credits,
                    "overageCapable": overage_capable
                }),
            )
        },
        Err(err) => {
            use crate::api_client::error_utils::{
                GetUsageLimitsErrorType,
                classify_get_usage_limits_error,
            };
            let is_feature_not_supported = matches!(
                classify_get_usage_limits_error(&err),
                GetUsageLimitsErrorType::FeatureNotSupported
            );

            if is_feature_not_supported {
                CommandResult::success_with_data(
                    "Your plan is managed by admin".to_string(),
                    json!({
                        "planName": "Q Developer Pro",
                        "billingCycleReset": "Unknown",
                        "overagesEnabled": false,
                        "isEnterprise": true,
                        "usageBreakdowns": [],
                        "bonusCredits": [],
                        "addOnCredits": [],
                        "overageCapable": false
                    }),
                )
            } else {
                CommandResult::error(format!("Failed to retrieve usage information: {}", err))
            }
        },
    }
}

/// A purchased add-on credit pack: `(used, total, expires_at_secs, expires_at_display)`.
type AddOnPack = (f64, f64, Option<i64>, Option<String>);

/// Index of the active add-on pack — the earliest-expiring pack (by expiry timestamp) that still
/// has remaining credits. Only designated once add-on consumption has started; returns `None`
/// when no pack is being consumed (the UI then shows a single combined summary). Packs with no
/// expiry sort last (FIFO uses earliest expiry first).
fn active_pack_index(packs: &[AddOnPack]) -> Option<usize> {
    if !packs.iter().any(|(used, _, _, _)| *used > 0.0) {
        return None;
    }
    packs
        .iter()
        .enumerate()
        .filter(|(_, (used, total, _, _))| used < total)
        .min_by(|(_, (_, _, a, _)), (_, (_, _, b, _))| match (a, b) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_active_pack_when_none_consumed() {
        let packs: Vec<AddOnPack> = vec![(0.0, 250.0, Some(200), None), (0.0, 200.0, Some(100), None)];
        assert_eq!(active_pack_index(&packs), None);
    }

    #[test]
    fn active_is_earliest_expiry_with_remaining_once_consumed() {
        // Second pack expires earlier (100 < 200) and is being consumed -> active.
        let packs: Vec<AddOnPack> = vec![(0.0, 250.0, Some(200), None), (2.64, 200.0, Some(100), None)];
        assert_eq!(active_pack_index(&packs), Some(1));
    }

    #[test]
    fn no_active_pack_when_all_exhausted() {
        let packs: Vec<AddOnPack> = vec![(250.0, 250.0, Some(200), None), (200.0, 200.0, Some(100), None)];
        assert_eq!(active_pack_index(&packs), None);
    }

    #[test]
    fn finite_expiry_wins_over_missing_expiry() {
        // Pack 0 has no expiry (sorts last); pack 1 has a finite expiry and remaining -> active.
        let packs: Vec<AddOnPack> = vec![(1.0, 200.0, None, None), (0.0, 250.0, Some(100), None)];
        assert_eq!(active_pack_index(&packs), Some(1));
    }
}
