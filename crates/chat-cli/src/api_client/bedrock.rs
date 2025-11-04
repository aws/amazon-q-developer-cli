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
                let content = ContentBlock::Text(user_msg.content);
                Ok(vec![Message::builder()
                    .role(ConversationRole::User)
                    .content(content)
                    .build()
                    .map_err(|e| eyre::eyre!("Failed to build user message: {}", e))?])
            }
            ChatMessage::AssistantResponseMessage(_assistant_msg) => {
                // For now, simplified - will need to handle tool use/results
                Ok(vec![])
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
}
