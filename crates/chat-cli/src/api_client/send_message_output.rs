use aws_types::request_id::RequestId;

use crate::api_client::ApiClientError;
use crate::api_client::model::ChatResponseStream;

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SendMessageOutput {
    Codewhisperer(
        amzn_codewhisperer_streaming_client::operation::generate_assistant_response::GenerateAssistantResponseOutput,
    ),
    Mock(Vec<ChatResponseStream>),
    /// Yields the events, then hangs forever — simulates a silently dead connection.
    #[cfg(test)]
    MockSilent(Vec<ChatResponseStream>),
}

impl SendMessageOutput {
    pub fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::Mock(_) => None,
            #[cfg(test)]
            SendMessageOutput::MockSilent(_) => None,
        }
    }

    pub async fn recv(&mut self) -> Result<Option<ChatResponseStream>, ApiClientError> {
        match self {
            SendMessageOutput::Codewhisperer(output) => Ok(output
                .generate_assistant_response_response
                .recv()
                .await?
                .map(|s| s.into())),
            SendMessageOutput::Mock(vec) => Ok(vec.pop()),
            #[cfg(test)]
            SendMessageOutput::MockSilent(vec) => match vec.pop() {
                Some(ev) => Ok(Some(ev)),
                None => std::future::pending().await,
            },
        }
    }
}

impl RequestId for SendMessageOutput {
    fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::Mock(_) => Some("<mock-request-id>"),
            #[cfg(test)]
            SendMessageOutput::MockSilent(_) => Some("<mock-request-id>"),
        }
    }
}
