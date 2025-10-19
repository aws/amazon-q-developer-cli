use aws_sdk_bedrockruntime::{Client as BedrockClient, types::{Message, ConversationRole, ContentBlock, SystemContentBlock}};
use tokio_util::sync::CancellationToken;

use super::model_provider::*;

#[derive(Clone)]
pub struct BedrockConverseStreamModelProvider {
    client: BedrockClient,
    model_id: String,
}

impl BedrockConverseStreamModelProvider {
    pub fn new(client: BedrockClient) -> Self {
        Self {
            client,
            model_id: "us.anthropic.claude-sonnet-4-20250514-v1:0".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl ModelProvider for BedrockConverseStreamModelProvider {
    async fn request(
        &self,
        request: ModelRequest,
        when_receiving_begin: Box<dyn Fn() + Send>,
        when_received: Box<dyn Fn(ModelResponseChunk) + Send>,
        cancellation_token: CancellationToken,
    ) -> Result<ModelResponse, eyre::Error> {
        // Build system content blocks
        let mut system_blocks = Vec::new();
        if let Some(prompt) = request.system_prompt {
            system_blocks.push(SystemContentBlock::Text(prompt));
        }
        if let Some(context) = request.context {
            system_blocks.push(SystemContentBlock::Text(context));
        }

        // Convert messages to Bedrock format
        let messages: Vec<Message> = request.messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    MessageRole::User => ConversationRole::User,
                    MessageRole::Assistant => ConversationRole::Assistant,
                };
                Message::builder()
                    .role(role)
                    .content(ContentBlock::Text(msg.content.clone()))
                    .build()
                    .unwrap()
            })
            .collect();

        // Build request
        let mut request_builder = self.client
            .converse_stream()
            .model_id(&self.model_id)
            .set_messages(Some(messages));

        if !system_blocks.is_empty() {
            request_builder = request_builder.set_system(Some(system_blocks));
        }

        let response = tokio::select! {
            result = request_builder.send() => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("AWS Bedrock request failed:");
                        eprintln!("  Model ID: {}", self.model_id);
                        eprintln!("  Error type: {:?}", e);
                        eprintln!("  Error message: {}", e);
                        
                        if e.to_string().contains("dispatch failure") {
                            eprintln!("  Likely causes:");
                            eprintln!("    - AWS credentials not configured");
                            eprintln!("    - Network connectivity issues");
                            eprintln!("    - AWS region not set or incorrect");
                            eprintln!("    - Bedrock service not available in region");
                        }
                        
                        return Err(eyre::eyre!("Bedrock request failed: {}", e));
                    }
                }
            },
            _ = cancellation_token.cancelled() => {
                return Err(eyre::eyre!("Request cancelled"));
            }
        };

        when_receiving_begin();
        let mut accumulated_content = String::new();
        let mut stream = response.stream;

        loop {
            let event = tokio::select! {
                event = stream.recv() => event,
                _ = cancellation_token.cancelled() => {
                    println!("Model request cancelled during streaming");
                    return Err(eyre::eyre!("Request cancelled"));
                }
            };

            match event {
                Ok(Some(output)) => {
                    if cancellation_token.is_cancelled() {
                        println!("Model request cancelled during chunk processing");
                        return Err(eyre::eyre!("Request cancelled"));
                    }
                    
                    match output {
                        aws_sdk_bedrockruntime::types::ConverseStreamOutput::ContentBlockDelta(delta) => {
                            if let Some(delta_content) = delta.delta {
                                if let aws_sdk_bedrockruntime::types::ContentBlockDelta::Text(text) = delta_content {
                                    accumulated_content.push_str(&text);
                                    when_received(ModelResponseChunk::AssistantMessage(text));
                                }
                            }
                        }
                        aws_sdk_bedrockruntime::types::ConverseStreamOutput::MessageStop(_) => {
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(None) => break,
                Err(e) => return Err(eyre::eyre!("Stream error: {}", e)),
            }
        }

        Ok(ModelResponse {
            content: accumulated_content,
            tool_requests: Vec::new(),
        })
    }
}
