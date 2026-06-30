use chrono::{
    DateTime,
    Utc,
};

use crate::api_client::error_utils::{
    GetUsageLimitsErrorType,
    classify_get_usage_limits_error,
};
use crate::cli::chat::ChatError;
use crate::os::Os;

/// Sentinel value the backend returns as `usage_limit` for users without a cap. Confirmed with
/// backend that `999999` is canonical: at/above this the user has no limit (progress bar hidden);
/// a value below it is a real cap.
const NO_LIMIT_SENTINEL: f64 = 999_999.0;

/// Get billing usage data from API
pub(super) async fn get_billing_usage_data(os: &Os) -> Result<super::BillingUsageData, ChatError> {
    match os.client.get_usage_limits().await {
        Ok(usage_limits) => {
            let usage_breakdown = usage_limits.usage_breakdown_list();

            // Get plan info
            let plan_name = usage_limits
                .subscription_info()
                .map_or("Unknown", |si| si.subscription_title())
                .to_string();

            // Whether this user is allowed to use/purchase overages (add-on credits).
            // Observed: FREE users are OVERAGE_INCAPABLE — they should not see the purchase prompt.
            let overage_capable = usage_limits
                .subscription_info()
                .is_some_and(|si| si.overage_capability().as_str() == "OVERAGE_CAPABLE");

            // Get overage status
            let overages_enabled = usage_limits
                .overage_configuration()
                .is_some_and(|config| config.overage_status().as_str() == "ENABLED");

            // Get billing cycle reset date from main object
            let billing_cycle_reset = usage_limits.next_date_reset().map_or_else(
                || "Unknown".to_string(),
                |next_reset| {
                    let reset_secs = next_reset.secs();
                    let reset_date = DateTime::from_timestamp(reset_secs, 0).unwrap_or_else(Utc::now);
                    reset_date.format("%Y-%m-%d").to_string()
                },
            );

            // Process all usage breakdowns
            let mut usage_breakdowns = Vec::new();
            let mut bonus_credits = Vec::new();
            let mut add_on_credits: Vec<super::AddOnCreditPack> = Vec::new();

            for item in usage_breakdown {
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

                // No real limit when the backend returns the no-limit sentinel (e.g. credit-pooling
                // users with no per-user cap).
                let has_limit = limit > 0.0 && limit < NO_LIMIT_SENTINEL;

                // Collect purchased add-on credit packs (the single CREDIT breakdown carries them).
                for pack in item.overage_credits() {
                    let expires_at_secs = pack.expires_at().map(|ts| ts.secs());
                    let expires_at = expires_at_secs
                        .and_then(|s| DateTime::from_timestamp(s, 0).map(|d| d.format("%b %d, %Y").to_string()));
                    add_on_credits.push(super::AddOnCreditPack {
                        used: pack.current_usage(),
                        total: pack.usage_limit(),
                        expires_at,
                        expires_at_secs,
                        is_active: false,
                    });
                }

                usage_breakdowns.push(super::UsageBreakdownInfo {
                    resource_type: resource_type.clone(),
                    display_name: display_name.clone(),
                    used,
                    limit,
                    percentage,
                    current_overages,
                    overage_rate,
                    overage_charges,
                    currency,
                    has_limit,
                });

                // Check for welcome bonus (free trial)
                if let Some(free_trial_info) = item.free_trial_info()
                    && free_trial_info.free_trial_status().map(|s| s.as_str()) == Some("ACTIVE")
                {
                    let bonus_used = free_trial_info.current_usage_with_precision().unwrap_or(0.0);
                    let bonus_total = free_trial_info.usage_limit_with_precision().unwrap_or(0.0);

                    if let Some(expiry_timestamp) = free_trial_info.free_trial_expiry() {
                        let expiry_secs = expiry_timestamp.secs();
                        let expiry_date = DateTime::from_timestamp(expiry_secs, 0).unwrap_or_else(Utc::now);
                        let now = Utc::now();
                        let days_until_expiry = (expiry_date - now).num_days().max(0);

                        bonus_credits.push(super::BonusCredit {
                            name: "Welcome bonus".to_string(),
                            used: bonus_used,
                            total: bonus_total,
                            days_until_expiry,
                        });
                    }
                }

                // Check for additional bonuses (only ACTIVE and EXHAUSTED)
                for bonus in item.bonuses() {
                    use amzn_codewhisperer_client::types::BonusStatus;
                    if !matches!(bonus.status(), BonusStatus::Active | BonusStatus::Exhausted) {
                        continue;
                    }

                    let expiry_secs = bonus.expires_at().secs();
                    let expiry_date = DateTime::from_timestamp(expiry_secs, 0).unwrap_or_else(Utc::now);
                    let now = Utc::now();
                    let days_until_expiry = (expiry_date - now).num_days().max(0);

                    bonus_credits.push(super::BonusCredit {
                        name: bonus.display_name().to_string(),
                        used: bonus.current_usage(),
                        total: bonus.usage_limit(),
                        days_until_expiry,
                    });
                }
            }

            // Designate the active pack only once add-on consumption has started: the
            // earliest-expiring pack with remaining credits (FIFO) is the one being drawn from.
            // If no add-on credits have been consumed yet, NO pack is "active" and the UI shows a
            // single combined summary of all packs (per design).
            if let Some(active_idx) = active_pack_index(&add_on_credits) {
                add_on_credits[active_idx].is_active = true;
            }

            Ok(super::BillingUsageData {
                status: super::BillingDataStatus::Available,
                plan_name,
                overages_enabled,
                billing_cycle_reset,
                usage_breakdowns,
                bonus_credits,
                add_on_credits,
                overage_capable,
            })
        },
        Err(err) => {
            // Check if this is an AccessDeniedError with FEATURE_NOT_SUPPORTED reason
            let is_feature_not_supported = matches!(
                classify_get_usage_limits_error(&err),
                GetUsageLimitsErrorType::FeatureNotSupported
            );

            let status = if is_feature_not_supported {
                super::BillingDataStatus::FeatureNotSupported
            } else {
                super::BillingDataStatus::BackendError(err.to_string())
            };

            Ok(super::BillingUsageData {
                status,
                plan_name: "Unknown".to_string(),
                overages_enabled: false,
                billing_cycle_reset: "Unknown".to_string(),
                usage_breakdowns: Vec::new(),
                bonus_credits: Vec::new(),
                add_on_credits: Vec::new(),
                overage_capable: false,
            })
        },
    }
}

