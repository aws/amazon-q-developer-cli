use anyhow::Result;
use serde_json::json;
use tokio::test;

mod mcp_test_infrastructure;
mod mock_mcp_server;

use mcp_test_infrastructure::TestFixture;
use mock_mcp_server::{MockMcpServer, MockServerConfig, ToolCallResponse, ToolSpecification};

#[test]
async fn test_mock_server_with_test_fixture() -> Result<()> {
    // Create a test fixture from our infrastructure
    let fixture = TestFixture::new().await?;
    
    // Create a custom tool specification
    let tool_spec = ToolSpecification {
        name: "hello_world".to_string(),
        description: "A simple hello world tool".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name to greet"
                }
            },
            "required": ["name"]
        }),
    };
    
    // Create a custom response
    let response = ToolCallResponse {
        status: "success".to_string(),
        result: json!({
            "greeting": "Hello, Integration Test!"
        }),
    };
    
    // Create server config with our custom tool and response
    let config = MockServerConfig {
        tools: vec![tool_spec],
        responses: std::sync::Arc::new(tokio::sync::RwLock::new(vec![response])),
        error_simulation: std::sync::Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    // Create the mock server
    let server = MockMcpServer::new(config).await?;
    
    // Store the server URL in the test fixture's state for potential later use
    fixture.set_state("mcp_server_url", server.url()).await?;
    
    // Create a config file in the test fixture's temp directory
    let config_content = format!(
        r#"{{
            "mcp_servers": {{
                "test_server": {{
                    "endpoint": "{}"
                }}
            }}
        }}"#,
        server.url()
    );
    
    let config_path = fixture.create_file("mcp_config.json", &config_content).await?;
    
    // Verify the config file was created
    let content = mcp_test_infrastructure::read_file_to_string(&config_path).await?;
    assert!(content.contains(&server.url()));
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

#[test]
async fn test_mock_server_error_simulation() -> Result<()> {
    // Create a test fixture
    let _fixture = TestFixture::new().await?;
    
    // Create a server with default config
    let mut server = MockMcpServer::new(MockServerConfig::default()).await?;
    
    // Enable error simulation
    server.set_error_simulation(true).await;
    
    // Create a client to test the server
    let client = reqwest::Client::new();
    
    // Test the error simulation
    let response = client
        .post(&format!("{}/tools/call", server.url()))
        .json(&json!({
            "tool_name": "test_tool",
            "parameters": { "message": "test" }
        }))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let result = response.json::<mock_mcp_server::ToolCallResponse>().await?;
    assert_eq!(result.status, "error");
    assert!(result.result["error"].as_str().unwrap().contains("Simulated error"));
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

#[test]
async fn test_custom_tool_specification() -> Result<()> {
    // Create a test fixture
    let _fixture = TestFixture::new().await?;
    
    // Create a custom tool specification
    let custom_tool = ToolSpecification {
        name: "custom_calculator".to_string(),
        description: "A simple calculator tool".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["add", "subtract", "multiply", "divide"],
                    "description": "Operation to perform"
                },
                "a": {
                    "type": "number",
                    "description": "First operand"
                },
                "b": {
                    "type": "number",
                    "description": "Second operand"
                }
            },
            "required": ["operation", "a", "b"]
        }),
    };
    
    // Create a custom response
    let custom_response = ToolCallResponse {
        status: "success".to_string(),
        result: json!({
            "result": 42,
            "operation": "add"
        }),
    };
    
    // Create server config with our custom tool and response
    let config = MockServerConfig {
        tools: vec![custom_tool],
        responses: std::sync::Arc::new(tokio::sync::RwLock::new(vec![custom_response])),
        error_simulation: std::sync::Arc::new(tokio::sync::RwLock::new(false)),
    };
    
    // Create the mock server
    let server = MockMcpServer::new(config).await?;
    
    // Test the tool list endpoint
    let client = reqwest::Client::new();
    let response = client
        .get(&format!("{}/tools/list", server.url()))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let tools = response.json::<mock_mcp_server::ToolListResponse>().await?;
    assert_eq!(tools.tools.len(), 1);
    assert_eq!(tools.tools[0].name, "custom_calculator");
    
    // Test the tool call endpoint
    let response = client
        .post(&format!("{}/tools/call", server.url()))
        .json(&json!({
            "tool_name": "custom_calculator",
            "parameters": {
                "operation": "add",
                "a": 20,
                "b": 22
            }
        }))
        .send()
        .await?;
    
    assert!(response.status().is_success());
    
    let result = response.json::<mock_mcp_server::ToolCallResponse>().await?;
    assert_eq!(result.status, "success");
    assert_eq!(result.result["result"], 42);
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}
