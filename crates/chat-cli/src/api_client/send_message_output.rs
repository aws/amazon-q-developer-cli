use aws_types::request_id::RequestId;

use crate::api_client::ApiClientError;
use crate::api_client::model::ChatResponseStream;

#[derive(Debug)]
pub enum SendMessageOutput {
    Codewhisperer(
        amzn_codewhisperer_streaming_client::operation::generate_assistant_response::GenerateAssistantResponseOutput,
    ),
    QDeveloper(amzn_qdeveloper_streaming_client::operation::send_message::SendMessageOutput),
    Bedrock(aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamOutput),
    Mock(Vec<ChatResponseStream>),
}

impl SendMessageOutput {
    pub fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::QDeveloper(output) => output.request_id(),
            SendMessageOutput::Bedrock(output) => output.request_id(),
            SendMessageOutput::Mock(_) => None,
        }
    }

    pub async fn recv(&mut self) -> Result<Option<ChatResponseStream>, ApiClientError> {
        match self {
            SendMessageOutput::Codewhisperer(output) => Ok(output
                .generate_assistant_response_response
                .recv()
                .await?
                .map(|s| s.into())),
            SendMessageOutput::QDeveloper(output) => Ok(output.send_message_response.recv().await?.map(|s| s.into())),
            SendMessageOutput::Bedrock(output) => {
                use aws_sdk_bedrockruntime::types::ConverseStreamOutput as BedrockStream;
                use crate::api_client::error::{ConverseStreamError, ConverseStreamErrorKind, ConverseStreamSdkError};
                
                let event = output.stream.recv().await
                    .map_err(|e| ApiClientError::ConverseStream(
                        ConverseStreamError::new(
                            ConverseStreamErrorKind::Unknown {
                                reason_code: e.to_string(),
                            },
                            None::<ConverseStreamSdkError>,
                        )
                    ))?;
                
                match event {
                    Some(event) => match event {
                        BedrockStream::ContentBlockDelta(delta) => {
                            if let Some(delta_content) = delta.delta {
                                use aws_sdk_bedrockruntime::types::ContentBlockDelta;
                                match delta_content {
                                    ContentBlockDelta::Text(text) => {
                                        Ok(Some(ChatResponseStream::AssistantResponseEvent {
                                            content: text,
                                        }))
                                    }
                                    ContentBlockDelta::ToolUse(tool_use) => {
                                        Ok(Some(ChatResponseStream::ToolUseEvent {
                                            tool_use_id: delta.content_block_index.to_string(),
                                            name: String::new(),
                                            input: Some(tool_use.input),
                                            stop: None,
                                        }))
                                    }
                                    _ => Ok(Some(ChatResponseStream::Unknown)),
                                }
                            } else {
                                Ok(Some(ChatResponseStream::Unknown))
                            }
                        }
                        BedrockStream::ContentBlockStart(start) => {
                            if let Some(start_content) = start.start {
                                use aws_sdk_bedrockruntime::types::ContentBlockStart;
                                match start_content {
                                    ContentBlockStart::ToolUse(tool_use) => {
                                        Ok(Some(ChatResponseStream::ToolUseEvent {
                                            tool_use_id: tool_use.tool_use_id,
                                            name: tool_use.name,
                                            input: None,
                                            stop: None,
                                        }))
                                    }
                                    _ => Ok(Some(ChatResponseStream::Unknown)),
                                }
                            } else {
                                Ok(Some(ChatResponseStream::Unknown))
                            }
                        }
                        BedrockStream::ContentBlockStop(_) => {
                            Ok(Some(ChatResponseStream::Unknown))
                        }
                        BedrockStream::MessageStart(_) => {
                            Ok(Some(ChatResponseStream::Unknown))
                        }
                        BedrockStream::MessageStop(_) => {
                            Ok(None)
                        }
                        BedrockStream::Metadata(metadata) => {
                            Ok(Some(ChatResponseStream::MessageMetadataEvent {
                                conversation_id: None,
                                utterance_id: metadata.usage.map(|u| format!("{:?}", u)),
                            }))
                        }
                        _ => Ok(Some(ChatResponseStream::Unknown)),
                    },
                    None => Ok(None),
                }
            }
            SendMessageOutput::Mock(vec) => Ok(vec.pop()),
        }
    }
}

impl RequestId for SendMessageOutput {
    fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::QDeveloper(output) => output.request_id(),
            SendMessageOutput::Bedrock(output) => output.request_id(),
            SendMessageOutput::Mock(_) => Some("<mock-request-id>"),
        }
    }
}
