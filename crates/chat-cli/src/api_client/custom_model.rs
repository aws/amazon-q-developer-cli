use aws_credential_types::provider::ProvideCredentials;
use tracing::{debug, info};

use crate::api_client::credentials::CredentialsChain;

/// Parse custom model format: custom:<region>:<actual-model-id>
/// Example: custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0
fn parse_custom_model(model_id: &str) -> Option<(String, String)> {
    if !model_id.starts_with("custom:") {
        return None;
    }
    
    // Remove "custom:" prefix
    let without_prefix = &model_id[7..];
    
    // Find the first colon to separate region from model ID
    if let Some(colon_pos) = without_prefix.find(':') {
        let region = without_prefix[..colon_pos].to_string();
        let actual_model_id = without_prefix[colon_pos + 1..].to_string();
        
        return Some((region, actual_model_id));
    }
    
    None
}

/// Handle custom model requests using AWS credentials
pub struct CustomModelHandler {
    pub region: String,
    pub actual_model_id: String,
}

impl CustomModelHandler {
    /// Parse a custom model ID string
    /// Format: custom:<region>:<actual-model-id>
    /// Example: custom:us-east-1:us.anthropic.claude-3-5-sonnet-20241022-v2:0
    pub fn from_model_id(model_id: &str) -> Option<Self> {
        parse_custom_model(model_id).map(|(region, actual_model_id)| {
            Self {
                region,
                actual_model_id,
            }
        })
    }

    /// Check if this is a Bedrock/Anthropic model
    pub fn is_bedrock(&self) -> bool {
        self.actual_model_id.contains("anthropic") || 
        self.actual_model_id.contains("claude")
    }

    /// Get the actual model ID for API calls (without custom: prefix)
    pub fn get_model_id(&self) -> &str {
        &self.actual_model_id
    }

    /// Set environment to use AWS credentials
    pub fn setup_aws_auth(&self) {
        // Set the environment variable to use SigV4 authentication
        // Note: Using unsafe as required for dynamic configuration
        unsafe {
            std::env::set_var("AMAZON_Q_SIGV4", "1");
            
            // Set the region if specified
            if !self.region.is_empty() {
                std::env::set_var("AWS_REGION", &self.region);
            }
        }
        
        info!("Configured custom model with AWS authentication: region={}, model={}", 
              self.region, self.actual_model_id);
    }
    
    /// Validate that AWS credentials are available
    pub async fn validate_credentials() -> Result<(), String> {
        let credentials_chain = CredentialsChain::new().await;
        match credentials_chain.provide_credentials().await {
            Ok(_) => {
                debug!("AWS credentials validated successfully");
                Ok(())
            }
            Err(e) => {
                Err(format!("Failed to get AWS credentials: {}", e))
            }
        }
    }
}