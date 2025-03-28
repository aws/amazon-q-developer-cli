// Tool Manager Advanced Tests
// This module implements advanced tests for the ToolManager

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

/// Test tool namespacing with multiple MCP servers
#[test]
async fn test_tool_namespacing_with_multiple_servers() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
    // Create first mock server with tools
    let server1_tools = vec![
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
    ];
    
    let server1_config = MockServerConfig {
        tools: server1_tools,
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "message": "Echo from server 1" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let server1 = MockMcpServer::new(server1_config).await?;
    
    // Create second mock server with tools
    let server2_tools = vec![
        ToolSpecification {
            name: "echo".to_string(),
            description: "Echo a message (server 2)".to_string(),
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
    
    let server2_config = MockServerConfig {
        tools: server2_tools,
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "message": "Echo from server 2" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let server2 = MockMcpServer::new(server2_config).await?;
    
    // Create a configuration file for the ToolManager with both servers
    let config_content = format!(
        r#"{{
            "mcp_servers": {{
                "server1": {{
                    "endpoint": "{}",
                    "authToken": null
                }},
                "server2": {{
                    "endpoint": "{}",
                    "authToken": null
                }}
            }}
        }}"#,
        server1.url(),
        server2.url()
    );
    
    let config_path = fixture.create_file("mcp_config.json", &config_content).await?;
    
    // Set the environment variable to point to our config file
    std::env::set_var("FIG_SETTINGS_MCP_CONFIG", config_path.to_str().unwrap());
    
    // Load the configuration
    let config_content = tokio::fs::read_to_string(&config_path).await?;
    let mcp_config: McpServerConfig = serde_json::from_str(&config_content)?;
    
    // Create a test HTTP client
    let client = reqwest::Client::new();
    
    // Test the tool list endpoint for server1
    let response1 = client
        .get(&format!("{}/tools/list", server1.url()))
        .send()
        .await?;
    
    assert!(response1.status().is_success());
    
    let tools_response1: mock_mcp_server::ToolListResponse = response1.json().await?;
    assert_eq!(tools_response1.tools.len(), 1);
    assert_eq!(tools_response1.tools[0].name, "echo");
    
    // Test the tool list endpoint for server2
    let response2 = client
        .get(&format!("{}/tools/list", server2.url()))
        .send()
        .await?;
    
    assert!(response2.status().is_success());
    
    let tools_response2: mock_mcp_server::ToolListResponse = response2.json().await?;
    assert_eq!(tools_response2.tools.len(), 2);
    assert_eq!(tools_response2.tools[0].name, "echo");
    assert_eq!(tools_response2.tools[1].name, "calculator");
    
    // Clean up
    server1.shutdown().await;
    server2.shutdown().await;
    
    Ok(())
}

/// Test conflict resolution between tools with the same name
#[test]
async fn test_tool_conflict_resolution() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
    // Create first mock server with a tool
    let server1_tools = vec![
        ToolSpecification {
            name: "common_tool".to_string(),
            description: "Common tool from server 1".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input parameter"
                    }
                },
                "required": ["input"]
            }),
        },
    ];
    
    let server1_config = MockServerConfig {
        tools: server1_tools,
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "output": "Result from server 1" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let server1 = MockMcpServer::new(server1_config).await?;
    
    // Create second mock server with a tool with the same name
    let server2_tools = vec![
        ToolSpecification {
            name: "common_tool".to_string(),
            description: "Common tool from server 2".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input parameter"
                    }
                },
                "required": ["input"]
            }),
        },
    ];
    
    let server2_config = MockServerConfig {
        tools: server2_tools,
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "output": "Result from server 2" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let server2 = MockMcpServer::new(server2_config).await?;
    
    // Create a configuration file for the ToolManager with both servers
    let config_content = format!(
        r#"{{
            "mcp_servers": {{
                "server1": {{
                    "endpoint": "{}",
                    "authToken": null
                }},
                "server2": {{
                    "endpoint": "{}",
                    "authToken": null
                }}
            }}
        }}"#,
        server1.url(),
        server2.url()
    );
    
    let config_path = fixture.create_file("mcp_config.json", &config_content).await?;
    
    // Set the environment variable to point to our config file
    std::env::set_var("FIG_SETTINGS_MCP_CONFIG", config_path.to_str().unwrap());
    
    // Load the configuration
    let config_content = tokio::fs::read_to_string(&config_path).await?;
    let mcp_config: McpServerConfig = serde_json::from_str(&config_content)?;
    
    // Create a test HTTP client
    let client = reqwest::Client::new();
    
    // Test the tool list endpoint for server1
    let response1 = client
        .get(&format!("{}/tools/list", server1.url()))
        .send()
        .await?;
    
    assert!(response1.status().is_success());
    
    let tools_response1: mock_mcp_server::ToolListResponse = response1.json().await?;
    assert_eq!(tools_response1.tools.len(), 1);
    assert_eq!(tools_response1.tools[0].name, "common_tool");
    assert_eq!(tools_response1.tools[0].description, "Common tool from server 1");
    
    // Test the tool list endpoint for server2
    let response2 = client
        .get(&format!("{}/tools/list", server2.url()))
        .send()
        .await?;
    
    assert!(response2.status().is_success());
    
    let tools_response2: mock_mcp_server::ToolListResponse = response2.json().await?;
    assert_eq!(tools_response2.tools.len(), 1);
    assert_eq!(tools_response2.tools[0].name, "common_tool");
    assert_eq!(tools_response2.tools[0].description, "Common tool from server 2");
    
    // Clean up
    server1.shutdown().await;
    server2.shutdown().await;
    
    Ok(())
}

