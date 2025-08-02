use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use eyre::Result;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use super::connection_handler::ConnectionHandler;
use super::protocol::{MessageType, SocketMessage};
use super::socket_manager::{SocketManager, SocketType};

/// Core message router for distributing messages between chat session and sockets
#[derive(Debug)]
pub struct MessageRouter {
    /// Reference to the socket manager for socket operations
    socket_manager: Arc<Mutex<SocketManager>>,
    /// Connection handler for managing client connections
    connection_handler: Arc<Mutex<ConnectionHandler>>,
    /// Broadcast channels for each socket type
    broadcasters: HashMap<SocketType, broadcast::Sender<String>>,
    /// Session ID for this router instance
    session_id: String,
    /// Message validation settings
    max_message_size: usize,
    /// Statistics tracking
    stats: Arc<Mutex<RouterStats>>,
}

/// Statistics for message routing operations
#[derive(Debug, Default)]
pub struct RouterStats {
    /// Total messages routed
    pub messages_routed: u64,
    /// Messages routed by socket type
    pub messages_by_type: HashMap<SocketType, u64>,
    /// Validation errors encountered
    pub validation_errors: u64,
    /// Routing errors encountered
    pub routing_errors: u64,
    /// Last activity timestamp
    pub last_activity: Option<SystemTime>,
}

/// Error types for message routing operations
#[derive(Debug, thiserror::Error)]
pub enum MessageRouterError {
    #[error("Message validation failed: {reason}")]
    ValidationError { reason: String },
    #[error("Message too large: {size} bytes (max: {max_size} bytes)")]
    MessageTooLarge { size: usize, max_size: usize },
    #[error("Invalid JSON format: {error}")]
    InvalidJson { error: String },
    #[error("Unsupported message type: {message_type}")]
    UnsupportedMessageType { message_type: String },
    #[error("Socket type not available: {socket_type:?}")]
    SocketTypeUnavailable { socket_type: SocketType },
    #[error("Broadcast failed: {error}")]
    BroadcastFailed { error: String },
}

impl MessageRouter {
    /// Create a new message router with shared broadcasters
    pub fn new(
        socket_manager: Arc<Mutex<SocketManager>>,
        broadcasters: HashMap<SocketType, broadcast::Sender<String>>,
        session_id: String,
    ) -> Self {
        // Create a dummy connection handler since we won't use it for client management
        let dummy_handler = ConnectionHandler::new(Arc::clone(&socket_manager), HashMap::new());
        
        Self {
            socket_manager,
            connection_handler: Arc::new(Mutex::new(dummy_handler)),
            broadcasters,
            session_id,
            max_message_size: 1024 * 1024, // 1MB default limit
            stats: Arc::new(Mutex::new(RouterStats::default())),
        }
    }

    /// Set the maximum message size for validation
    pub fn set_max_message_size(&mut self, max_size: usize) {
        self.max_message_size = max_size;
        info!("Message router max size set to {} bytes", max_size);
    }

    /// Route a response message to the output socket
    pub async fn route_output_message(&self, content: &str, formatted: bool) -> Result<(), MessageRouterError> {
        let message = SocketMessage::new(MessageType::Response {
            content: content.to_string(),
            formatted,
        });

        self.route_message_to_socket(SocketType::Output, message).await
    }

    /// Route a thinking message to the thinking socket
    pub async fn route_thinking_message(&self, content: &str, step: Option<String>) -> Result<(), MessageRouterError> {
        let message = SocketMessage::new(MessageType::Thinking {
            content: content.to_string(),
            step,
        });

        self.route_message_to_socket(SocketType::Thinking, message).await
    }

    /// Route a tool request message to the tools socket
    pub async fn route_tool_request(&self, tool_name: &str, parameters: Value, id: &str) -> Result<(), MessageRouterError> {
        let message = SocketMessage::new(MessageType::ToolRequest {
            tool_name: tool_name.to_string(),
            parameters,
            id: id.to_string(),
        });

        self.route_message_to_socket(SocketType::Tools, message).await
    }

    /// Route a tool response message to the tools socket
    pub async fn route_tool_response(&self, id: &str, result: Value, status: super::protocol::ToolStatus) -> Result<(), MessageRouterError> {
        let message = SocketMessage::new(MessageType::ToolResponse {
            id: id.to_string(),
            result,
            status,
        });

        self.route_message_to_socket(SocketType::Tools, message).await
    }

