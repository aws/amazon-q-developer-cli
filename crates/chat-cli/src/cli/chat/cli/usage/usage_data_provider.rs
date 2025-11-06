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

            // Get overage status
            let overages_enabled = usage_limits
                .overage_configuration()
                .is_some_and(|config| config.overage_status().as_str() == "ENABLED");

            // Get billing cycle reset date from main object
            let billing_cycle_reset = usage_limits.next_date_reset().map_or_else(
                || "Billing cycle reset: Unknown".to_string(),
                |next_reset| {
                    let reset_secs = next_reset.secs();
                    let reset_date = DateTime::from_timestamp(reset_secs, 0).unwrap_or_else(Utc::now);
                    format!("Billing cycle reset: {}", reset_date.format("%m/%d"))
                },
            );

            // Process all usage breakdowns
            let mut usage_breakdowns = Vec::new();
            let mut bonus_credits = Vec::new();

            for item in usage_breakdown {
                // Skip items without free trial info
                if item.free_trial_info().is_none() {
                    continue;
                }

                let resource_type = item.resource_type().map_or("Unknown", |rt| rt.as_str()).to_string();
                let display_name = item
                    .display_name_plural()
                    .or_else(|| item.display_name())
                    .unwrap_or(&resource_type)
                    .to_string();
                let used = item.current_usage_with_precision().unwrap_or(0.0);
                let limit = item.usage_limit_with_precision().unwrap_or(0.0);
                let percentage = if limit > 0.0 { (used / limit * 100.0) as i32 } else { 0 };

                usage_breakdowns.push(super::UsageBreakdownInfo {
                    resource_type: resource_type.clone(),
                    display_name: display_name.clone(),
                    used,
                    limit,
                    percentage,
                });

                // Check for bonus credits in this item
                if let Some(free_trial_info) = item.free_trial_info() {
                    if free_trial_info.free_trial_status().map(|s| s.as_str()) == Some("ACTIVE") {
                        let bonus_used = free_trial_info.current_usage_with_precision().unwrap_or(0.0);
                        let bonus_total = free_trial_info.usage_limit_with_precision().unwrap_or(0.0);

                        if let Some(expiry_timestamp) = free_trial_info.free_trial_expiry() {
                            let expiry_secs = expiry_timestamp.secs();
                            let expiry_date = DateTime::from_timestamp(expiry_secs, 0).unwrap_or_else(Utc::now);
                            let now = Utc::now();
                            let days_until_expiry = (expiry_date - now).num_days().max(0);

                            bonus_credits.push(super::BonusCredit {
                                name: display_name,
                                used: bonus_used,
                                total: bonus_total,
                                days_until_expiry,
                            });
                        }
                    }
                }
            }

            Ok(super::BillingUsageData {
                status: super::BillingDataStatus::Available,
                plan_name,
                overages_enabled,
                billing_cycle_reset,
                usage_breakdowns,
                bonus_credits,
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
            })
        },
    }
}
