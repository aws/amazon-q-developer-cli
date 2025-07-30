use aws_smithy_runtime_api::client::interceptors::context::InterceptorContext;
use tracing::debug;

use super::error::ApiClientError;

/// Configuration for error classification patterns
#[derive(Debug, Clone)]
pub struct ErrorClassifierConfig {
    /// Error message patterns that indicate monthly limit reached
    pub monthly_limit_patterns: Vec<&'static str>,
    /// Error message patterns that indicate model overload
    pub model_overload_patterns: Vec<&'static str>,
    /// Error message patterns that indicate high load conditions
    pub high_load_patterns: Vec<&'static str>,
    /// Error message patterns that indicate context window overflow
    pub context_overflow_patterns: Vec<&'static str>,
}

impl Default for ErrorClassifierConfig {
    fn default() -> Self {
        Self {
            monthly_limit_patterns: vec!["MONTHLY_REQUEST_COUNT"],
            model_overload_patterns: vec![
                "INSUFFICIENT_MODEL_CAPACITY",
                "I am experiencing high traffic, please try again shortly.",
            ],
            high_load_patterns: vec![
                "Encountered unexpectedly high load when processing the request, please try again.",
            ],
            context_overflow_patterns: vec!["Input is too long."],
        }
    }
}

/// Consolidated error classifier for API responses
#[derive(Debug, Clone)]
pub struct ErrorClassifier {
    config: ErrorClassifierConfig,
}

impl ErrorClassifier {
    /// Create a new error classifier with default configuration
    pub fn new() -> Self {
        Self {
            config: ErrorClassifierConfig::default(),
        }
    }

    /// Classify an error from InterceptorContext (for retry classifier)
    pub fn classify_from_context(&self, ctx: &InterceptorContext) -> Option<ApiClientError> {
        let resp = ctx.response()?;
        let status_code = resp.status().as_u16();
        let response_body = resp.body().bytes().and_then(|bytes| std::str::from_utf8(bytes).ok());

        self.classify_error(status_code, response_body, None, None, None)
    }

    /// Classify an error based on status code, response body, service error metadata, and request
    /// ID
    pub fn classify_error(
        &self,
        status_code: u16,
        response_body: Option<&str>,
        error_code: Option<&str>,
        error_message: Option<&str>,
        request_id: Option<String>,
    ) -> Option<ApiClientError> {
        // Check for context window overflow first (can happen with various status codes)
        if self.is_context_window_overflow(error_code, error_message) {
            debug!("ErrorClassifier: Context window overflow detected");
            return Some(ApiClientError::ContextWindowOverflow {
                status_code: Some(status_code),
            });
        }

        match status_code {
            429 => self.classify_429_error(response_body, error_code, request_id),
            500 => self.classify_500_error(response_body, error_code, request_id),
            503 => {
                debug!("ErrorClassifier: Service unavailable (503) - treating as model overloaded");
                Some(ApiClientError::ModelOverloadedError {
                    request_id,
                    status_code: Some(status_code),
                })
            },
            _ => None,
        }
    }

    /// Classify a 429 (Too Many Requests) error
    fn classify_429_error(
        &self,
        response_body: Option<&str>,
        error_code: Option<&str>,
        request_id: Option<String>,
    ) -> Option<ApiClientError> {
        let body = response_body?;

        // Check for monthly limit first (highest priority)
        if Self::contains_any_pattern(body, &self.config.monthly_limit_patterns) {
            debug!("ErrorClassifier: Monthly limit error detected");
            return Some(ApiClientError::MonthlyLimitReached { status_code: Some(429) });
        }

        // Check for throttling exception with insufficient model capacity
        if self.is_throttling_with_insufficient_capacity(error_code, Some(body)) {
            debug!("ErrorClassifier: Throttling with insufficient capacity detected");
            return Some(ApiClientError::ModelOverloadedError {
                request_id,
                status_code: Some(429),
            });
        }

        // Check for model overload patterns
        if Self::contains_any_pattern(body, &self.config.model_overload_patterns) {
            debug!("ErrorClassifier: Model overload error detected (429)");
            return Some(ApiClientError::ModelOverloadedError {
                request_id,
                status_code: Some(429),
            });
        }

        // Default to general throttling for other 429 errors
        debug!("ErrorClassifier: General throttling error detected");
        Some(ApiClientError::QuotaBreach {
            message: "quota has reached its limit",
            status_code: Some(429),
        })
    }

    /// Classify a 500 (Internal Server Error) error
    fn classify_500_error(
        &self,
        response_body: Option<&str>,
        _error_code: Option<&str>,
        request_id: Option<String>,
    ) -> Option<ApiClientError> {
        let body = response_body?;

        // Check for high load patterns that indicate model overload
        if Self::contains_any_pattern(body, &self.config.high_load_patterns) {
            debug!("ErrorClassifier: Model overload error detected (500)");
            return Some(ApiClientError::ModelOverloadedError {
                request_id,
                status_code: Some(500),
            });
        }

        None
    }

    /// Check if the response body contains any of the given patterns
    fn contains_any_pattern(body: &str, patterns: &[&str]) -> bool {
        patterns.iter().any(|pattern| body.contains(pattern))
    }

    /// Check if an error indicates a context window overflow based on service error metadata
    pub fn is_context_window_overflow(&self, error_code: Option<&str>, error_message: Option<&str>) -> bool {
        error_code == Some("ValidationException")
            && error_message.is_some_and(|msg| Self::contains_any_pattern(msg, &self.config.context_overflow_patterns))
    }

