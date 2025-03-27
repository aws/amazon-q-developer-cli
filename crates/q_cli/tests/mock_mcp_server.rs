use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Response for the /tools/list endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolListResponse {
    pub tools: Vec<ToolSpecification>,
}

/// Tool specification as returned by the mock server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpecification {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Request for the /tools/call endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub tool_name: String,
    pub parameters: Value,
}

/// Response for the /tools/call endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    pub status: String,
    pub result: Value,
}

/// Configuration for the mock server
#[derive(Debug, Clone)]
pub struct MockServerConfig {
    pub tools: Vec<ToolSpecification>,
    pub responses: Arc<RwLock<Vec<ToolCallResponse>>>,
    pub error_simulation: Arc<RwLock<bool>>,
}

impl Default for MockServerConfig {
    fn default() -> Self {
        Self {
            tools: vec![ToolSpecification {
                name: "test_tool".to_string(),
                description: "A test tool".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to echo"
                        }
                    }
                }),
            }],
            responses: Arc::new(RwLock::new(vec![
                ToolCallResponse {
                    status: "success".to_string(),
                    result: json!({ "message": "Hello from mock server!" }),
                },
            ])),
            error_simulation: Arc::new(RwLock::new(false)),
        }
    }
}

/// Mock MCP server for testing
pub struct MockMcpServer {
    address: SocketAddr,
    config: MockServerConfig,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MockMcpServer {
    /// Create a new mock server with the given configuration
    pub async fn new(config: MockServerConfig) -> Result<Self> {
        // Bind to a random port
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("Failed to bind to port")?;
        let address = listener.local_addr().context("Failed to get local address")?;

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let config_clone1 = config.clone();
        let config_clone2 = config.clone();
        
        let app = Router::new()
            .route("/tools/list", get(move || handle_tool_list(config_clone1.clone())))
            .route(
                "/tools/call",
                post(move |payload| handle_tool_call(config_clone2.clone(), payload)),
            );

        // Spawn the server task
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await
                .ok();
        });

        info!("Mock MCP server started on {}", address);
        Ok(Self {
            address,
            config,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Get the server's URL
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Shut down the server
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            tx.send(()).ok();
            debug!("Mock MCP server shutdown signal sent");
        }
    }

    /// Add a response to be returned by the server
    pub async fn add_response(&self, response: ToolCallResponse) {
        self.config.responses.write().await.push(response);
    }

    /// Enable or disable error simulation
    pub async fn set_error_simulation(&mut self, enabled: bool) {
        let mut error_simulation = self.config.error_simulation.write().await;
        *error_simulation = enabled;
    }
}

/// Handler for /tools/list endpoint
async fn handle_tool_list(config: MockServerConfig) -> Json<ToolListResponse> {
    Json(ToolListResponse {
        tools: config.tools,
    })
}

/// Handler for /tools/call endpoint
async fn handle_tool_call(
    config: MockServerConfig,
    Json(request): Json<ToolCallRequest>,
) -> Json<ToolCallResponse> {
    if *config.error_simulation.read().await {
        return Json(ToolCallResponse {
            status: "error".to_string(),
            result: json!({
                "error": "Simulated error",
                "details": "Error simulation is enabled"
            }),
        });
    }

    let responses = config.responses.read().await;
    if let Some(response) = responses.first() {
        Json(response.clone())
    } else {
        Json(ToolCallResponse {
            status: "error".to_string(),
            result: json!({
                "error": "No response configured",
                "request": request
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;

    #[tokio::test]
    async fn test_mock_server_initialization() {
        let server = MockMcpServer::new(MockServerConfig::default())
            .await
            .expect("Failed to create mock server");
        
        assert!(!server.url().is_empty());
        server.shutdown().await;
    }

    #[tokio::test]
    async fn test_tool_list_endpoint() {
        let server = MockMcpServer::new(MockServerConfig::default())
            .await
            .expect("Failed to create mock server");
        
        let client = Client::new();
        let response = client
            .get(&format!("{}/tools/list", server.url()))
            .send()
            .await
            .expect("Failed to send request");
        
        assert!(response.status().is_success());
        let tools: ToolListResponse = response.json().await.expect("Failed to parse response");
        assert!(!tools.tools.is_empty());
        
        server.shutdown().await;
    }

    #[tokio::test]
    async fn test_tool_call_endpoint() {
        let server = MockMcpServer::new(MockServerConfig::default())
            .await
            .expect("Failed to create mock server");
        
        let client = Client::new();
        let request = ToolCallRequest {
            tool_name: "test_tool".to_string(),
            parameters: json!({"message": "test"}),
        };
        
        let response = client
            .post(&format!("{}/tools/call", server.url()))
            .json(&request)
            .send()
            .await
            .expect("Failed to send request");
        
        assert!(response.status().is_success());
        let result: ToolCallResponse = response.json().await.expect("Failed to parse response");
        assert_eq!(result.status, "success");
        
        server.shutdown().await;
    }

    #[tokio::test]
    async fn test_error_simulation() {
        let mut server = MockMcpServer::new(MockServerConfig::default())
            .await
            .expect("Failed to create mock server");
        
        // Enable error simulation
        server.set_error_simulation(true).await;
        
        let client = Client::new();
        let request = ToolCallRequest {
            tool_name: "test_tool".to_string(),
            parameters: json!({"message": "test"}),
        };
        
        let response = client
            .post(&format!("{}/tools/call", server.url()))
            .json(&request)
            .send()
            .await
            .expect("Failed to send request");
        
        assert!(response.status().is_success());
        let result: ToolCallResponse = response.json().await.expect("Failed to parse response");
        assert_eq!(result.status, "error");
        
        server.shutdown().await;
    }
}