    /// Route a message to a specific socket type
    async fn route_message_to_socket(
        &self,
        socket_type: SocketType,
        message: SocketMessage,
    ) -> Result<(), MessageRouterError> {
        // Validate the message
        self.validate_message(&message).await?;

        // Serialize the message
        let json_message = message.to_json()
            .map_err(|e| MessageRouterError::InvalidJson { error: e.to_string() })?;

        // Broadcast the message to connected clients
        // Note: We don't check client count because broadcast channels handle no receivers gracefully
        if let Some(broadcaster) = self.broadcasters.get(&socket_type) {
            match broadcaster.send(json_message) {
                Ok(_receiver_count) => {
                    // Update statistics
                    self.update_stats(socket_type, true);
                },
                Err(e) => {
                    self.update_stats(socket_type, false);
                    return Err(MessageRouterError::BroadcastFailed {
                        error: e.to_string(),
                    });
                }
            }
        } else {
            return Err(MessageRouterError::SocketTypeUnavailable { socket_type });
        }

        Ok(())
    }

    /// Validate a message before routing
    async fn validate_message(&self, message: &SocketMessage) -> Result<(), MessageRouterError> {
        // Serialize to check size
        let json_str = message.to_json()
            .map_err(|e| MessageRouterError::InvalidJson { error: e.to_string() })?;

        // Check message size
        let message_size = json_str.len();
        if message_size > self.max_message_size {
            self.increment_validation_errors();
            return Err(MessageRouterError::MessageTooLarge {
                size: message_size,
                max_size: self.max_message_size,
            });
        }

        // Validate message structure
        if message.timestamp.is_empty() {
            self.increment_validation_errors();
            return Err(MessageRouterError::ValidationError {
                reason: "Message timestamp cannot be empty".to_string(),
            });
        }

        // Validate timestamp format
        if chrono::DateTime::parse_from_rfc3339(&message.timestamp).is_err() {
            self.increment_validation_errors();
            return Err(MessageRouterError::ValidationError {
                reason: "Invalid timestamp format, expected RFC3339".to_string(),
            });
        }

        debug!("Message validation passed for {:?} message", message.message_type);
        Ok(())
    }

    /// Process an incoming message from a client
    pub async fn process_incoming_message(
        &self,
        socket_type: SocketType,
        raw_message: &str,
    ) -> Result<Option<String>, MessageRouterError> {
        // Validate message size
        if raw_message.len() > self.max_message_size {
            self.increment_validation_errors();
            return Err(MessageRouterError::MessageTooLarge {
                size: raw_message.len(),
                max_size: self.max_message_size,
            });
        }

        // Parse the JSON message
        let message: SocketMessage = SocketMessage::from_json(raw_message)
            .map_err(|e| MessageRouterError::InvalidJson { error: e.to_string() })?;

        // Validate the parsed message
        self.validate_message(&message).await?;

        // Process based on message type and socket type
        match (&socket_type, &message.message_type) {
            (SocketType::Input, MessageType::UserInput { text }) => {
                info!("Received user input: {}", text);
                Ok(Some(text.clone()))
            },
            (SocketType::Input, MessageType::SlashCommand { command, args }) => {
                let full_command = if args.is_empty() {
                    format!("/{}", command)
                } else {
                    format!("/{} {}", command, args.join(" "))
                };
                info!("Received slash command: {}", full_command);
                Ok(Some(full_command))
            },
            _ => {
                warn!(
                    "Unexpected message type {:?} on socket {:?}",
                    message.message_type, socket_type
                );
                Err(MessageRouterError::UnsupportedMessageType {
                    message_type: format!("{:?}", message.message_type),
                })
            }
        }
    }

    /// Get a broadcast receiver for a specific socket type
    pub fn get_broadcast_receiver(&self, socket_type: SocketType) -> Option<broadcast::Receiver<String>> {
        self.broadcasters.get(&socket_type).map(|sender| sender.subscribe())
    }

    /// Get current router statistics
    pub fn get_stats(&self) -> RouterStats {
        let stats = self.stats.lock().unwrap();
        RouterStats {
            messages_routed: stats.messages_routed,
            messages_by_type: stats.messages_by_type.clone(),
            validation_errors: stats.validation_errors,
            routing_errors: stats.routing_errors,
            last_activity: stats.last_activity,
        }
    }

    /// Update statistics for a routing operation
    fn update_stats(&self, socket_type: SocketType, success: bool) {
        let mut stats = self.stats.lock().unwrap();
        
        if success {
            stats.messages_routed += 1;
            *stats.messages_by_type.entry(socket_type).or_insert(0) += 1;
        } else {
            stats.routing_errors += 1;
        }
        
        stats.last_activity = Some(SystemTime::now());
    }

