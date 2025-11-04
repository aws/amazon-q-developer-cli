use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamOutput;
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ConversationRole, Message, SystemContentBlock, Tool, ToolConfiguration,
    ToolInputSchema, ToolSpecification,
};
use eyre::Result;

use crate::api_client::model::{ConversationState, UserInputMessageContext};
use crate::database::settings::Setting;
use crate::database::Database;

#[derive(Clone, Debug)]
pub struct BedrockApiClient {
    client: BedrockClient,
    model_id: String,
    database: Database,
}

impl BedrockApiClient {
    pub async fn new(database: Database) -> Result<Self> {
        let region = database
            .settings
            .get(Setting::BedrockRegion)
            .and_then(|v| v.as_str())
            .unwrap_or("us-east-1");

        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .load()
            .await;

        let client = BedrockClient::new(&config);

        let model_id = database
            .settings
            .get(Setting::BedrockModel)
            .and_then(|v| v.as_str())
            .unwrap_or("anthropic.claude-3-sonnet-20240229-v1:0")
            .to_string();

        Ok(Self {
            client,
            model_id,
            database,
        })
    }

    pub async fn converse_stream(
        &self,
        conversation: ConversationState,
    ) -> Result<ConverseStreamOutput> {
        let ConversationState {
            conversation_id: _,
            user_input_message,
            history,
        } = conversation;

        // Build messages from history and current message
        let mut messages = Vec::new();

        // Add history messages
        if let Some(hist) = history {
            for msg in hist {
                messages.extend(self.convert_chat_message_to_bedrock(msg)?);
            }
        }

        // Add current user message
        let user_content = ContentBlock::Text(user_input_message.content.clone());
        messages.push(
            Message::builder()
                .role(ConversationRole::User)
                .content(user_content)
                .build()
                .map_err(|e| eyre::eyre!("Failed to build message: {}", e))?,
        );

        // Build tool configuration if tools are present
        let tool_config = user_input_message
            .user_input_message_context
            .and_then(|ctx| self.build_tool_configuration(ctx));

        // Get inference parameters
        let temperature = self.get_temperature();
        let max_tokens = self.get_max_tokens();

        // Build the request
        let mut request = self
            .client
            .converse_stream()
            .model_id(&self.model_id)
            .set_messages(Some(messages));

        if let Some(tool_cfg) = tool_config {
            request = request.tool_config(tool_cfg);
        }

        // Set system prompt if configured
        if let Some(system_prompt) = self.get_system_prompt() {
            request = request.system(
                SystemContentBlock::Text(system_prompt)
            );
        }

        // Set inference config
        request = request.inference_config(
            aws_sdk_bedrockruntime::types::InferenceConfiguration::builder()
                .temperature(temperature)
                .max_tokens(max_tokens)
                .build(),
        );

        let response = request.send().await?;
        Ok(response)
    }