    /// Check if an error is a throttling exception with insufficient model capacity
    pub fn is_throttling_with_insufficient_capacity(
        &self,
        error_code: Option<&str>,
        response_body: Option<&str>,
    ) -> bool {
        error_code == Some("ThrottlingException")
            && response_body.is_some_and(|body| Self::contains_any_pattern(body, &self.config.model_overload_patterns))
    }
}

impl Default for ErrorClassifier {
    fn default() -> Self {
        Self::new()
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

    fn create_context_with_response(status_code: u16, body: &str) -> InterceptorContext {
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());
        let response = Response::builder()
            .status(status_code)
            .body(body)
            .unwrap()
            .map(SdkBody::from);
        ctx.set_response(response.try_into().unwrap());
        ctx
    }

    #[test]
    fn test_monthly_limit_classification() {
        let classifier = ErrorClassifier::new();

        let result = classifier.classify_error(
            429,
            Some(r#"{"error": "MONTHLY_REQUEST_COUNT exceeded"}"#),
            None,
            None,
            Some("test-request-id".to_string()),
        );

        match result {
            Some(ApiClientError::MonthlyLimitReached { status_code }) => {
                assert_eq!(status_code, Some(429));
            },
            _ => panic!("Expected MonthlyLimitReached error"),
        }
    }

    #[test]
    fn test_model_overload_429_classification_with_request_id() {
        let classifier = ErrorClassifier::new();
        let request_id = "test-request-id-123".to_string();

        let result = classifier.classify_error(
            429,
            Some(r#"{"error": "I am experiencing high traffic, please try again shortly."}"#),
            None,
            None,
            Some(request_id.clone()),
        );

        match result {
            Some(ApiClientError::ModelOverloadedError {
                status_code,
                request_id: req_id,
            }) => {
                assert_eq!(status_code, Some(429));
                assert_eq!(req_id, Some(request_id));
            },
            _ => panic!("Expected ModelOverloadedError with request_id"),
        }
    }

    #[test]
    fn test_general_throttling_classification() {
        let classifier = ErrorClassifier::new();

        let result = classifier.classify_error(
            429,
            Some("Too Many Requests - some other error"),
            None,
            None,
            Some("test-request-id".to_string()),
        );

        match result {
            Some(ApiClientError::QuotaBreach { status_code, .. }) => {
                assert_eq!(status_code, Some(429));
            },
            _ => panic!("Expected QuotaBreach error"),
        }
    }

    #[test]
    fn test_model_overload_500_classification_with_request_id() {
        let classifier = ErrorClassifier::new();
        let request_id = "test-request-id-500".to_string();

        let result = classifier.classify_error(
            500,
            Some(r#"{"error": "Encountered unexpectedly high load when processing the request, please try again."}"#),
            None,
            None,
            Some(request_id.clone()),
        );

        match result {
            Some(ApiClientError::ModelOverloadedError {
                status_code,
                request_id: req_id,
            }) => {
                assert_eq!(status_code, Some(500));
                assert_eq!(req_id, Some(request_id));
            },
            _ => panic!("Expected ModelOverloadedError with request_id"),
        }
    }

    #[test]
    fn test_service_unavailable_classification_with_request_id() {
        let classifier = ErrorClassifier::new();
        let request_id = "test-request-id-503".to_string();

        let result = classifier.classify_error(503, Some("Service Unavailable"), None, None, Some(request_id.clone()));

        match result {
            Some(ApiClientError::ModelOverloadedError {
                status_code,
                request_id: req_id,
            }) => {
                assert_eq!(status_code, Some(503));
                assert_eq!(req_id, Some(request_id));
            },
            _ => panic!("Expected ModelOverloadedError for 503 with request_id"),
        }
    }

    #[test]
    fn test_context_window_overflow() {
        let classifier = ErrorClassifier::new();

        let result = classifier.classify_error(
            400,
            None,
            Some("ValidationException"),
            Some("Input is too long."),
            Some("test-request-id".to_string()),
        );

        match result {
            Some(ApiClientError::ContextWindowOverflow { status_code }) => {
                assert_eq!(status_code, Some(400));
            },
            _ => panic!("Expected ContextWindowOverflow"),
        }
    }

    #[test]
    fn test_throttling_with_insufficient_capacity() {
        let classifier = ErrorClassifier::new();
        let request_id = "test-throttling-request-id".to_string();

        let result = classifier.classify_error(
            429,
            Some("INSUFFICIENT_MODEL_CAPACITY"),
            Some("ThrottlingException"),
            None,
            Some(request_id.clone()),
        );

        match result {
            Some(ApiClientError::ModelOverloadedError {
                status_code,
                request_id: req_id,
            }) => {
                assert_eq!(status_code, Some(429));
                assert_eq!(req_id, Some(request_id));
            },
            _ => panic!("Expected ModelOverloadedError for throttling with insufficient capacity"),
        }
    }

    #[test]
    fn test_classify_from_context() {
        let classifier = ErrorClassifier::new();

        let ctx = create_context_with_response(429, r#"{"error": "MONTHLY_REQUEST_COUNT exceeded"}"#);
        let result = classifier.classify_from_context(&ctx);

        match result {
            Some(ApiClientError::MonthlyLimitReached { .. }) => {},
            _ => panic!("Expected MonthlyLimitReached from context"),
        }
    }

    #[test]
    fn test_no_classification_for_other_status_codes() {
        let classifier = ErrorClassifier::new();

        let test_cases = vec![200, 400, 401, 403, 404, 502];
        for status_code in test_cases {
            let result = classifier.classify_error(
                status_code,
                Some("Some error message"),
                None,
                None,
                Some("test-request-id".to_string()),
            );
            assert!(result.is_none(), "Status code {} should return None", status_code);
        }
    }
}
