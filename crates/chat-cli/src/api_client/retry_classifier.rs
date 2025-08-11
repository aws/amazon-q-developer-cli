use std::fmt;

use aws_smithy_runtime_api::client::interceptors::context::InterceptorContext;
use aws_smithy_runtime_api::client::retries::classifiers::{
    ClassifyRetry,
    RetryAction,
    RetryClassifierPriority,
};
use tracing::debug;

/// Error marker for monthly limit exceeded errors
const MONTHLY_LIMIT_ERROR_MARKER: &str = "MONTHLY_REQUEST_COUNT";

/// Error message for high load conditions that should be retried
const HIGH_LOAD_ERROR_MESSAGE: &str =
    "Encountered unexpectedly high load when processing the request, please try again.";

/// Error message for insufficient model capacity that should be retried
const INSUFFICIENT_MODEL_CAPACITY_MESSAGE: &str = "I am experiencing high traffic, please try again shortly.";

/// Status codes that indicate service overload/unavailability and should be retried
const SERVICE_OVERLOAD_STATUS_CODES: &[u16] = &[
    429, // Too Many Requests - throttling with insufficient model capacity
    500, // Internal Server Error - requires specific message check for high load conditions
    503, // Service Unavailable - server is temporarily overloaded or under maintenance
];

#[derive(Debug, Default)]
pub struct QCliRetryClassifier;

impl QCliRetryClassifier {
    pub fn new() -> Self {
        Self
    }

    /// Return the priority of this retry classifier.
    ///
    /// We want this to run after the standard classifiers but with high priority
    /// to override their decisions for our specific error cases.
    ///
    /// # Returns
    /// A priority that runs after the transient error classifier but can override its decisions.
    pub fn priority() -> RetryClassifierPriority {
        RetryClassifierPriority::run_after(RetryClassifierPriority::transient_error_classifier())
    }

    /// Check if the error indicates a monthly limit has been reached
    fn is_monthly_limit_error(ctx: &InterceptorContext) -> bool {
        let Some(resp) = ctx.response() else {
            return false;
        };

        // Check status code first - monthly limit errors typically return 429 (Too Many Requests)
        let status_code = resp.status().as_u16();
        if status_code != 429 {
            return false;
        }

        let Some(bytes) = resp.body().bytes() else {
            return false;
        };

        let is_monthly_limit = match std::str::from_utf8(bytes) {
            Ok(body_str) => body_str.contains(MONTHLY_LIMIT_ERROR_MARKER),
            Err(_) => false,
        };

        debug!(
            "QCliRetryClassifier: Monthly limit error detected: {}",
            is_monthly_limit
        );
        is_monthly_limit
    }

    /// Check if the error indicates a model is unavailable due to high load
    fn is_service_overloaded_error(ctx: &InterceptorContext) -> bool {
        let Some(resp) = ctx.response() else {
            return false;
        };

        let status_code = resp.status().as_u16();

        // Fail fast: if status code is not in our list, return false immediately
        if !SERVICE_OVERLOAD_STATUS_CODES.contains(&status_code) {
            return false;
        }

        let is_overloaded = match status_code {
            429 => {
                // For 429 errors, check if the response body contains the insufficient model capacity message
                let Some(bytes) = resp.body().bytes() else {
                    return false;
                };

                match std::str::from_utf8(bytes) {
                    Ok(body_str) => body_str.contains(INSUFFICIENT_MODEL_CAPACITY_MESSAGE),
                    Err(_) => false,
                }
            },
            500 => {
                // For 500 errors, check if the response body contains the specific high load message
                let Some(bytes) = resp.body().bytes() else {
                    return false;
                };

                match std::str::from_utf8(bytes) {
                    Ok(body_str) => body_str.contains(HIGH_LOAD_ERROR_MESSAGE),
                    Err(_) => false,
                }
            },
            503 => {
                // For 503 Service Unavailable, always retry (no additional checks needed)
                true
            },
            _ => {
                // This shouldn't happen given our fail-fast check above, but handle gracefully
                false
            },
        };

        debug!(
            "QCliRetryClassifier: Service overloaded error detected (status {}): {}",
            status_code, is_overloaded
        );
        is_overloaded
    }
}

