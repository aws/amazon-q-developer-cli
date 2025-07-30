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

/// Status codes that indicate service overload/unavailability and should be retried
const SERVICE_OVERLOAD_STATUS_CODES: &[u16] = &[
    500, // Internal Server Error - server encountered an unexpected condition
    503, // Service Unavailable - server is temporarily overloaded or under maintenance
];

/// Custom retry classifier for Q CLI specific error handling.
///
/// This classifier handles two specific cases:
/// 1. Monthly limit reached errors - classified as RetryForbidden
/// 2. Model unavailable errors - classified as ThrottlingError
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
            debug!("QCliRetryClassifier: No response available in context");
            return false;
        };

        // Check status code first - monthly limit errors typically return 429 (Too Many Requests)
        let status_code = resp.status().as_u16();
        if status_code != 429 {
            debug!(
                "QCliRetryClassifier: Status code {} is not 429, not a monthly limit error",
                status_code
            );
            return false;
        }

        let Some(bytes) = resp.body().bytes() else {
            debug!("QCliRetryClassifier: No response body available");
            return false;
        };

        match std::str::from_utf8(bytes) {
            Ok(body_str) => body_str.contains(MONTHLY_LIMIT_ERROR_MARKER),
            Err(e) => {
                debug!("QCliRetryClassifier: Failed to parse response body as UTF-8: {}", e);
                false
            },
        }
    }

    /// Check if the error indicates a model is unavailable due to high load
    fn is_service_overloaded_error(ctx: &InterceptorContext) -> bool {
        let Some(resp) = ctx.response() else {
            debug!("QCliRetryClassifier: No response available for service overload check");
            return false;
        };

        let status_code = resp.status().as_u16();
        SERVICE_OVERLOAD_STATUS_CODES.contains(&status_code)
    }
}

impl ClassifyRetry for QCliRetryClassifier {
    fn classify_retry(&self, ctx: &InterceptorContext) -> RetryAction {
        // Check for monthly limit error first - this should never be retried
        if Self::is_monthly_limit_error(ctx) {
            debug!("QCliRetryClassifier: Monthly limit error detected - returning RetryForbidden");
            return RetryAction::RetryForbidden;
        }

        // Check for service overloaded error - this should be treated as throttling
        if Self::is_service_overloaded_error(ctx) {
            debug!("QCliRetryClassifier: Service overloaded error detected - returning throttling_error");
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
    fn test_service_overloaded_error_classification() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a 500 response - should be treated as service overloaded
        let response_body = "Internal Server Error";
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
