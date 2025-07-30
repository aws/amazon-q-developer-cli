use std::fmt;

use aws_smithy_runtime_api::client::interceptors::context::InterceptorContext;
use aws_smithy_runtime_api::client::retries::classifiers::{
    ClassifyRetry,
    RetryAction,
    RetryClassifierPriority,
};
use tracing::debug;

use super::error::ApiClientError;
use super::error_classifier::ErrorClassifier;

/// Custom retry classifier for Q CLI specific error handling.
///
/// This classifier handles specific error cases by using the consolidated ErrorClassifier:
/// 1. Monthly limit reached errors - classified as RetryForbidden
/// 2. Model overloaded errors (high load, insufficient model capacity) - classified as
///    ThrottlingError
/// 3. All other errors - NoActionIndicated (let other classifiers handle them)
#[derive(Debug, Default)]
pub struct QCliRetryClassifier {
    error_classifier: ErrorClassifier,
}

impl QCliRetryClassifier {
    pub fn new() -> Self {
        Self {
            error_classifier: ErrorClassifier::new(),
        }
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
}

impl ClassifyRetry for QCliRetryClassifier {
    fn classify_retry(&self, ctx: &InterceptorContext) -> RetryAction {
        // Use the consolidated error classifier to determine the error type
        if let Some(error) = self.error_classifier.classify_from_context(ctx) {
            debug!("QCliRetryClassifier: Classified error as: {:?}", error);

            match error {
                // Monthly limit should never be retried
                ApiClientError::MonthlyLimitReached { .. } => {
                    debug!("QCliRetryClassifier: Monthly limit reached - RetryForbidden");
                    RetryAction::RetryForbidden
                },

                // Model overloaded should be retried with throttling
                ApiClientError::ModelOverloadedError { .. } => {
                    debug!("QCliRetryClassifier: Model overloaded - throttling_error");
                    RetryAction::throttling_error()
                },

                // All other classified errors should not be handled by this classifier
                // Let the standard classifiers handle them
                _ => RetryAction::NoActionIndicated,
            }
        } else {
            // No specific action for unclassified errors
            RetryAction::NoActionIndicated
        }
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
    fn test_429_error_without_insufficient_capacity_message_no_action() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Create a 429 response without the specific insufficient model capacity message - should be
        // NoActionIndicated (let standard classifiers handle it)
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
    fn test_context_window_overflow_no_action() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        // Context window overflow should return NoActionIndicated (let standard classifiers handle it)
        let response = Response::builder()
            .status(400)
            .body("ValidationException: Input is too long.")
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::NoActionIndicated);
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

        // Test various status codes that are not handled by the error classifier
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