/// Index of the active add-on pack — the earliest-expiring pack (by expiry timestamp) that still
/// has remaining credits. Only designated once add-on consumption has started; returns `None`
/// when no pack is being consumed (the UI then shows a single combined summary). Packs with no
/// expiry sort last (FIFO uses earliest expiry first).
fn active_pack_index(packs: &[super::AddOnCreditPack]) -> Option<usize> {
    if !packs.iter().any(|p| p.used > 0.0) {
        return None;
    }
    packs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.used < p.total)
        .min_by(|(_, a), (_, b)| match (a.expires_at_secs, b.expires_at_secs) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::active_pack_index;
    use crate::cli::chat::cli::usage::AddOnCreditPack;

    fn pack(used: f64, total: f64, expires_at_secs: Option<i64>) -> AddOnCreditPack {
        AddOnCreditPack {
            used,
            total,
            expires_at: None,
            expires_at_secs,
            is_active: false,
        }
    }

    #[test]
    fn no_active_pack_when_none_consumed() {
        let packs = vec![pack(0.0, 250.0, Some(200)), pack(0.0, 200.0, Some(100))];
        assert_eq!(active_pack_index(&packs), None);
    }

    #[test]
    fn active_is_earliest_expiry_with_remaining_once_consumed() {
        // Second pack expires earlier (100 < 200) and is being consumed -> active.
        let packs = vec![pack(0.0, 250.0, Some(200)), pack(2.64, 200.0, Some(100))];
        assert_eq!(active_pack_index(&packs), Some(1));
    }

    #[test]
    fn no_active_pack_when_all_exhausted() {
        let packs = vec![pack(250.0, 250.0, Some(200)), pack(200.0, 200.0, Some(100))];
        assert_eq!(active_pack_index(&packs), None);
    }

    #[test]
    fn finite_expiry_wins_over_missing_expiry() {
        // Pack 0 has no expiry (sorts last); pack 1 has a finite expiry and remaining -> active.
        let packs = vec![pack(1.0, 200.0, None), pack(0.0, 250.0, Some(100))];
        assert_eq!(active_pack_index(&packs), Some(1));
    }
}
