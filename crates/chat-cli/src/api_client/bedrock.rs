use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ConverseStreamOutput, Message, SystemContentBlock, ToolConfiguration,
};
use eyre::Result;

use crate::database::Database;
use crate::database::settings::Setting;

pub struct BedrockApiClient {
    client: BedrockClient,
    model_id: String,
}

impl BedrockApiClient {
    pub async fn new(database: &Database) -> Result<Self> {
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

        Ok(Self { client, model_id })
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn client(&self) -> &BedrockClient {
        &self.client
    }
}
