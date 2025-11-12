use amzn_codewhisperer_client::operation::create_subscription_token::CreateSubscriptionTokenError;
use amzn_codewhisperer_client::operation::generate_completions::GenerateCompletionsError;
use amzn_codewhisperer_client::operation::get_profile::GetProfileError;
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
use aws_credential_types::provider::error::CredentialsError;
use aws_sdk_ssooidc::error::ProvideErrorMetadata;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
pub use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::http::Response;
use aws_smithy_types::event_stream::RawMessage;
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

    #[error(transparent)]
    SmithyBuild(#[from] aws_smithy_types::error::operation::BuildError),

    #[error(transparent)]
    ListAvailableProfilesError(#[from] SdkError<ListAvailableProfilesError, HttpResponse>),

    #[error(transparent)]
    AuthError(#[from] AuthError),

    // Credential errors
    #[error("failed to load credentials: {}", .0)]
    Credentials(CredentialsError),

    #[error(transparent)]
    ListAvailableModelsError(#[from] SdkError<ListAvailableModelsError, HttpResponse>),

    #[error("No default model found in the ListAvailableModels API response")]
    DefaultModelNotFound,

    #[error(transparent)]
    GetProfileError(#[from] SdkError<GetProfileError, HttpResponse>),
}

impl ApiClientError {
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
            Self::Credentials(_e) => None,
            Self::ListAvailableModelsError(e) => sdk_status_code(e),
            Self::DefaultModelNotFound => None,
            Self::GetProfileError(e) => sdk_status_code(e),
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
            Self::Credentials(_) => "CredentialsError".to_string(),
            Self::ListAvailableModelsError(e) => sdk_error_code(e),
            Self::DefaultModelNotFound => "DefaultModelNotFound".to_string(),
            Self::GetProfileError(e) => sdk_error_code(e),
        }
    }
}

#[derive(Debug, Error)]
#[error("{}", .kind)]
pub struct ConverseStreamError {
    pub request_id: Option<String>,
    pub status_code: Option<u16>,
    pub kind: ConverseStreamErrorKind,
    #[source]
    pub source: Option<ConverseStreamSdkError>,
}

impl ConverseStreamError {
    pub fn new(kind: ConverseStreamErrorKind, source: Option<impl Into<ConverseStreamSdkError>>) -> Self {
        Self {
            kind,
            source: source.map(Into::into),
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
            ConverseStreamErrorKind::Unknown { reason_code } => reason_code.clone(),
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
            },
            source: Some(value.into()),
        }
    }
}

#[derive(Debug, Error)]
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
    #[error("An unknown error occurred: {}", .reason_code)]
    Unknown { reason_code: String },
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
                source: Some(ConverseStreamSdkError::CodewhispererGenerateAssistantResponse(
                    SdkError::service_error(GenerateAssistantResponseError::unhandled("<unhandled>"), response()),
                )),
            }),
            ApiClientError::ConverseStream(ConverseStreamError {
                request_id: None,
                status_code: None,
                kind: ConverseStreamErrorKind::Throttling,
                source: Some(ConverseStreamSdkError::QDeveloperSendMessage(SdkError::service_error(
                    QDeveloperSendMessageError::unhandled("<unhandled>"),
                    response(),
                ))),
            }),
            ApiClientError::Credentials(CredentialsError::unhandled("<unhandled>")),
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
}
