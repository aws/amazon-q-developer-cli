// Test Configuration and Data Models for MCP Integration Tests
// This module provides the data models and configuration structures for MCP testing

#[path = "mock_mcp_server.rs"]
mod mock_mcp_server;
#[path = "mcp_test_infrastructure.rs"]
mod mcp_test_infrastructure;

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use mock_mcp_server::{ToolCallResponse, ToolSpecification};
use mcp_test_infrastructure::TestFixture;

/// MCP server configuration for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Map of server name to server configuration
    pub mcp_servers: HashMap<String, CustomToolConfig>,
}

/// Configuration for a custom tool server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomToolConfig {
    /// Server endpoint URL
    pub endpoint: String,
    /// Optional authentication token
    pub auth_token: Option<String>,
}

/// Test case definition for tool invocation tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTestCase {
    /// Name of the test case
    pub name: String,
    /// Description of what the test is verifying
    pub description: String,
    /// Name of the tool to invoke
    pub tool_name: String,
    /// Parameters to pass to the tool
    pub parameters: Value,
    /// Expected result status (success or error)
    pub expected_status: String,
    /// Expected result content (partial match)
    pub expected_content: Value,
    /// Whether to simulate an error
    pub simulate_error: bool,
}

impl Default for ToolTestCase {
    fn default() -> Self {
        Self {
            name: "default_test".to_string(),
            description: "Default test case".to_string(),
            tool_name: "test_tool".to_string(),
            parameters: json!({"message": "test"}),
            expected_status: "success".to_string(),
            expected_content: json!({"message": "Hello from mock server!"}),
            simulate_error: false,
        }
    }
}

/// Collection of test cases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseCollection {
    /// Test cases in the collection
    pub test_cases: Vec<ToolTestCase>,
}

/// Tool specification builder for creating test tool specifications
#[derive(Debug, Clone)]
pub struct ToolSpecificationBuilder {
    name: String,
    description: String,
    parameters: Value,
}

impl ToolSpecificationBuilder {
    /// Create a new tool specification builder with the given name
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Test tool: {}", name),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    /// Set the description for the tool
    pub fn description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    /// Add a string parameter to the tool
    pub fn add_string_param(mut self, name: &str, description: &str, required: bool) -> Self {
        let mut parameters = self.parameters.as_object().unwrap().clone();
        
        // Add to properties
        let properties = parameters["properties"].as_object().unwrap().clone();
        let mut properties = serde_json::Map::from(properties);
        properties.insert(
            name.to_string(),
            json!({
                "type": "string",
                "description": description
            }),
        );
        parameters.insert("properties".to_string(), json!(properties));
        
        // Add to required if needed
        if required {
            let required = parameters.get("required")
                .and_then(|v| v.as_array())
                .map(|arr| arr.clone())
                .unwrap_or_else(Vec::new);
            
            let mut required = required;
            required.push(json!(name));
            parameters.insert("required".to_string(), json!(required));
        }
        
        self.parameters = json!(parameters);
        self
    }

    /// Add a number parameter to the tool
    pub fn add_number_param(mut self, name: &str, description: &str, required: bool) -> Self {
        let mut parameters = self.parameters.as_object().unwrap().clone();
        
        // Add to properties
        let properties = parameters["properties"].as_object().unwrap().clone();
        let mut properties = serde_json::Map::from(properties);
        properties.insert(
            name.to_string(),
            json!({
                "type": "number",
                "description": description
            }),
        );
        parameters.insert("properties".to_string(), json!(properties));
        
        // Add to required if needed
        if required {
            let required = parameters.get("required")
                .and_then(|v| v.as_array())
                .map(|arr| arr.clone())
                .unwrap_or_else(Vec::new);
            
            let mut required = required;
            required.push(json!(name));
            parameters.insert("required".to_string(), json!(required));
        }
        
        self.parameters = json!(parameters);
        self
    }

    /// Build the tool specification
    pub fn build(self) -> ToolSpecification {
        ToolSpecification {
            name: self.name,
            description: self.description,
            parameters: self.parameters,
        }
    }
}

/// Response validator for verifying tool responses
pub struct ResponseValidator;

impl ResponseValidator {
    /// Validate that the response matches the expected status and content
    pub fn validate_response(
        response: &ToolCallResponse,
        expected_status: &str,
        expected_content: &Value,
    ) -> Result<()> {
        // Check status
        if response.status != expected_status {
            return Err(anyhow::anyhow!(
                "Status mismatch: expected '{}', got '{}'",
                expected_status,
                response.status
            ));
        }

        // Check content (partial match)
        Self::validate_json_subset(&response.result, expected_content)
            .context("Response content validation failed")
    }

