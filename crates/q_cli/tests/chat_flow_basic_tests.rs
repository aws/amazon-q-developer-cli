// Chat Flow Basic Integration Tests
// This module implements basic integration tests for the chat flow with tool usage

#[path = "mock_mcp_server.rs"]
mod mock_mcp_server;
#[path = "mcp_test_infrastructure.rs"]
mod mcp_test_infrastructure;
#[path = "mcp_test_models.rs"]
mod mcp_test_models;

use std::io::Cursor;
use std::sync::Arc;

use anyhow::Result;
use fig_api_client::model::{ChatResponseStream, AssistantResponseMessage};
use fig_api_client::StreamingClient;
use fig_os_shim::Context;
use serde_json::json;
use tokio::test;

use mock_mcp_server::{MockMcpServer, MockServerConfig, ToolCallResponse, ToolSpecification};
use mcp_test_infrastructure::TestFixture;
use mcp_test_models::McpServerConfig;

/// Test chat initialization with ToolManager
#[test]
async fn test_chat_initialization() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
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
    
    // Create a fake context for testing
    let ctx = Context::builder().with_test_home().await?.build_fake();
    
    // Create a mock StreamingClient that returns a predefined response
    let mock_client = create_test_client();
    
    // Create a buffer to capture output
    let mut output = Cursor::new(Vec::new());
    
    // Create a ChatContext with our test configuration
    let mut chat_context = crate::cli::chat::ChatContext::new(
        Arc::clone(&ctx),
        fig_settings::Settings::new_fake(),
        &mut output,
        Some("Hello".to_string()), // Initial input
        crate::cli::chat::input_source::InputSource::new_mock(vec![
            "/quit".to_string(), // Exit after first response
        ]),
        true, // Interactive mode
        mock_client,
        || Some(80), // Terminal width
        Some(mcp_config),
        true, // Accept all tools
        None, // No specific profile
    )
    .await?;
    
    // Run the chat
    chat_context.try_chat().await?;
    
    // Get the output as a string
    let output_str = String::from_utf8(output.into_inner())?;
    
    // Verify the output contains expected content
    assert!(output_str.contains("Hello from mock server!"));
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

/// Test simple tool invocation within chat context
#[test]
async fn test_simple_tool_invocation() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
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
    
    // Create a fake context for testing
    let ctx = Context::builder().with_test_home().await?.build_fake();
    
    // Create a mock StreamingClient that returns a tool use
    let mock_client = create_test_client_with_tool_use();
    
    // Create a buffer to capture output
    let mut output = Cursor::new(Vec::new());
    
    // Create a ChatContext with our test configuration
    let mut chat_context = crate::cli::chat::ChatContext::new(
        Arc::clone(&ctx),
        fig_settings::Settings::new_fake(),
        &mut output,
        Some("Use the echo tool".to_string()), // Initial input
        crate::cli::chat::input_source::InputSource::new_mock(vec![
            "y".to_string(), // Accept tool use
            "/quit".to_string(), // Exit after tool execution
        ]),
        true, // Interactive mode
        mock_client,
        || Some(80), // Terminal width
        Some(mcp_config),
        false, // Don't accept all tools automatically
        None, // No specific profile
    )
    .await?;
    
    // Run the chat
    chat_context.try_chat().await?;
    
    // Get the output as a string
    let output_str = String::from_utf8(output.into_inner())?;
    
    // Verify the output contains expected content
    assert!(output_str.contains("I'll use the echo tool"));
    assert!(output_str.contains("test_server___echo"));
    assert!(output_str.contains("Hello from mock server!"));
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

/// Test response handling and formatting
#[test]
async fn test_response_handling() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
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
    
    // Create a fake context for testing
    let ctx = Context::builder().with_test_home().await?.build_fake();
    
    // Create a mock StreamingClient that returns formatted text
    let mock_client = create_test_client_with_formatting();
    
    // Create a buffer to capture output
    let mut output = Cursor::new(Vec::new());
    
    // Create a ChatContext with our test configuration
    let mut chat_context = crate::cli::chat::ChatContext::new(
        Arc::clone(&ctx),
        fig_settings::Settings::new_fake(),
        &mut output,
        Some("Show me some formatted text".to_string()), // Initial input
        crate::cli::chat::input_source::InputSource::new_mock(vec![
            "/quit".to_string(), // Exit after response
        ]),
        true, // Interactive mode
        mock_client,
        || Some(80), // Terminal width
        Some(mcp_config),
        true, // Accept all tools
        None, // No specific profile
    )
    .await?;
    
    // Run the chat
    chat_context.try_chat().await?;
    
    // Get the output as a string
    let output_str = String::from_utf8(output.into_inner())?;
    
    // Verify the output contains expected content
    assert!(output_str.contains("Here's some formatted text"));
    assert!(output_str.contains("Code block"));
    assert!(output_str.contains("Bold text"));
    assert!(output_str.contains("Italic text"));
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