impl ClassifyRetry for QCliRetryClassifier {
    fn classify_retry(&self, ctx: &InterceptorContext) -> RetryAction {
        // Check for monthly limit error first - this should never be retried
        if Self::is_monthly_limit_error(ctx) {
            return RetryAction::RetryForbidden;
        }

        // Check for service overloaded error - this should be treated as throttling
        if Self::is_service_overloaded_error(ctx) {
            return RetryAction::throttling_error();
        }

        // No specific action for other errors
        RetryAction::NoActionIndicated
    }

    fn name(&self) -> &'static str {
        "Q CLI Custom Retry Classifier"
    }

    fn priority(&self) -> RetryClassifierPriority {
        Self::priority()
    }
}

impl fmt::Display for QCliRetryClassifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QCliRetryClassifier")
    }
}

#[cfg(test)]
mod tests {
    use aws_smithy_runtime_api::client::interceptors::context::{
        Input,
        InterceptorContext,
    };
    use aws_smithy_types::body::SdkBody;
    use http::Response;

    use super::*;

    #[test]
    fn test_monthly_limit_error_classification() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a response with MONTHLY_REQUEST_COUNT in the body
        let response_body = r#"{"error": "MONTHLY_REQUEST_COUNT exceeded"}"#;
        let response = Response::builder()
            .status(429)
            .body(response_body)
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::RetryForbidden);
    }

    #[test]
    fn test_insufficient_model_capacity_error_classification() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a 429 response with the insufficient model capacity message - should be treated as service
        // overloaded
        let response_body = r#"{"error": "I am experiencing high traffic, please try again shortly."}"#;
        let response = Response::builder()
            .status(429)
            .body(response_body)
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::throttling_error());
    }

    #[test]
    fn test_429_error_without_insufficient_capacity_message_not_retried() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a 429 response without the specific insufficient model capacity message - should NOT be
        // retried
        let response_body = "Too Many Requests - some other error";
        let response = Response::builder()
            .status(429)
            .body(response_body)
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::NoActionIndicated);
    }

    #[test]
    fn test_service_overloaded_error_classification() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a 500 response with the specific high load message - should be treated as service
        // overloaded
        let response_body =
            r#"{"error": "Encountered unexpectedly high load when processing the request, please try again."}"#;
        let response = Response::builder()
            .status(500)
            .body(response_body)
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::throttling_error());
    }

    #[test]
    fn test_500_error_without_high_load_message_not_retried() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a 500 response without the specific high load message - should NOT be retried
        let response_body = "Internal Server Error - some other error";
        let response = Response::builder()
            .status(500)
            .body(response_body)
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::NoActionIndicated);
    }

    #[test]
    fn test_service_unavailable_error_classification() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a 503 response - should be treated as service overloaded
        let response_body = "Service Unavailable";
        let response = Response::builder()
            .status(503)
            .body(response_body)
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::throttling_error());
    }

    #[test]
    fn test_no_action_for_non_overload_errors() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a 400 response - should not be treated as service overloaded
        let response = Response::builder()
            .status(400)
            .body("Bad Request")
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::NoActionIndicated);
    }

    #[test]
    fn test_fail_fast_for_non_service_overload_status_codes() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Test various status codes that are not in SERVICE_OVERLOAD_STATUS_CODES
        let test_cases = vec![
            (200, "OK"),
            (400, "Bad Request"),
            (401, "Unauthorized"),
            (403, "Forbidden"),
            (404, "Not Found"),
            (502, "Bad Gateway"),
        ];

        for (status_code, body) in test_cases {
            let response = Response::builder()
                .status(status_code)
                .body(body)
                .unwrap()
                .map(SdkBody::from);

            ctx.set_response(response.try_into().unwrap());

            let result = classifier.classify_retry(&ctx);
            assert_eq!(
                result,
                RetryAction::NoActionIndicated,
                "Status code {} should return NoActionIndicated",
                status_code
            );
        }
    }

    #[test]
    fn test_classifier_priority() {
        let priority = QCliRetryClassifier::priority();
        let transient_priority = RetryClassifierPriority::transient_error_classifier();

        // Our classifier should have higher priority than the transient error classifier
        assert!(priority > transient_priority);
    }

    #[test]
    fn test_classifier_name() {
        let classifier = QCliRetryClassifier::new();
        assert_eq!(classifier.name(), "Q CLI Custom Retry Classifier");
    }
}
