//! /usage command execution

use agent::tui_commands::CommandResult;
use serde_json::json;

use super::CommandContext;

pub async fn execute(ctx: &CommandContext<'_>) -> CommandResult {
    match ctx.api_client.get_usage_limits().await {
        Ok(usage_limits) => {
            // Extract plan info
            let plan_name = usage_limits
                .subscription_info()
                .map_or("Unknown", |si| si.subscription_title())
                .to_string();

            let overages_enabled = usage_limits
                .overage_configuration()
                .is_some_and(|config| config.overage_status().as_str() == "ENABLED");

            // Process usage breakdowns
            let mut breakdowns = Vec::new();
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

                breakdowns.push(json!({
                    "resourceType": resource_type,
                    "displayName": display_name,
                    "used": used,
                    "limit": limit,
                    "percentage": percentage,
                    "currentOverages": current_overages,
                    "overageRate": overage_rate,
                    "overageCharges": overage_charges,
                    "currency": currency
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

            // Check if enterprise
            let is_enterprise = usage_limits
                .overage_configuration()
                .is_some_and(|config| config.overage_status().as_str() == "MANAGED");

            let message = format!("Plan: {} | {} usage breakdowns", plan_name, breakdowns.len());

            CommandResult::success_with_data(
                message,
                json!({
                    "planName": plan_name,
                    "overagesEnabled": overages_enabled,
                    "isEnterprise": is_enterprise,
                    "usageBreakdowns": breakdowns,
                    "bonusCredits": bonuses
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
                        "overagesEnabled": false,
                        "isEnterprise": true,
                        "usageBreakdowns": [],
                        "bonusCredits": []
                    }),
                )
            } else {
                CommandResult::error(format!("Failed to retrieve usage information: {}", err))
            }
        },
    }
}
