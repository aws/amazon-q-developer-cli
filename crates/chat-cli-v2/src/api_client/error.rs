use std::sync::Arc;

use amzn_codewhisperer_client::operation::create_subscription_token::CreateSubscriptionTokenError;
use amzn_codewhisperer_client::operation::generate_completions::GenerateCompletionsError;
use amzn_codewhisperer_client::operation::get_profile::GetProfileError;
use amzn_codewhisperer_client::operation::get_usage_limits::GetUsageLimitsError;
use amzn_codewhisperer_client::operation::list_available_customizations::ListAvailableCustomizationsError;
use amzn_codewhisperer_client::operation::list_available_models::ListAvailableModelsError;
use amzn_codewhisperer_client::operation::list_available_profiles::ListAvailableProfilesError;
use amzn_codewhisperer_client::operation::send_telemetry_event::SendTelemetryEventError;
pub use amzn_codewhisperer_streaming_client::operation::generate_assistant_response::GenerateAssistantResponseError;
use amzn_codewhisperer_streaming_client::types::error::ChatResponseStreamError as CodewhispererChatResponseStreamError;
use amzn_consolas_client::operation::generate_recommendations::GenerateRecommendationsError;
use amzn_consolas_client::operation::list_customizations::ListCustomizationsError;
use amzn_qdeveloper_streaming_client::operation::send_message::SendMessageError as QDeveloperSendMessageError;
use amzn_qdeveloper_streaming_client::types::error::ChatResponseStreamError as QDeveloperChatResponseStreamError;
use aws_sdk_ssooidc::error::ProvideErrorMetadata;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
pub use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::http::Response;
use aws_smithy_types::event_stream::RawMessage;
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

use crate::auth::AuthError;
use crate::aws_common::SdkErrorDisplay;
use crate::telemetry::ReasonCode;