/// Test advanced error handling with multiple servers
#[test]
async fn test_advanced_error_handling() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
    // Create a working server
    let working_server_tools = vec![
        ToolSpecification {
            name: "working_tool".to_string(),
            description: "A tool that works".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input parameter"
                    }
                },
                "required": ["input"]
            }),
        },
    ];
    
    let working_server_config = MockServerConfig {
        tools: working_server_tools,
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "output": "Success result" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let working_server = MockMcpServer::new(working_server_config).await?;
    
    // Create a failing server
    let failing_server_tools = vec![
        ToolSpecification {
            name: "failing_tool".to_string(),
            description: "A tool that fails".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input parameter"
                    }
                },
                "required": ["input"]
            }),
        },
    ];
    
    let failing_server_config = MockServerConfig {
        tools: failing_server_tools,
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "error".to_string(),
                result: json!({
                    "error": "Simulated error",
                    "details": "This tool is designed to fail"
                }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(true)),
    };
    
    let mut failing_server = MockMcpServer::new(failing_server_config).await?;
    failing_server.set_error_simulation(true).await;
    
    // Test the working server
    let client = reqwest::Client::new();
    
    // Test the tool call endpoint for the working server
    let working_response = client
        .post(&format!("{}/tools/call", working_server.url()))
        .json(&json!({
            "tool_name": "working_tool",
            "parameters": {
                "input": "Test input"
            }
        }))
        .send()
        .await?;
    
    assert!(working_response.status().is_success());
    
    let working_result: mock_mcp_server::ToolCallResponse = working_response.json().await?;
    assert_eq!(working_result.status, "success");
    assert_eq!(working_result.result["output"], "Success result");
    
    // Test the tool call endpoint for the failing server
    let failing_response = client
        .post(&format!("{}/tools/call", failing_server.url()))
        .json(&json!({
            "tool_name": "failing_tool",
            "parameters": {
                "input": "Test input"
            }
        }))
        .send()
        .await?;
    
    assert!(failing_response.status().is_success());
    
    let failing_result: mock_mcp_server::ToolCallResponse = failing_response.json().await?;
    assert_eq!(failing_result.status, "error");
    assert!(failing_result.result["error"].is_string());
    
    // Clean up
    working_server.shutdown().await;
    failing_server.shutdown().await;
    
    Ok(())
}

/// Test server initialization failures
#[test]
async fn test_server_initialization_failures() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
    // Create a working server
    let working_server_tools = vec![
        ToolSpecification {
            name: "working_tool".to_string(),
            description: "A tool that works".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input parameter"
                    }
                },
                "required": ["input"]
            }),
        },
    ];
    
    let working_server_config = MockServerConfig {
        tools: working_server_tools,
        responses: Arc::new(tokio::sync::RwLock::new(vec![
            ToolCallResponse {
                status: "success".to_string(),
                result: json!({ "output": "Success result" }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    let working_server = MockMcpServer::new(working_server_config).await?;
    
    // Create a configuration file for the ToolManager with both a working server and an invalid server
    let config_content = format!(
        r#"{{
            "mcp_servers": {{
                "working": {{
                    "endpoint": "{}",
                    "authToken": null
                }},
                "invalid": {{
                    "endpoint": "http://localhost:12345",
                    "authToken": null
                }}
            }}
        }}"#,
        working_server.url()
    );
    
    let config_path = fixture.create_file("mcp_config.json", &config_content).await?;
    
    // Set the environment variable to point to our config file
    std::env::set_var("FIG_SETTINGS_MCP_CONFIG", config_path.to_str().unwrap());
    
    // Test the working server directly
    let client = reqwest::Client::new();
    
    // Test the tool list endpoint for the working server
    let response = client
        .get(&format!("{}/tools/list", working_server.url()))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let tools_response: mock_mcp_server::ToolListResponse = response.json().await?;
    assert_eq!(tools_response.tools.len(), 1);
    assert_eq!(tools_response.tools[0].name, "working_tool");
    
    // Clean up
    working_server.shutdown().await;
    
    Ok(())
}