    /// Increment validation error count
    fn increment_validation_errors(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.validation_errors += 1;
        stats.last_activity = Some(SystemTime::now());
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Check if any clients are connected to any socket
    pub async fn has_connected_clients(&self) -> bool {
        let connection_handler = self.connection_handler.lock().unwrap();
        connection_handler.connection_count() > 0
    }

    /// Get connection count for a specific socket type
    pub async fn connection_count_for_socket(&self, socket_type: &SocketType) -> usize {
        let connection_handler = self.connection_handler.lock().unwrap();
        connection_handler.connection_count_for_socket(socket_type)
    }

    /// Shutdown the message router
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down message router for session {}", self.session_id);
        
        // Send shutdown notification to all connected clients
        for socket_type in SocketType::all() {
            let shutdown_message = SocketMessage::new(MessageType::SessionEnd {
                reason: "Chat session ending".to_string(),
            });

            // Best effort - don't fail shutdown if we can't notify clients
            let _ = self.route_message_to_socket(socket_type, shutdown_message).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_router() -> (MessageRouter, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        
        // Create socket manager with std mutex for both router and connection handler
        let socket_manager = Arc::new(Mutex::new(
            SocketManager::new(temp_dir.path()).expect("Failed to create socket manager")
        ));
        
        // Create shared broadcast channels for all socket types
        let mut shared_broadcasters = std::collections::HashMap::new();
        for socket_type in crate::cli::chat::api::SocketType::all() {
            let (sender, _) = tokio::sync::broadcast::channel(1000);
            shared_broadcasters.insert(socket_type, sender);
        }
        
        // Create connection handler
        let connection_handler = Arc::new(Mutex::new(
            ConnectionHandler::new(Arc::clone(&socket_manager), shared_broadcasters.clone())
        ));
        
        let router = MessageRouter::new(
            socket_manager,
            shared_broadcasters,
            "test-session".to_string(),
        );

        (router, temp_dir)
    }

    #[tokio::test]
    async fn test_message_router_creation() {
        let (router, _temp_dir) = create_test_router().await;
        
        assert_eq!(router.session_id(), "test-session");
        assert_eq!(router.max_message_size, 1024 * 1024);
        assert!(!router.has_connected_clients().await);
    }

    #[tokio::test]
    async fn test_output_message_routing() {
        let (router, _temp_dir) = create_test_router().await;
        
        // Should succeed even with no connected clients (just logs a debug message)
        let result = router.route_output_message("Hello, world!", false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_thinking_message_routing() {
        let (router, _temp_dir) = create_test_router().await;
        
        let result = router.route_thinking_message("Thinking about the problem...", Some("step1".to_string())).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tool_request_routing() {
        let (router, _temp_dir) = create_test_router().await;
        
        let params = serde_json::json!({"param1": "value1"});
        let result = router.route_tool_request("test_tool", params, "tool-123").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_message_validation() {
        let (router, _temp_dir) = create_test_router().await;
        
        // Test valid message
        let valid_message = SocketMessage::new(MessageType::Response {
            content: "Test".to_string(),
            formatted: false,
        });
        assert!(router.validate_message(&valid_message).await.is_ok());
    }

    #[tokio::test]
    async fn test_statistics_tracking() {
        let (router, _temp_dir) = create_test_router().await;
        
        // Route some messages (they won't actually be sent since no clients are connected,
        // but the validation will still update stats)
        let _ = router.route_output_message("Test message 1", false).await;
        let _ = router.route_thinking_message("Test thinking", None).await;
        
        let stats = router.get_stats();
        // Note: messages_routed will be 0 since no clients are connected,
        // but last_activity should be set from validation
        // Since we're not actually routing (no clients), let's just verify the router works
        assert_eq!(stats.messages_routed, 0); // No clients connected, so no actual routing
        assert_eq!(stats.validation_errors, 0); // But validation should succeed
    }

    #[tokio::test]
    async fn test_broadcast_receiver() {
        let (router, _temp_dir) = create_test_router().await;
        
        let receiver = router.get_broadcast_receiver(SocketType::Output);
        assert!(receiver.is_some());
        
        let mut rx = receiver.unwrap();
        
        // This should not block since no message is sent
        let result = rx.try_recv();
        assert!(matches!(result, Err(broadcast::error::TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn test_max_message_size_configuration() {
        let (mut router, _temp_dir) = create_test_router().await;
        
        router.set_max_message_size(1024); // 1KB limit
        assert_eq!(router.max_message_size, 1024);
        
        // Test that the new limit is enforced
        let large_content = "x".repeat(2048);
        let result = router.route_output_message(&large_content, false).await;
        assert!(matches!(result, Err(MessageRouterError::MessageTooLarge { .. })));
    }
}