#[derive(Debug, Error)]
pub enum ApiClientError {
    /// The converse stream operation
    #[error("{}", .0)]
    ConverseStream(#[from] ConverseStreamError),

    // Generate completions errors
    #[error("{}", SdkErrorDisplay(.0))]
    GenerateCompletions(#[from] SdkError<GenerateCompletionsError, HttpResponse>),
    #[error("{}", SdkErrorDisplay(.0))]
    GenerateRecommendations(#[from] SdkError<GenerateRecommendationsError, HttpResponse>),

    // List customizations error
    #[error("{}", SdkErrorDisplay(.0))]
    ListAvailableCustomizations(#[from] SdkError<ListAvailableCustomizationsError, HttpResponse>),
    #[error("{}", SdkErrorDisplay(.0))]
    ListAvailableServices(#[from] SdkError<ListCustomizationsError, HttpResponse>),

    // Telemetry client error
    #[error("{}", SdkErrorDisplay(.0))]
    SendTelemetryEvent(#[from] SdkError<SendTelemetryEventError, HttpResponse>),

    // chat stream errors
    #[error("{}", SdkErrorDisplay(.0))]
    CodewhispererChatResponseStream(#[from] SdkError<CodewhispererChatResponseStreamError, RawMessage>),
    #[error("{}", SdkErrorDisplay(.0))]
    QDeveloperChatResponseStream(#[from] SdkError<QDeveloperChatResponseStreamError, RawMessage>),

    #[error("{}", SdkErrorDisplay(.0))]
    CreateSubscriptionToken(#[from] SdkError<CreateSubscriptionTokenError, HttpResponse>),

    #[error("{}", SdkErrorDisplay(.0))]
    GetUsageLimitsError(#[from] SdkError<GetUsageLimitsError, HttpResponse>),

    #[error(transparent)]
    SmithyBuild(#[from] aws_smithy_types::error::operation::BuildError),

    #[error(transparent)]
    ListAvailableProfilesError(#[from] SdkError<ListAvailableProfilesError, HttpResponse>),

    #[error(transparent)]
    AuthError(#[from] AuthError),

    #[error(transparent)]
    ListAvailableModelsError(#[from] SdkError<ListAvailableModelsError, HttpResponse>),

    #[error("No default model found in the ListAvailableModels API response")]
    DefaultModelNotFound,

    #[error(transparent)]
    GetProfileError(#[from] SdkError<GetProfileError, HttpResponse>),

    #[error("{0}")]
    Other(String),
}

/// Byte-level check for the two markers the backend uses to signal a context
/// window overflow: the modeled reason enum value and the human-readable
/// validation message (some responses carry only the latter). Only for service
/// error messages and bodies — raw payload scans must use
/// [`contains_overflow_reason_marker`], since arbitrary stream content quoting
/// the English sentence would otherwise be misclassified as a terminal overflow.
pub(crate) fn contains_overflow_marker(bytes: &[u8]) -> bool {
    const MESSAGE_MARKER: &[u8] = b"Input content length exceeds threshold";
    contains_overflow_reason_marker(bytes) || bytes.windows(MESSAGE_MARKER.len()).any(|w| w == MESSAGE_MARKER)
}

/// The structured overflow reason code alone, safe to match against raw payload
/// bytes.
pub(crate) fn contains_overflow_reason_marker(bytes: &[u8]) -> bool {
    const REASON_MARKER: &[u8] = b"CONTENT_LENGTH_EXCEEDS_THRESHOLD";
    bytes.windows(REASON_MARKER.len()).any(|w| w == REASON_MARKER)
}

fn raw_message_has_overflow_marker(raw: Option<&RawMessage>) -> bool {
    match raw {
        Some(RawMessage::Decoded(message)) => contains_overflow_reason_marker(message.payload()),
        Some(RawMessage::Invalid(Some(bytes))) => contains_overflow_reason_marker(bytes),
        _ => false,
    }
}

impl ApiClientError {
    /// Whether this error is a context window overflow, regardless of which layer
    /// surfaced it (send-time classification or a mid-stream exception event).
    /// Overflow is terminal: retrying the same conversation can never succeed, so
    /// callers must compact or fail instead of feeding any retry loop.
    pub fn is_context_window_overflow(&self) -> bool {
        match self {
            Self::ConverseStream(e) => matches!(e.kind, ConverseStreamErrorKind::ContextWindowOverflow),
            Self::CodewhispererChatResponseStream(e) => {
                e.as_service_error()
                    .and_then(|se| se.meta().message())
                    .is_some_and(|m| contains_overflow_marker(m.as_bytes()))
                    || raw_message_has_overflow_marker(e.raw_response())
            },
            Self::QDeveloperChatResponseStream(e) => {
                e.as_service_error()
                    .and_then(|se| se.meta().message())
                    .is_some_and(|m| contains_overflow_marker(m.as_bytes()))
                    || raw_message_has_overflow_marker(e.raw_response())
            },
            _ => false,
        }
    }

    /// Whether this is the backend's mid-stream `InternalServerError`. The
    /// mid-stream error union rides an event-stream frame with no HTTP status,
    /// so a 5xx surfaces only as this modeled variant; recognizing it lets a
    /// mid-stream 5xx feed the same bounded transient retry as a send-path 5xx.
    pub fn is_mid_stream_internal_server_error(&self) -> bool {
        match self {
            Self::CodewhispererChatResponseStream(e) => matches!(
                e.as_service_error(),
                Some(CodewhispererChatResponseStreamError::InternalServerError(_))
            ),
            Self::QDeveloperChatResponseStream(e) => matches!(
                e.as_service_error(),
                Some(QDeveloperChatResponseStreamError::InternalServerError(_))
            ),
            _ => false,
        }
    }

    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::ConverseStream(e) => e.status_code,
            Self::GenerateCompletions(e) => sdk_status_code(e),
            Self::GenerateRecommendations(e) => sdk_status_code(e),
            Self::ListAvailableCustomizations(e) => sdk_status_code(e),
            Self::ListAvailableServices(e) => sdk_status_code(e),
            Self::CodewhispererChatResponseStream(_) => None,
            Self::QDeveloperChatResponseStream(_) => None,
            Self::ListAvailableProfilesError(e) => sdk_status_code(e),
            Self::SendTelemetryEvent(e) => sdk_status_code(e),
            Self::CreateSubscriptionToken(e) => sdk_status_code(e),
            Self::SmithyBuild(_) => None,
            Self::AuthError(_) => None,
            Self::ListAvailableModelsError(e) => sdk_status_code(e),
            Self::DefaultModelNotFound => None,
            Self::GetProfileError(e) => sdk_status_code(e),
            Self::GetUsageLimitsError(e) => sdk_status_code(e),
            Self::Other(_) => None,
        }
    }

    /// Whether this error is a transient network failure (e.g. connection reset) that occurred
    /// while receiving a response stream, making the request safe to retry.
    pub fn is_transient_stream_failure(&self) -> bool {
        match self {
            Self::CodewhispererChatResponseStream(SdkError::DispatchFailure(e)) => e.is_io() || e.is_timeout(),
            Self::QDeveloperChatResponseStream(SdkError::DispatchFailure(e)) => e.is_io() || e.is_timeout(),
            _ => false,
        }
    }
}

impl ReasonCode for ApiClientError {
    fn reason_code(&self) -> String {
        match self {
            Self::ConverseStream(e) => e.reason_code(),
            Self::GenerateCompletions(e) => sdk_error_code(e),
            Self::GenerateRecommendations(e) => sdk_error_code(e),
            Self::ListAvailableCustomizations(e) => sdk_error_code(e),
            Self::ListAvailableServices(e) => sdk_error_code(e),
            Self::CodewhispererChatResponseStream(e) => sdk_error_code(e),
            Self::QDeveloperChatResponseStream(e) => sdk_error_code(e),
            Self::ListAvailableProfilesError(e) => sdk_error_code(e),
            Self::SendTelemetryEvent(e) => sdk_error_code(e),
            Self::CreateSubscriptionToken(e) => sdk_error_code(e),
            Self::SmithyBuild(_) => "SmithyBuildError".to_string(),
            Self::AuthError(_) => "AuthError".to_string(),
            Self::ListAvailableModelsError(e) => sdk_error_code(e),
            Self::DefaultModelNotFound => "DefaultModelNotFound".to_string(),
            Self::GetProfileError(e) => sdk_error_code(e),
            Self::GetUsageLimitsError(e) => sdk_error_code(e),
            Self::Other(_) => "Other".to_string(),
        }
    }
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[error("{}", .kind)]
pub struct ConverseStreamError {
    pub request_id: Option<String>,
    pub status_code: Option<u16>,
    pub kind: ConverseStreamErrorKind,
    #[source]
    #[serde(skip)]
    pub source: Option<Arc<ConverseStreamSdkError>>,
}

impl ConverseStreamError {
    pub fn new(kind: ConverseStreamErrorKind, source: Option<impl Into<ConverseStreamSdkError>>) -> Self {
        Self {
            kind,
            source: source.map(|s| Arc::new(s.into())),
            request_id: None,
            status_code: None,
        }
    }

    pub fn set_request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id;
        self
    }

    pub fn set_status_code(mut self, status_code: Option<u16>) -> Self {
        self.status_code = status_code;
        self
    }
}

impl ReasonCode for ConverseStreamError {
    fn reason_code(&self) -> String {
        match &self.kind {
            // maintaining backwards compatibility with the previous throttling error code.
            ConverseStreamErrorKind::Throttling => "QuotaBreachError".to_string(),
            ConverseStreamErrorKind::MonthlyLimitReached => "MonthlyLimitReached".to_string(),
            ConverseStreamErrorKind::ContextWindowOverflow => "ContextWindowOverflow".to_string(),
            ConverseStreamErrorKind::ModelOverloadedError => "ModelOverloadedError".to_string(),
            ConverseStreamErrorKind::AccessDenied { .. } => "AccessDenied".to_string(),
            ConverseStreamErrorKind::InvalidModelId { .. } => "InvalidModelId".to_string(),
            ConverseStreamErrorKind::Unknown { reason_code, .. } => reason_code.clone(),
        }
    }
}

impl From<aws_smithy_types::error::operation::BuildError> for ConverseStreamError {
    fn from(value: aws_smithy_types::error::operation::BuildError) -> Self {
        Self {
            request_id: None,
            status_code: None,
            kind: ConverseStreamErrorKind::Unknown {
                reason_code: value.to_string(),
                message: None,
            },
            source: Some(Arc::new(value.into())),
        }
    }
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ConverseStreamErrorKind {
    #[error("Too many requests have been sent recently, please wait and try again later")]
    Throttling,
    #[error("The monthly usage limit has been reached")]
    MonthlyLimitReached,
    /// Returned from the backend when the user input is too large to fit within the model context
    /// window.
    ///
    /// Note that we currently do not receive token usage information regarding how large the
    /// context window is.
    #[error("The context window has overflowed")]
    ContextWindowOverflow,
    #[error(
        "The model you've selected is temporarily unavailable. Please use '/model' to select a different model and try again."
    )]
    ModelOverloadedError,
    /// Authentication was rejected: HTTP 401, or a 403 / `AccessDeniedException`
    /// whose payload is credential-shaped (see [`is_credential_denial_marker`]).
    ///
    /// Produced only send-path (at request build/dispatch); on a long multi-cycle
    /// turn this commonly means the access token expired between requests. Modeled
    /// distinctly (V2 previously collapsed 403 to `Unknown`) so it can be messaged
    /// clearly and drive the one-shot token refresh. Non-credential denials (e.g.
    /// gated features) stay `Unknown` so the service's own explanation renders and
    /// no refresh is wasted on a failure it cannot fix.
    #[error("{}", format_access_denied(.message.as_deref()))]
    AccessDenied {
        /// User-friendly message from the service, if available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Returned from the backend when the request specifies a model id that is
    /// not allowed in the current inference path (e.g. removed or gated).
    ///
    /// Corresponds to `ValidationException` with `reason == INVALID_MODEL_ID`.
    /// `model_id` carries the rejected id (from the outbound request) when known,
    /// so we can surface a more specific message to the user.
    #[error("{}", format_invalid_model_id(.model_id.as_deref()))]
    InvalidModelId { model_id: Option<String> },
    #[error("{}", format_unknown_error(.reason_code, .message.as_deref()))]
    Unknown {
        reason_code: String,
        /// User-friendly message from the service, if available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

fn format_access_denied(message: Option<&str>) -> String {
    match message {
        Some(msg) => format!("Access denied: {msg}"),
        None => "Authentication failed. Your credentials may be invalid or expired.".to_string(),
    }
}

/// Whether an access-denial payload looks like a credential problem (expired or
/// invalid token) rather than a policy denial such as a gated feature or an
/// unauthorized profile. Only credential-shaped denials should be reported as
/// expired credentials and drive a token refresh.
pub(crate) fn is_credential_denial_marker(bytes: &[u8]) -> bool {
    contains_ignore_ascii_case(bytes, b"expired")
        || contains_ignore_ascii_case(bytes, b"invalid token")
        || contains_ignore_ascii_case(bytes, b"invalid bearer token")
        || contains_ignore_ascii_case(bytes, b"unauthenticated")
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

fn format_invalid_model_id(model_id: Option<&str>) -> String {
    match model_id {
        Some(id) => {
            format!("The model '{id}' is not available. Please use '/model' to select a different model and try again.")
        },
        None => "The selected model is not available. Please use '/model' to select a different model and try again."
            .to_string(),
    }
}

fn format_unknown_error(reason_code: &str, message: Option<&str>) -> String {
    match message {
        Some(msg) => msg.to_string(),
        None => format!("An unknown error occurred: {}", reason_code),
    }
}

#[derive(Debug, Error)]
pub enum ConverseStreamSdkError {
    #[error("{}", SdkErrorDisplay(.0))]
    CodewhispererGenerateAssistantResponse(#[from] SdkError<GenerateAssistantResponseError, HttpResponse>),
    #[error("{}", SdkErrorDisplay(.0))]
    QDeveloperSendMessage(#[from] SdkError<QDeveloperSendMessageError, HttpResponse>),
    #[error(transparent)]
    SmithyBuild(#[from] aws_smithy_types::error::operation::BuildError),
}

pub fn sdk_error_code<T: ProvideErrorMetadata, R>(e: &SdkError<T, R>) -> String {
    e.as_service_error()
        .and_then(|se| se.meta().code().map(str::to_string))
        .unwrap_or_else(|| e.to_string())
}

fn sdk_status_code<E>(e: &SdkError<E, Response>) -> Option<u16> {
    e.raw_response().map(|res| res.status().as_u16())
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use aws_smithy_runtime_api::http::Response;
    use aws_smithy_types::body::SdkBody;
    use aws_smithy_types::event_stream::Message;

    use super::*;

    fn response() -> Response {
        Response::new(500.try_into().unwrap(), SdkBody::empty())
    }

    fn raw_message() -> RawMessage {
        RawMessage::Decoded(Message::new(b"<payload>".to_vec()))
    }

    fn all_errors() -> Vec<ApiClientError> {
        vec![
            ApiClientError::ConverseStream(ConverseStreamError {
                request_id: None,
                status_code: None,
                kind: ConverseStreamErrorKind::Throttling,
                source: Some(Arc::new(
                    ConverseStreamSdkError::CodewhispererGenerateAssistantResponse(SdkError::service_error(
                        GenerateAssistantResponseError::unhandled("<unhandled>"),
                        response(),
                    )),
                )),
            }),
            ApiClientError::ConverseStream(ConverseStreamError {
                request_id: None,
                status_code: None,
                kind: ConverseStreamErrorKind::Throttling,
                source: Some(Arc::new(ConverseStreamSdkError::QDeveloperSendMessage(
                    SdkError::service_error(QDeveloperSendMessageError::unhandled("<unhandled>"), response()),
                ))),
            }),
            ApiClientError::GenerateCompletions(SdkError::service_error(
                GenerateCompletionsError::unhandled("<unhandled>"),
                response(),
            )),
            ApiClientError::GenerateRecommendations(SdkError::service_error(
                GenerateRecommendationsError::unhandled("<unhandled>"),
                response(),
            )),
            ApiClientError::ListAvailableCustomizations(SdkError::service_error(
                ListAvailableCustomizationsError::unhandled("<unhandled>"),
                response(),
            )),
            ApiClientError::GetProfileError(SdkError::service_error(
                GetProfileError::unhandled("<unhandled>"),
                response(),
            )),
            ApiClientError::ListAvailableModelsError(SdkError::service_error(
                ListAvailableModelsError::unhandled("<unhandled>"),
                response(),
            )),
            ApiClientError::ListAvailableServices(SdkError::service_error(
                ListCustomizationsError::unhandled("<unhandled>"),
                response(),
            )),
            ApiClientError::CreateSubscriptionToken(SdkError::service_error(
                CreateSubscriptionTokenError::unhandled("<unhandled>"),
                response(),
            )),
            ApiClientError::CodewhispererChatResponseStream(SdkError::service_error(
                CodewhispererChatResponseStreamError::unhandled("<unhandled>"),
                raw_message(),
            )),
            ApiClientError::QDeveloperChatResponseStream(SdkError::service_error(
                QDeveloperChatResponseStreamError::unhandled("<unhandled>"),
                raw_message(),
            )),
            ApiClientError::SmithyBuild(aws_smithy_types::error::operation::BuildError::other("<other>")),
        ]
    }

    #[test]
    fn test_errors() {
        for error in all_errors() {
            let _ = error.source();
            println!("{error} {error:?}");
        }
    }

    #[test]
    fn test_is_transient_stream_failure() {
        use aws_smithy_runtime_api::client::result::{
            ConnectorError,
            DispatchFailure,
        };

        let io_failure = || {
            DispatchFailure::builder()
                .source(ConnectorError::io("connection reset by peer".into()))
                .build()
        };
        assert!(
            ApiClientError::CodewhispererChatResponseStream(SdkError::DispatchFailure(io_failure()))
                .is_transient_stream_failure()
        );
        assert!(
            ApiClientError::QDeveloperChatResponseStream(SdkError::DispatchFailure(io_failure()))
                .is_transient_stream_failure()
        );

        // Service-modeled errors and other variants are not transient.
        for error in all_errors() {
            assert!(!error.is_transient_stream_failure(), "{error:?}");
        }
    }

    #[test]
    fn overflow_detected_from_converse_stream_kind() {
        let err = ApiClientError::ConverseStream(ConverseStreamError {
            request_id: None,
            status_code: Some(400),
            kind: ConverseStreamErrorKind::ContextWindowOverflow,
            source: None,
        });
        assert!(err.is_context_window_overflow());
    }

    #[test]
    fn overflow_detected_from_mid_stream_raw_payload() {
        // The mid-stream exception surfaces as an event-stream SdkError whose raw
        // message payload carries the validation reason marker.
        let raw = RawMessage::Decoded(Message::new(
            br#"{"__type":"ValidationException","message":"Input content length exceeds threshold","reason":"CONTENT_LENGTH_EXCEEDS_THRESHOLD"}"#.to_vec(),
        ));
        let err = ApiClientError::CodewhispererChatResponseStream(SdkError::service_error(
            CodewhispererChatResponseStreamError::unhandled("<unhandled>"),
            raw,
        ));
        assert!(err.is_context_window_overflow());
    }

    #[test]
    fn non_overflow_errors_are_not_classified_as_overflow() {
        for error in all_errors() {
            assert!(
                !error.is_context_window_overflow(),
                "should not classify as overflow: {error:?}"
            );
        }
    }

    #[test]
    fn raw_payload_quoting_overflow_prose_is_not_overflow() {
        // A stream frame that merely quotes the human-readable overflow sentence
        // (e.g. assistant output discussing this very error) must not be
        // classified as a terminal overflow; only the structured reason code
        // counts on raw payload bytes.
        let raw = RawMessage::Decoded(Message::new(
            br#"{"content":"the backend replied: Input content length exceeds threshold"}"#.to_vec(),
        ));
        let err = ApiClientError::CodewhispererChatResponseStream(SdkError::service_error(
            CodewhispererChatResponseStreamError::unhandled("<unhandled>"),
            raw,
        ));
        assert!(!err.is_context_window_overflow());
    }

    #[test]
    fn credential_denial_marker_matches_credential_shapes_only() {
        assert!(is_credential_denial_marker(
            b"The bearer token included in the request is EXPIRED"
        ));
        assert!(is_credential_denial_marker(b"Invalid Token provided"));
        assert!(is_credential_denial_marker(b"request is unauthenticated"));
        assert!(!is_credential_denial_marker(
            b"You do not have access to this feature; contact your administrator"
        ));
    }
}
