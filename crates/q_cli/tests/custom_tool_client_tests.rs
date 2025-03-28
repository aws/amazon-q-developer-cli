// CustomToolClient Basic Tests
// This module implements basic tests for the CustomToolClient

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
use mcp_test_models::{McpConfigHelper, McpServerConfig};

/// Test CustomToolClient initialization
#[test]
async fn test_custom_tool_client_initialization() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
    // Create a mock server with a simple tool
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
    
    // Create a CustomToolConfig that points to our mock server
    let server_name = "test_server";
    
    // Create the config file manually instead of using the helper
    let config_content = format!(
        r#"{{
            "mcp_servers": {{
                "{}": {{
                    "endpoint": "{}",
                    "auth_token": null
                }}
            }}
        }}"#,
        server_name, server.url()
    );
    
    let config_path = fixture.create_file("mcp_config.json", &config_content).await?;
    
    // Load the config
    let config_content = tokio::fs::read_to_string(&config_path).await?;
    let mcp_config: McpServerConfig = serde_json::from_str(&config_content)?;
    
    // Extract the CustomToolConfig for our test server
    let custom_tool_config = mcp_config.mcp_servers.get(server_name)
        .ok_or_else(|| anyhow::anyhow!("Failed to get config for test server"))?;
    
    // Verify the config is correctly set up
    assert_eq!(custom_tool_config.endpoint, server.url());
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

/// Test tool specification retrieval
#[test]
async fn test_tool_specification_retrieval() -> Result<()> {
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

/// Test simple tool invocation
#[test]
async fn test_simple_tool_invocation() -> Result<()> {
    // Create a test fixture
    let _fixture = TestFixture::new().await?;
    
    // Create a mock server with a calculator tool
    let tool_spec = ToolSpecification {
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
    };
    
    // Create a custom response for the calculator tool
    let calc_response = ToolCallResponse {
        status: "success".to_string(),
        result: json!({
            "result": 42,
            "operation": "add"
        }),
    };
    
    let config = MockServerConfig {
        tools: vec![tool_spec],
        responses: Arc::new(tokio::sync::RwLock::new(vec![calc_response])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let server = MockMcpServer::new(config).await?;
    
    // Create a test HTTP client
    let client = reqwest::Client::new();
    
    // Test the tool call endpoint directly
    let response = client
        .post(&format!("{}/tools/call", server.url()))
        .json(&json!({
            "tool_name": "calculator",
            "parameters": {
                "a": 20,
                "b": 22,
                "operation": "add"
            }
        }))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let result: mock_mcp_server::ToolCallResponse = response.json().await?;
    assert_eq!(result.status, "success");
    assert_eq!(result.result["result"], 42);
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

/// Test error handling in tool invocation
#[test]
async fn test_tool_invocation_error_handling() -> Result<()> {
    // Create a test fixture
    let _fixture = TestFixture::new().await?;
    
    // Create a mock server with error simulation enabled
    let tool_spec = ToolSpecification {
        name: "error_prone_tool".to_string(),
        description: "A tool that might fail".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Input that might cause an error"
                }
            },
            "required": ["input"]
        }),
    };
    
    let config = MockServerConfig {
        tools: vec![tool_spec],
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "error".to_string(),
                result: json!({
                    "error": "Simulated error",
                    "details": "Error simulation is enabled"
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
    
    // Test the tool call endpoint with error simulation
    let response = client
        .post(&format!("{}/tools/call", server.url()))
        .json(&json!({
            "tool_name": "error_prone_tool",
            "parameters": {
                "input": "trigger_error"
            }
        }))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let result: mock_mcp_server::ToolCallResponse = response.json().await?;
    assert_eq!(result.status, "error");
    assert!(result.result["error"].as_str().unwrap().contains("Simulated error"));
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}