    fn convert_chat_message_to_bedrock(
        &self,
        msg: crate::api_client::model::ChatMessage,
    ) -> Result<Vec<Message>> {
        use crate::api_client::model::ChatMessage;

        match msg {
            ChatMessage::UserInputMessage(user_msg) => {
                let mut content_blocks = vec![];
                
                // Add text content
                if !user_msg.content.is_empty() {
                    content_blocks.push(ContentBlock::Text(user_msg.content.clone()));
                }
                
                // Add tool results if present
                if let Some(ctx) = user_msg.user_input_message_context {
                    if let Some(tool_results) = ctx.tool_results {
                        for result in tool_results {
                            let tool_result_content: Vec<_> = result.content.into_iter().filter_map(|c| {
                                match c {
                                    crate::api_client::model::ToolResultContentBlock::Json(doc) => {
                                        // Convert JSON to text representation
                                        Some(aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                                            format!("{:?}", doc)
                                        ))
                                    }
                                    crate::api_client::model::ToolResultContentBlock::Text(text) => {
                                        Some(aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(text))
                                    }
                                }
                            }).collect();
                            
                            let status = match result.status {
                                crate::api_client::model::ToolResultStatus::Success => {
                                    aws_sdk_bedrockruntime::types::ToolResultStatus::Success
                                }
                                crate::api_client::model::ToolResultStatus::Error => {
                                    aws_sdk_bedrockruntime::types::ToolResultStatus::Error
                                }
                            };
                            
                            content_blocks.push(ContentBlock::ToolResult(
                                aws_sdk_bedrockruntime::types::ToolResultBlock::builder()
                                    .tool_use_id(result.tool_use_id)
                                    .set_content(Some(tool_result_content))
                                    .status(status)
                                    .build()
                                    .map_err(|e| eyre::eyre!("Failed to build tool result: {}", e))?
                            ));
                        }
                    }
                }
                
                Ok(vec![Message::builder()
                    .role(ConversationRole::User)
                    .set_content(Some(content_blocks))
                    .build()
                    .map_err(|e| eyre::eyre!("Failed to build user message: {}", e))?])
            }
            ChatMessage::AssistantResponseMessage(assistant_msg) => {
                let mut content_blocks = vec![];
                
                // Add text content
                if !assistant_msg.content.is_empty() {
                    content_blocks.push(ContentBlock::Text(assistant_msg.content));
                }
                
                // Add tool uses
                if let Some(tool_uses) = assistant_msg.tool_uses {
                    for tool_use in tool_uses {
                        content_blocks.push(ContentBlock::ToolUse(
                            aws_sdk_bedrockruntime::types::ToolUseBlock::builder()
                                .tool_use_id(tool_use.tool_use_id)
                                .name(tool_use.name)
                                .input(tool_use.input.into())
                                .build()
                                .map_err(|e| eyre::eyre!("Failed to build tool use: {}", e))?
                        ));
                    }
                }
                
                Ok(vec![Message::builder()
                    .role(ConversationRole::Assistant)
                    .set_content(Some(content_blocks))
                    .build()
                    .map_err(|e| eyre::eyre!("Failed to build assistant message: {}", e))?])
            }
        }
    }

    fn build_tool_configuration(&self, ctx: UserInputMessageContext) -> Option<ToolConfiguration> {
        let tools = ctx.tools?;
        
        let tool_specs: Vec<ToolSpecification> = tools
            .into_iter()
            .filter_map(|tool| {
                match tool {
                    crate::api_client::model::Tool::ToolSpecification(spec) => {
                        let input_schema = if let Some(json_doc) = spec.input_schema.json {
                            ToolInputSchema::Json(json_doc.into())
                        } else {
                            return None;
                        };
                        
                        ToolSpecification::builder()
                            .name(spec.name)
                            .description(spec.description)
                            .input_schema(input_schema)
                            .build()
                            .ok()
                    }
                }
            })
            .collect();

        if tool_specs.is_empty() {
            return None;
        }

        ToolConfiguration::builder()
            .set_tools(Some(tool_specs.into_iter().map(Tool::ToolSpec).collect()))
            .build()
            .ok()
    }

    fn get_temperature(&self) -> f32 {
        // If thinking is enabled, temperature is always 1.0
        if self
            .database
            .settings
            .get(Setting::BedrockThinkingEnabled)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return 1.0;
        }

        // Otherwise use configured temperature or default
        self.database
            .settings
            .get(Setting::BedrockTemperature)
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(1.0)
    }

    fn get_max_tokens(&self) -> i32 {
        self.database
            .settings
            .get(Setting::BedrockContextWindow)
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(4096)
    }

    fn get_system_prompt(&self) -> Option<String> {
        // TODO: Implement custom system prompt loading
        // For now, return None to use default
        None
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn client(&self) -> &BedrockClient {
        &self.client
    }

    pub async fn list_foundation_models(&self) -> Result<Vec<String>> {
        use aws_sdk_bedrockruntime::config::Region;
        
        // Create a Bedrock client (not runtime) for listing models
        let bedrock_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(
                self.database
                    .settings
                    .get(Setting::BedrockRegion)
                    .and_then(|v| v.as_str())
                    .unwrap_or("us-east-1")
                    .to_string(),
            ))
            .load()
            .await;

        let bedrock_client = aws_sdk_bedrock::Client::new(&bedrock_config);

        let response = bedrock_client
            .list_foundation_models()
            .send()
            .await?;

        let models: Vec<String> = response
            .model_summaries()
            .iter()
            .filter(|m| {
                // Filter for Claude models that support converse
                m.model_id().contains("anthropic.claude") && 
                m.inference_types_supported().contains(&aws_sdk_bedrock::types::InferenceType::OnDemand)
            })
            .map(|m| m.model_id().to_string())
            .collect();

        Ok(models)
    }
}
