use aws_config::{BehaviorVersion, Region};
use aws_sdk_bedrockruntime::Client as BedrockClient;

use crate::agent_env::{Session, model_providers::BedrockConverseStreamModelProvider};
use super::cli_interface::CliUi;

pub async fn build_session() -> Result<Session, eyre::Error> {
    println!("Loading AWS configuration...");
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .load()
        .await;
    
    println!("AWS Configuration:");
    println!("  Region: {:?}", config.region());
    
    if config.credentials_provider().is_some() {
        println!("  Credentials provider: configured");
    } else {
        eprintln!("  Credentials provider: NOT FOUND");
        eprintln!("  Please run: aws configure");
        eprintln!("  Or set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY");
        return Err(eyre::eyre!("No AWS credentials provider found"));
    }
    
    let bedrock_client = BedrockClient::new(&config);
    println!("Bedrock client created successfully");
    
    let model_provider = BedrockConverseStreamModelProvider::new(bedrock_client);
    let model_providers = vec![model_provider];
    
    Ok(Session::new(model_providers))
}

pub fn build_ui() -> CliUi {
    CliUi::new()
}