/// Test error handling in chat flow
#[test]
async fn test_error_handling() -> Result<()> {
    // Create a test fixture
    let fixture = TestFixture::new().await?;
    
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
                    "error": "Simulated error",
                    "details": "This is a simulated error for testing"
                }),
            },
        ])),
        error_simulation: Arc::new(tokio::sync::RwLock::new(true)),
    };
    
    let mut server = MockMcpServer::new(config).await?;
    server.set_error_simulation(true).await;
    
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
    
    // Create a fake context for testing
    let ctx = Context::builder().with_test_home().await?.build_fake();
    
    // Create a mock StreamingClient that returns a tool use that will fail
    let mock_client = create_test_client_with_failing_tool();
    
    // Create a buffer to capture output
    let mut output = Cursor::new(Vec::new());
    
    // Create a ChatContext with our test configuration
    let mut chat_context = crate::cli::chat::ChatContext::new(
        Arc::clone(&ctx),
        fig_settings::Settings::new_fake(),
        &mut output,
        Some("Use the echo tool".to_string()), // Initial input
        crate::cli::chat::input_source::InputSource::new_mock(vec![
            "y".to_string(), // Accept tool use
            "/quit".to_string(), // Exit after error
        ]),
        true, // Interactive mode
        mock_client,
        || Some(80), // Terminal width
        Some(mcp_config),
        false, // Don't accept all tools automatically
        None, // No specific profile
    )
    .await?;
    
    // Run the chat
    chat_context.try_chat().await?;
    
    // Get the output as a string
    let output_str = String::from_utf8(output.into_inner())?;
    
    // Verify the output contains expected error content
    assert!(output_str.contains("I'll use the echo tool"));
    assert!(output_str.contains("test_server___echo"));
    assert!(output_str.contains("Execution failed"));
    assert!(output_str.contains("Simulated error"));
    
    // Clean up
    server.shutdown().await;
    
    Ok(())
}

// Helper function to create a test StreamingClient
fn create_test_client() -> StreamingClient {
    let response = vec![
        ChatResponseStream::AssistantResponseEvent {
            content: "Hello from mock server!".to_string(),
        },
        ChatResponseStream::EndOfStream {
            message: Box::new(AssistantResponseMessage {
                message_id: Some("test_message_id".to_string()),
                content: "Hello from mock server!".to_string(),
                tool_uses: None,
            }),
        },
    ];
    
    StreamingClient::mock(vec![response])
}

// Helper function to create a test StreamingClient with tool use
fn create_test_client_with_tool_use() -> StreamingClient {
    let response1 = vec![
        ChatResponseStream::AssistantResponseEvent {
            content: "I'll use the echo tool".to_string(),
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: "tool1".to_string(),
            name: "test_server___echo".to_string(),
            input: Some(r#"{"message":"Hello world"}"#.to_string()),
            stop: None,
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: "tool1".to_string(),
            name: "test_server___echo".to_string(),
            input: None,
            stop: Some(true),
        },
        ChatResponseStream::EndOfStream {
            message: Box::new(AssistantResponseMessage {
                message_id: Some("test_message_id".to_string()),
                content: "I'll use the echo tool".to_string(),
                tool_uses: Some(vec![
                    crate::cli::chat::parser::ToolUse {
                        id: "tool1".to_string(),
                        name: "test_server___echo".to_string(),
                        args: json!({"message":"Hello world"}),
                    },
                ]),
            }),
        },
    ];
    
    let response2 = vec![
        ChatResponseStream::AssistantResponseEvent {
            content: "The tool returned: Hello from mock server!".to_string(),
        },
        ChatResponseStream::EndOfStream {
            message: Box::new(AssistantResponseMessage {
                message_id: Some("test_message_id2".to_string()),
                content: "The tool returned: Hello from mock server!".to_string(),
                tool_uses: None,
            }),
        },
    ];
    
    StreamingClient::mock(vec![response1, response2])
}

// Helper function to create a test StreamingClient with formatted text
fn create_test_client_with_formatting() -> StreamingClient {
    let response = vec![
        ChatResponseStream::AssistantResponseEvent {
            content: "Here's some formatted text:\n\n".to_string(),
        },
        ChatResponseStream::AssistantResponseEvent {
            content: "```rust\nfn main() {\n    println!(\"Code block\");\n}\n```\n\n".to_string(),
        },
        ChatResponseStream::AssistantResponseEvent {
            content: "**Bold text** and *Italic text*".to_string(),
        },
        ChatResponseStream::EndOfStream {
            message: Box::new(AssistantResponseMessage {
                message_id: Some("test_message_id".to_string()),
                content: "Here's some formatted text:\n\n```rust\nfn main() {\n    println!(\"Code block\");\n}\n```\n\n**Bold text** and *Italic text*".to_string(),
                tool_uses: None,
            }),
        },
    ];
    
    StreamingClient::mock(vec![response])
}

// Helper function to create a test StreamingClient with failing tool
fn create_test_client_with_failing_tool() -> StreamingClient {
    let response1 = vec![
        ChatResponseStream::AssistantResponseEvent {
            content: "I'll use the echo tool".to_string(),
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: "tool1".to_string(),
            name: "test_server___echo".to_string(),
            input: Some(r#"{"message":"Hello world"}"#.to_string()),
            stop: None,
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: "tool1".to_string(),
            name: "test_server___echo".to_string(),
            input: None,
            stop: Some(true),
        },
        ChatResponseStream::EndOfStream {
            message: Box::new(AssistantResponseMessage {
                message_id: Some("test_message_id".to_string()),
                content: "I'll use the echo tool".to_string(),
                tool_uses: Some(vec![
                    crate::cli::chat::parser::ToolUse {
                        id: "tool1".to_string(),
                        name: "test_server___echo".to_string(),
                        args: json!({"message":"Hello world"}),
                    },
                ]),
            }),
        },
    ];
    
    let response2 = vec![
        ChatResponseStream::AssistantResponseEvent {
            content: "I encountered an error: Simulated error".to_string(),
        },
        ChatResponseStream::EndOfStream {
            message: Box::new(AssistantResponseMessage {
                message_id: Some("test_message_id2".to_string()),
                content: "I encountered an error: Simulated error".to_string(),
                tool_uses: None,
            }),
        },
    ];
    
    StreamingClient::mock(vec![response1, response2])
}
