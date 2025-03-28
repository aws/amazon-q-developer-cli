// Tool Manager Basic Tests
// This module implements basic tests for the ToolManager

#[path = "mock_mcp_server.rs"]
mod mock_mcp_server;
#[path = "mcp_test_infrastructure.rs"]
mod mcp_test_infrastructure;
#[path = "mcp_test_models.rs"]
mod mcp_test_models;

use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use tokio::test;

use mock_mcp_server::{MockMcpServer, MockServerConfig, ToolCallResponse, ToolSpecification};
use mcp_test_infrastructure::TestFixture;
use mcp_test_models::McpServerConfig;

/// Test configuration loading
#[test]
async fn test_config_loading() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
    // Create a mock server
    let tool_spec = ToolSpecification {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Message to echo"
                }
            },
            "required": ["message"]
        }),
    };
    
    let config = MockServerConfig {
        tools: vec![tool_spec],
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "message": "Hello from mock server!" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let server = MockMcpServer::new(config).await?;
    
    // Create a configuration file for the ToolManager
    let config_content = format!(
        r#"{{
            "mcp_servers": {{
                "test_server": {{
                    "endpoint": "{}",
                    "authToken": null
                }}
            }}
        }}"#,
        server.url()
    );
    
    let config_path = fixture.create_file("mcp_config.json", &config_content).await?;
    
    // Set the environment variable to point to our config file
    std::env::set_var("FIG_SETTINGS_MCP_CONFIG", config_path.to_str().unwrap());
    
    // Load the configuration
    let config_content = tokio::fs::read_to_string(&config_path).await?;
    let mcp_config: McpServerConfig = serde_json::from_str(&config_content)?;
    
    // Verify the configuration was loaded correctly
    assert_eq!(mcp_config.mcp_servers.len(), 1);
    assert!(mcp_config.mcp_servers.contains_key("test_server"));
    assert_eq!(mcp_config.mcp_servers["test_server"].endpoint, server.url());
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

/// Test tool discovery with mock server
#[test]
async fn test_tool_discovery_with_mock() -> Result<()> {
    // Create a test fixture
    let _fixture = TestFixture::new().await?;
    
    // Create a mock server with multiple tools
    let tool_specs = vec![
        ToolSpecification {
            name: "echo".to_string(),
            description: "Echo a message".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Message to echo"
                    }
                },
                "required": ["message"]
            }),
        },
        ToolSpecification {
            name: "calculator".to_string(),
            description: "Perform calculations".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "a": {
                        "type": "number",
                        "description": "First operand"
                    },
                    "b": {
                        "type": "number",
                        "description": "Second operand"
                    },
                    "operation": {
                        "type": "string",
                        "description": "Operation to perform"
                    }
                },
                "required": ["a", "b", "operation"]
            }),
        },
    ];
    
    let config = MockServerConfig {
        tools: tool_specs,
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "message": "Hello from mock server!" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let server = MockMcpServer::new(config).await?;
    
    // Create a test HTTP client
    let client = reqwest::Client::new();
    
    // Test the tool list endpoint directly
    let response = client
        .get(&format!("{}/tools/list", server.url()))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let tools_response: mock_mcp_server::ToolListResponse = response.json().await?;
    assert_eq!(tools_response.tools.len(), 2);
    assert_eq!(tools_response.tools[0].name, "echo");
    assert_eq!(tools_response.tools[1].name, "calculator");
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

/// Test tool selection with mock server
#[test]
async fn test_tool_selection_with_mock() -> Result<()> {
    // Create a test fixture
    let _fixture = TestFixture::new().await?;
    
    // Create a mock server with a tool
    let tool_spec = ToolSpecification {
        name: "echo".to_string(),
        description: "Echo a message".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Message to echo"
                }
            },
            "required": ["message"]
        }),
    };
    
    let config = MockServerConfig {
        tools: vec![tool_spec],
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "message": "Hello, world!" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let server = MockMcpServer::new(config).await?;
    
    // Create a test HTTP client
    let client = reqwest::Client::new();
    
    // Test the tool call endpoint directly
    let response = client
        .post(&format!("{}/tools/call", server.url()))
        .json(&json!({
            "tool_name": "echo",
            "parameters": {
                "message": "Hello, world!"
            }
        }))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let result: mock_mcp_server::ToolCallResponse = response.json().await?;
    assert_eq!(result.status, "success");
    assert_eq!(result.result["message"], "Hello, world!");
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

/// Test error handling for invalid tool names
#[test]
async fn test_error_handling_with_mock() -> Result<()> {
    // Create a test fixture
    let _fixture = TestFixture::new().await?;
    
    // Create a mock server with error simulation enabled
    let tool_spec = ToolSpecification {
        name: "echo".to_string(),
        description: "Echo a message".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Message to echo"
                }
            },
            "required": ["message"]
        }),
    };
    
    let config = MockServerConfig {
        tools: vec![tool_spec],
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "error".to_string(),
                result: json!({
                    "error": "Tool not found",
                    "details": "The requested tool 'unknown_tool' does not exist"
                }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(true)),
    };
    
    let mut server = MockMcpServer::new(config).await?;
    
    // Enable error simulation
    server.set_error_simulation(true).await;
    
    // Create a test HTTP client
    let client = reqwest::Client::new();
    
    // Test the tool call endpoint with an unknown tool
    let response = client
        .post(&format!("{}/tools/call", server.url()))
        .json(&json!({
            "tool_name": "unknown_tool",
            "parameters": {
                "message": "Hello, world!"
            }
        }))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let result: mock_mcp_server::ToolCallResponse = response.json().await?;
    assert_eq!(result.status, "error");
    assert!(result.result["error"].is_string());
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}
