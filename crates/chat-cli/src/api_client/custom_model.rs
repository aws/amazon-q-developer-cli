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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

// Add Debug trait implementation for better test output
impl std::fmt::Debug for CustomModelHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomModelHandler")
            .field("region", &self.region)
            .field("actual_model_id", &self.actual_model_id)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_model_handler_creation() {
        let handler = CustomModelHandler {
            region: "us-east-1".to_string(),
            actual_model_id: "CLAUDE_3_7_SONNET_20250219_V1_0".to_string(),
        };
        
        assert_eq!(handler.region, "us-east-1");
        assert_eq!(handler.actual_model_id, "CLAUDE_3_7_SONNET_20250219_V1_0");
    }

    #[test]
    fn test_from_model_id() {
        let handler = CustomModelHandler::from_model_id("custom:us-west-2:test-model-id");
        assert!(handler.is_some());
        let handler = handler.unwrap();
        assert_eq!(handler.region, "us-west-2");
        assert_eq!(handler.actual_model_id, "test-model-id");
    }

    #[test]
    fn test_is_bedrock() {
        let handler1 = CustomModelHandler {
            region: "us-east-1".to_string(),
            actual_model_id: "anthropic.claude-3-5-sonnet".to_string(),
        };
        assert!(handler1.is_bedrock());

        let handler2 = CustomModelHandler {
            region: "us-east-1".to_string(),
            actual_model_id: "claude-4-sonnet".to_string(),
        };
        assert!(handler2.is_bedrock());

        let handler3 = CustomModelHandler {
            region: "us-east-1".to_string(),
            actual_model_id: "other-model".to_string(),
        };
        assert!(!handler3.is_bedrock());
    }

    #[test]
    fn test_get_model_id() {
        let handler = CustomModelHandler {
            region: "eu-west-1".to_string(),
            actual_model_id: "CLAUDE_SONNET_4_20250514_V1_0".to_string(),
        };
        assert_eq!(handler.get_model_id(), "CLAUDE_SONNET_4_20250514_V1_0");
    }

    #[test]
    fn test_parse_custom_model() {
        // Valid format
        let result = parse_custom_model("custom:us-east-1:model-id");
        assert!(result.is_some());
        let (region, model) = result.unwrap();
        assert_eq!(region, "us-east-1");
        assert_eq!(model, "model-id");

        // Invalid formats
        assert!(parse_custom_model("invalid:format").is_none());
        assert!(parse_custom_model("custom:").is_none());
        assert!(parse_custom_model("custom:us-east-1").is_none());
        assert!(parse_custom_model("").is_none());
    }

    #[test]
    fn test_complex_model_ids() {
        let result = parse_custom_model("custom:us-east-1:vendor:model:version:0");
        assert!(result.is_some());
        let (region, model) = result.unwrap();
        assert_eq!(region, "us-east-1");
        assert_eq!(model, "vendor:model:version:0");
    }

    #[test]
    fn test_debug_trait() {
        let handler = CustomModelHandler {
            region: "ap-southeast-1".to_string(),
            actual_model_id: "TEST_MODEL".to_string(),
        };
        let debug_str = format!("{:?}", handler);
        assert!(debug_str.contains("CustomModelHandler"));
        assert!(debug_str.contains("ap-southeast-1"));
        assert!(debug_str.contains("TEST_MODEL"));
    }
}