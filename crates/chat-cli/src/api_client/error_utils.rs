use amzn_codewhisperer_client::types::AccessDeniedExceptionReason;
use amzn_codewhisperer_client::operation::get_usage_limits::GetUsageLimitsError;

use crate::api_client::ApiClientError;

#[derive(Debug, PartialEq)]
pub enum GetUsageLimitsErrorType {
    FeatureNotSupported,
    Other,
}

/// Classify GetUsageLimits API errors
pub fn classify_get_usage_limits_error(api_error: &ApiClientError) -> GetUsageLimitsErrorType {
    match api_error {
        ApiClientError::GetUsageLimitsError(sdk_err) => {
            match sdk_err.as_service_error() {
                Some(GetUsageLimitsError::AccessDeniedError(access_denied)) => {
                    match access_denied.reason() {
                        Some(AccessDeniedExceptionReason::FeatureNotSupported) => {
                            GetUsageLimitsErrorType::FeatureNotSupported
                        },
                        _ => GetUsageLimitsErrorType::Other,
                    }
                },
                _ => GetUsageLimitsErrorType::Other,
            }
        },
        _ => GetUsageLimitsErrorType::Other,
    }
}
