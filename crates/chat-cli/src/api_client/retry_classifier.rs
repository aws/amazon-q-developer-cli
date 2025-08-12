use std::fmt;

use aws_smithy_runtime_api::client::interceptors::context::InterceptorContext;
use aws_smithy_runtime_api::client::retries::classifiers::{
    ClassifyRetry,
    RetryAction,
    RetryClassifierPriority,
};
use tracing::debug;

const MONTHLY_LIMIT_ERROR_MARKER: &str = "MONTHLY_REQUEST_COUNT";
const HIGH_LOAD_ERROR_MESSAGE: &str =
    "Encountered unexpectedly high load when processing the request, please try again.";
const SERVICE_UNAVAILABLE_EXCEPTION: &str = "ServiceUnavailableException";

#[derive(Debug, Default)]
pub struct QCliRetryClassifier;

impl QCliRetryClassifier {
    pub fn new() -> Self {
        Self
    }

    pub fn priority() -> RetryClassifierPriority {
        RetryClassifierPriority::run_after(RetryClassifierPriority::transient_error_classifier())
    }

    fn extract_response_body(ctx: &InterceptorContext) -> Option<&str> {
        let bytes = ctx.response()?.body().bytes()?;
        std::str::from_utf8(bytes).ok()
    }

    fn is_monthly_limit_error(body_str: &str) -> bool {
        let is_monthly_limit = body_str.contains(MONTHLY_LIMIT_ERROR_MARKER);
        debug!(
            "QCliRetryClassifier: Monthly limit error detected: {}",
            is_monthly_limit
        );
        is_monthly_limit
    }

    fn is_service_overloaded_error(ctx: &InterceptorContext, body_str: &str) -> bool {
        let Some(resp) = ctx.response() else {
            return false;
        };

        if resp.status().as_u16() != 500 {
            return false;
        }

        let is_overloaded =
            body_str.contains(HIGH_LOAD_ERROR_MESSAGE) || body_str.contains(SERVICE_UNAVAILABLE_EXCEPTION);

        debug!(
            "QCliRetryClassifier: Service overloaded error detected (status 500): {}",
            is_overloaded
        );
        is_overloaded
    }
}

impl ClassifyRetry for QCliRetryClassifier {
    fn classify_retry(&self, ctx: &InterceptorContext) -> RetryAction {
        let Some(body_str) = Self::extract_response_body(ctx) else {
            return RetryAction::NoActionIndicated;
        };

        if Self::is_monthly_limit_error(body_str) {
            return RetryAction::RetryForbidden;
        }

        if Self::is_service_overloaded_error(ctx, body_str) {
            return RetryAction::throttling_error();
        }

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

        let response_body = r#"{"__type":"ThrottlingException","message":"Maximum Request reached for this month.","reason":"MONTHLY_REQUEST_COUNT"}"#;
        let response = Response::builder()
            .status(400)
            .body(response_body)
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::RetryForbidden);
    }

    #[test]
    fn test_service_unavailable_exception_classification() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        let response_body = r#"{"__type":"ServiceUnavailableException","message":"The service is temporarily unavailable. Please try again later."}"#;
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
    fn test_high_load_error_classification() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

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
    fn test_500_error_without_specific_message_not_retried() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        let response_body = r#"{"__type":"InternalServerException","message":"Some other error"}"#;
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
    fn test_no_action_for_other_status_codes() {
        let classifier = QCliRetryClassifier::new();
        let mut ctx = InterceptorContext::new(Input::doesnt_matter());

        let response = Response::builder()
            .status(400)
            .body("Bad Request")
            .unwrap()
            .map(SdkBody::from);

        ctx.set_response(response.try_into().unwrap());

        let result = classifier.classify_retry(&ctx);
        assert_eq!(result, RetryAction::NoActionIndicated);
    }
}
