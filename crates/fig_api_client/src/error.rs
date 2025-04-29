use amzn_codewhisperer_client::operation::generate_completions::GenerateCompletionsError;
use amzn_codewhisperer_client::operation::list_available_customizations::ListAvailableCustomizationsError;
pub use amzn_codewhisperer_streaming_client::operation::generate_assistant_response::GenerateAssistantResponseError;
// use amzn_codewhisperer_streaming_client::operation::send_message::SendMessageError as
// CodewhispererSendMessageError;
use amzn_codewhisperer_streaming_client::types::error::ChatResponseStreamError as CodewhispererChatResponseStreamError;
use amzn_consolas_client::operation::generate_recommendations::GenerateRecommendationsError;
use amzn_consolas_client::operation::list_customizations::ListCustomizationsError;
use amzn_qdeveloper_streaming_client::operation::send_message::SendMessageError as QDeveloperSendMessageError;
use amzn_qdeveloper_streaming_client::types::error::ChatResponseStreamError as QDeveloperChatResponseStreamError;
use aws_credential_types::provider::error::CredentialsError;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
pub use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_types::event_stream::RawMessage;
use fig_aws_common::SdkErrorDisplay;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to load credentials: {}", .0)]
    Credentials(CredentialsError),

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
    QuotaBreach(&'static str),

    /// Returned from the backend when the user input is too large to fit within the model context
    /// window.
    ///
    /// Note that we currently do not receive token usage information regarding how large the
    /// context window is.
    #[error("the context window has overflowed")]
    ContextWindowOverflow,

    #[error(transparent)]
    SmithyBuild(#[from] aws_smithy_types::error::operation::BuildError),

    #[error("unsupported action by consolas: {0}")]
    UnsupportedConsolas(&'static str),
}

impl Error {
    pub fn is_throttling_error(&self) -> bool {
        match self {
            Error::Credentials(_) => false,
            Error::GenerateCompletions(e) => e.as_service_error().is_some_and(|e| e.is_throttling_error()),
            Error::GenerateRecommendations(e) => e.as_service_error().is_some_and(|e| e.is_throttling_error()),
            Error::ListAvailableCustomizations(e) => e.as_service_error().is_some_and(|e| e.is_throttling_error()),
            Error::ListAvailableServices(e) => e.as_service_error().is_some_and(|e| e.is_throttling_error()),
            Error::CodewhispererGenerateAssistantResponse(e) => {
                e.as_service_error().is_some_and(|e| e.is_throttling_error())
            },
            Error::QDeveloperSendMessage(e) => e.as_service_error().is_some_and(|e| e.is_throttling_error()),
            Error::CodewhispererChatResponseStream(_)
            | Error::QDeveloperChatResponseStream(_)
            | Error::SmithyBuild(_)
            | Error::UnsupportedConsolas(_)
            | Error::ContextWindowOverflow
            | Error::QuotaBreach(_) => false,
        }
    }

    pub fn is_service_error(&self) -> bool {
        match self {
            Error::Credentials(_) => false,
            Error::GenerateCompletions(e) => e.as_service_error().is_some(),
            Error::GenerateRecommendations(e) => e.as_service_error().is_some(),
            Error::ListAvailableCustomizations(e) => e.as_service_error().is_some(),
            Error::ListAvailableServices(e) => e.as_service_error().is_some(),
            Error::CodewhispererGenerateAssistantResponse(e) => e.as_service_error().is_some(),
            Error::QDeveloperSendMessage(e) => e.as_service_error().is_some(),
            Error::ContextWindowOverflow => true,
            Error::CodewhispererChatResponseStream(_)
            | Error::QDeveloperChatResponseStream(_)
            | Error::SmithyBuild(_)
            | Error::UnsupportedConsolas(_)
            | Error::QuotaBreach(_) => false,
        }
    }
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

    fn all_errors() -> Vec<Error> {
        vec![
            Error::Credentials(CredentialsError::unhandled("<unhandled>")),
            Error::GenerateCompletions(SdkError::service_error(
                GenerateCompletionsError::unhandled("<unhandled>"),
                response(),
            )),
            Error::GenerateRecommendations(SdkError::service_error(
                GenerateRecommendationsError::unhandled("<unhandled>"),
                response(),
            )),
            Error::ListAvailableCustomizations(SdkError::service_error(
                ListAvailableCustomizationsError::unhandled("<unhandled>"),
                response(),
            )),
            Error::ListAvailableServices(SdkError::service_error(
                ListCustomizationsError::unhandled("<unhandled>"),
                response(),
            )),
            Error::CodewhispererGenerateAssistantResponse(SdkError::service_error(
                GenerateAssistantResponseError::unhandled("<unhandled>"),
                response(),
            )),
            Error::QDeveloperSendMessage(SdkError::service_error(
                QDeveloperSendMessageError::unhandled("<unhandled>"),
                response(),
            )),
            Error::CodewhispererChatResponseStream(SdkError::service_error(
                CodewhispererChatResponseStreamError::unhandled("<unhandled>"),
                raw_message(),
            )),
            Error::QDeveloperChatResponseStream(SdkError::service_error(
                QDeveloperChatResponseStreamError::unhandled("<unhandled>"),
                raw_message(),
            )),
            Error::SmithyBuild(aws_smithy_types::error::operation::BuildError::other("<other>")),
            Error::UnsupportedConsolas("test"),
        ]
    }

    #[test]
    fn test_errors() {
        for error in all_errors() {
            let _ = error.is_throttling_error();
            let _ = error.is_service_error();
            let _ = error.source();
            println!("{error} {error:?}");
        }
    }
}