    /// Validate that the subset JSON is contained within the superset JSON
    fn validate_json_subset(superset: &Value, subset: &Value) -> Result<()> {
        match (subset, superset) {
            // If subset is an object, check that all its key-value pairs are in the superset
            (Value::Object(subset_obj), Value::Object(superset_obj)) => {
                for (key, subset_value) in subset_obj {
                    match superset_obj.get(key) {
                        Some(superset_value) => {
                            Self::validate_json_subset(superset_value, subset_value)?;
                        }
                        None => {
                            return Err(anyhow::anyhow!("Key '{}' not found in response", key));
                        }
                    }
                }
                Ok(())
            }
            
            // If subset is an array, check that all its elements are in the superset
            (Value::Array(subset_arr), Value::Array(superset_arr)) => {
                if subset_arr.len() > superset_arr.len() {
                    return Err(anyhow::anyhow!(
                        "Array length mismatch: expected at least {} elements, got {}",
                        subset_arr.len(),
                        superset_arr.len()
                    ));
                }
                
                // This is a simplified check that just ensures the arrays have compatible lengths
                // A more sophisticated check would verify that each element in subset has a matching element in superset
                Ok(())
            }
            
            // For primitive values, check for equality
            _ => {
                if subset == superset {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "Value mismatch: expected '{}', got '{}'",
                        subset,
                        superset
                    ))
                }
            }
        }
    }
}

/// Helper to create MCP configuration files for testing
pub struct McpConfigHelper;

impl McpConfigHelper {
    /// Create an MCP configuration file with a single server
    pub async fn create_single_server_config(
        test_fixture: &TestFixture,
        server_name: &str,
        server_url: &str,
    ) -> Result<PathBuf> {
        let config = McpServerConfig {
            mcp_servers: HashMap::from([(
                server_name.to_string(),
                CustomToolConfig {
                    endpoint: server_url.to_string(),
                    auth_token: None,
                },
            )]),
        };
        
        test_fixture
            .create_json_file("mcp_config.json", &config)
            .await
            .context("Failed to create MCP config file")
    }

    /// Create an MCP configuration file with multiple servers
    pub async fn create_multi_server_config(
        test_fixture: &TestFixture,
        servers: &[(&str, &str)],
    ) -> Result<PathBuf> {
        let mut mcp_servers = HashMap::new();
        
        for (name, url) in servers {
            mcp_servers.insert(
                name.to_string(),
                CustomToolConfig {
                    endpoint: url.to_string(),
                    auth_token: None,
                },
            );
        }
        
        let config = McpServerConfig { mcp_servers };
        
        test_fixture
            .create_json_file("mcp_config.json", &config)
            .await
            .context("Failed to create MCP config file")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_specification_builder() {
        let tool_spec = ToolSpecificationBuilder::new("calculator")
            .description("A simple calculator tool")
            .add_number_param("a", "First operand", true)
            .add_number_param("b", "Second operand", true)
            .add_string_param("operation", "Operation to perform", true)
            .build();
        
        assert_eq!(tool_spec.name, "calculator");
        assert_eq!(tool_spec.description, "A simple calculator tool");
        
        let params = tool_spec.parameters;
        let properties = params["properties"].as_object().unwrap();
        
        assert!(properties.contains_key("a"));
        assert!(properties.contains_key("b"));
        assert!(properties.contains_key("operation"));
        
        let required = params["required"].as_array().unwrap();
        assert_eq!(required.len(), 3);
    }

    #[tokio::test]
    async fn test_response_validator() {
        // Success case
        let response = ToolCallResponse {
            status: "success".to_string(),
            result: json!({
                "message": "Hello, world!",
                "code": 200,
                "data": {
                    "value": 42
                }
            }),
        };
        
        let expected_content = json!({
            "message": "Hello, world!",
            "data": {
                "value": 42
            }
        });
        
        let result = ResponseValidator::validate_response(&response, "success", &expected_content);
        assert!(result.is_ok());
        
        // Failure case - status mismatch
        let result = ResponseValidator::validate_response(&response, "error", &expected_content);
        assert!(result.is_err());
        
        // Failure case - content mismatch
        let expected_content = json!({
            "message": "Wrong message",
            "data": {
                "value": 42
            }
        });
        
        let result = ResponseValidator::validate_response(&response, "success", &expected_content);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mcp_config_helper() -> Result<()> {
        let fixture = TestFixture::new().await?;
        
        // Test single server config
        let config_path = McpConfigHelper::create_single_server_config(
            &fixture,
            "test_server",
            "http://localhost:8080",
        )
        .await?;
        
        assert!(config_path.exists());
        
        let content = tokio::fs::read_to_string(config_path).await?;
        let config: McpServerConfig = serde_json::from_str(&content)?;
        
        assert_eq!(config.mcp_servers.len(), 1);
        assert!(config.mcp_servers.contains_key("test_server"));
        assert_eq!(
            config.mcp_servers["test_server"].endpoint,
            "http://localhost:8080"
        );
        
        // Test multi-server config
        let servers = vec![
            ("server1", "http://localhost:8081"),
            ("server2", "http://localhost:8082"),
        ];
        
        let config_path = McpConfigHelper::create_multi_server_config(&fixture, &servers).await?;
        
        assert!(config_path.exists());
        
        let content = tokio::fs::read_to_string(config_path).await?;
        let config: McpServerConfig = serde_json::from_str(&content)?;
        
        assert_eq!(config.mcp_servers.len(), 2);
        assert!(config.mcp_servers.contains_key("server1"));
        assert!(config.mcp_servers.contains_key("server2"));
        
        Ok(())
    }
}
