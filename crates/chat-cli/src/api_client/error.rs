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

    // Send message errors
    #[error("{}", SdkErrorDisplay(.0))]
    CodewhispererGenerateAssistantResponse(#[from] SdkError<GenerateAssistantResponseError, HttpResponse>),
    #[error("{}", SdkErrorDisplay(.0))]
    QDeveloperSendMessage(#[from] SdkError<QDeveloperSendMessageError, HttpResponse>),

    // chat stream errors
    #[error("{}", SdkErrorDisplay(.0))]
    CodewhispererChatResponseStream(#[from] SdkError<CodewhispererChatResponseStreamError, RawMessage>),
    #[error("{}", SdkErrorDisplay(.0))]
    QDeveloperChatResponseStream(#[from] SdkError<QDeveloperChatResponseStreamError, RawMessage>),

    // quota breach
    #[error("quota has reached its limit")]
    QuotaBreach {
        message: &'static str,
        status_code: Option<u16>,
    },

    // Separate from quota breach (somehow)
    #[error("monthly query limit reached")]
    MonthlyLimitReached { status_code: Option<u16> },

    #[error("{}", SdkErrorDisplay(.0))]
    CreateSubscriptionToken(#[from] SdkError<CreateSubscriptionTokenError, HttpResponse>),

    /// Returned from the backend when the user input is too large to fit within the model context
    /// window.
    ///
    /// Note that we currently do not receive token usage information regarding how large the
    /// context window is.
    #[error("the context window has overflowed")]
    ContextWindowOverflow { status_code: Option<u16> },

    #[error(transparent)]
    SmithyBuild(#[from] aws_smithy_types::error::operation::BuildError),

    #[error(transparent)]
    ListAvailableProfilesError(#[from] SdkError<ListAvailableProfilesError, HttpResponse>),

    #[error(transparent)]
    AuthError(#[from] AuthError),

    #[error(
        "The model you've selected is temporarily unavailable. Please use '/model' to select a different model and try again."
    )]
    ModelOverloadedError {
        request_id: Option<String>,
        status_code: Option<u16>,
    },

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
            Self::GenerateCompletions(e) => sdk_status_code(e),
            Self::GenerateRecommendations(e) => sdk_status_code(e),
            Self::ListAvailableCustomizations(e) => sdk_status_code(e),
            Self::ListAvailableServices(e) => sdk_status_code(e),
            Self::CodewhispererGenerateAssistantResponse(e) => sdk_status_code(e),
            Self::QDeveloperSendMessage(e) => sdk_status_code(e),
            Self::CodewhispererChatResponseStream(_) => None,
            Self::QDeveloperChatResponseStream(_) => None,
            Self::ListAvailableProfilesError(e) => sdk_status_code(e),
            Self::SendTelemetryEvent(e) => sdk_status_code(e),
            Self::CreateSubscriptionToken(e) => sdk_status_code(e),
            Self::QuotaBreach { status_code, .. } => *status_code,
            Self::ContextWindowOverflow { status_code } => *status_code,
            Self::SmithyBuild(_) => None,
            Self::AuthError(_) => None,
            Self::ModelOverloadedError { status_code, .. } => *status_code,
            Self::MonthlyLimitReached { status_code } => *status_code,
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
            Self::GenerateCompletions(e) => sdk_error_code(e),
            Self::GenerateRecommendations(e) => sdk_error_code(e),
            Self::ListAvailableCustomizations(e) => sdk_error_code(e),
            Self::ListAvailableServices(e) => sdk_error_code(e),
            Self::CodewhispererGenerateAssistantResponse(e) => sdk_error_code(e),
            Self::QDeveloperSendMessage(e) => sdk_error_code(e),
            Self::CodewhispererChatResponseStream(e) => sdk_error_code(e),
            Self::QDeveloperChatResponseStream(e) => sdk_error_code(e),
            Self::ListAvailableProfilesError(e) => sdk_error_code(e),
            Self::SendTelemetryEvent(e) => sdk_error_code(e),
            Self::CreateSubscriptionToken(e) => sdk_error_code(e),
            Self::QuotaBreach { .. } => "QuotaBreachError".to_string(),
            Self::ContextWindowOverflow { .. } => "ContextWindowOverflow".to_string(),
            Self::SmithyBuild(_) => "SmithyBuildError".to_string(),
            Self::AuthError(_) => "AuthError".to_string(),
            Self::ModelOverloadedError { .. } => "ModelOverloadedError".to_string(),
            Self::MonthlyLimitReached { .. } => "MonthlyLimitReached".to_string(),
            Self::Credentials(_) => "CredentialsError".to_string(),
            Self::ListAvailableModelsError(e) => sdk_error_code(e),
            Self::DefaultModelNotFound => "DefaultModelNotFound".to_string(),
            Self::GetProfileError(e) => sdk_error_code(e),
        }
    }
}

fn sdk_error_code<T: ProvideErrorMetadata, R>(e: &SdkError<T, R>) -> String {
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
            ApiClientError::CodewhispererGenerateAssistantResponse(SdkError::service_error(
                GenerateAssistantResponseError::unhandled("<unhandled>"),
                response(),
            )),
            ApiClientError::QDeveloperSendMessage(SdkError::service_error(
                QDeveloperSendMessageError::unhandled("<unhandled>"),
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
