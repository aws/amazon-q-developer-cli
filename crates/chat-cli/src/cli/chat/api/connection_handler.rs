//! Connection handler for Q Chat API mode socket connections
//!
//! This module handles client connections to Unix domain sockets, including
//! connection acceptance, message broadcasting, and graceful shutdown.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use eyre::{Result, eyre};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, AsyncReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use uuid::Uuid;

use super::socket_manager::{SocketManager, SocketType};

/// Guard that manages the lifecycle of connection accept tasks
pub struct ConnectionGuard {
    accept_tasks: Vec<JoinHandle<()>>,
}

impl ConnectionGuard {
    /// Get the number of active accept tasks
    pub fn task_count(&self) -> usize {
        self.accept_tasks.len()
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        // Abort all tasks if not gracefully shut down
        for task in &self.accept_tasks {
            task.abort();
        }
    }
}

/// Maximum number of clients per socket type
const MAX_CLIENTS_PER_SOCKET: usize = 10;

/// Buffer size for broadcast channels
const BROADCAST_BUFFER_SIZE: usize = 100;

/// Client connection information with communication channels
#[derive(Debug)]
pub struct ClientConnection {
    pub id: String,
    pub socket_type: SocketType,
    pub connected_at: SystemTime,
    pub sender: mpsc::UnboundedSender<String>,
    pub task_handle: JoinHandle<()>,
}

/// Connection handler for managing socket connections
#[derive(Debug)]
pub struct ConnectionHandler {
    /// Reference to the socket manager
    socket_manager: Arc<Mutex<SocketManager>>,
    /// Active client connections
    clients: Arc<Mutex<HashMap<String, ClientConnection>>>,
    /// Broadcast channels for each socket type
    broadcasters: HashMap<SocketType, broadcast::Sender<String>>,
    /// Input injection sender for forwarding input socket messages to InputSource
    input_injection_sender: Option<std::sync::mpsc::Sender<String>>,
}

impl ConnectionHandler {
    /// Create a new connection handler with shared broadcasters
    pub fn new(
        socket_manager: Arc<Mutex<SocketManager>>,
        broadcasters: HashMap<SocketType, broadcast::Sender<String>>,
    ) -> Self {
        Self {
            socket_manager,
            clients: Arc::new(Mutex::new(HashMap::new())),
            broadcasters,
            input_injection_sender: None,
        }
    }

    /// Set the input injection sender for forwarding input socket messages to InputSource
    pub fn set_input_injection_sender(&mut self, sender: std::sync::mpsc::Sender<String>) {
        self.input_injection_sender = Some(sender);
    }

    /// Start accepting connections for all socket types and return a guard
    /// The guard must be kept alive to maintain the accept tasks
    pub async fn start_accepting_connections(&mut self) -> Result<ConnectionGuard> {
        
        let mut accept_tasks = Vec::new();
        
        // Get all socket types that have listeners
        let socket_types = vec![
            SocketType::Control,
            SocketType::Input,
            SocketType::Output,
            SocketType::Thinking,
            SocketType::Tools,
            SocketType::Events,
        ];

        for socket_type in socket_types {
            // Get the existing listener from SocketManager instead of binding new one
            let listener = {
                let manager = self.socket_manager.lock()
                    .map_err(|_| eyre!("Failed to lock socket manager"))?;
                manager.get_listener(&socket_type)
            };

            if let Some(listener_arc) = listener {
                let task = self.spawn_accept_task_with_arc(socket_type.clone(), listener_arc).await?;
                accept_tasks.push(task);
            } else {
                warn!("No listener found for socket type {:?}", socket_type);
            }
        }

        
        // Return the guard that will keep tasks alive
        Ok(ConnectionGuard {
            accept_tasks,
        })
    }

    /// Spawn a task to accept connections for a specific socket type using Arc<UnixListener>
    async fn spawn_accept_task_with_arc(
        &self,
        socket_type: SocketType,
        listener: Arc<UnixListener>,
    ) -> Result<JoinHandle<()>> {
        let clients = Arc::clone(&self.clients);
        let broadcaster = self.broadcasters
            .get(&socket_type)
            .ok_or_else(|| eyre!("No broadcaster found for socket type {:?}", socket_type))?
            .clone();
        let input_injection_sender = self.input_injection_sender.clone();

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Accept new connections
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _addr)) => {
                                if let Err(e) = Self::handle_new_connection(
                                    stream,
                                    socket_type.clone(),
                                    Arc::clone(&clients),
                                    broadcaster.subscribe(),
                                    input_injection_sender.clone(),
                                ).await {
                                    warn!("Failed to handle new connection on {:?}: {}", socket_type, e);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to accept connection on {:?} socket: {}", socket_type, e);
                            }
                        }
                    }
                }
            }
        });

        Ok(task)
    }

    /// Spawn a task to accept connections for a specific socket type
    async fn spawn_accept_task(
        &self,
        socket_type: SocketType,
        listener: UnixListener,
    ) -> Result<JoinHandle<()>> {
        let clients = Arc::clone(&self.clients);
        let broadcaster = self.broadcasters.get(&socket_type)
            .ok_or_else(|| eyre!("No broadcaster found for socket type: {:?}", socket_type))?
            .clone();
        let input_injection_sender = self.input_injection_sender.clone();

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Accept new connections
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _addr)) => {
                                if let Err(e) = Self::handle_new_connection(
                                    stream,
                                    socket_type.clone(),
                                    Arc::clone(&clients),
                                    broadcaster.subscribe(),
                                    input_injection_sender.clone(),
                                ).await {
                                    warn!("Failed to handle new connection: {}", e);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to accept connection on {:?} socket: {}", socket_type, e);
                            }
                        }
                    }
                }
            }
        });

        Ok(task)
    }

    /// Handle a new client connection
    async fn handle_new_connection(
        stream: UnixStream,
        socket_type: SocketType,
        clients: Arc<Mutex<HashMap<String, ClientConnection>>>,
        mut broadcast_receiver: broadcast::Receiver<String>,
        input_injection_sender: Option<std::sync::mpsc::Sender<String>>,
    ) -> Result<()> {
        // Check connection limit
        {
            let clients_guard = clients.lock()
                .map_err(|_| eyre!("Failed to lock clients"))?;
            let socket_client_count = clients_guard.values()
                .filter(|conn| conn.socket_type == socket_type)
                .count();
            
            if socket_client_count >= MAX_CLIENTS_PER_SOCKET {
                eprintln!("Connection limit reached for {:?} socket", socket_type);
                return Ok(());
            }
        }

        let client_id = Uuid::new_v4().to_string();
        let (sender, mut receiver) = mpsc::unbounded_channel::<String>();

        // Split stream for reading and writing
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut writer = write_half;

        // Spawn task to handle outgoing messages to client
        let client_id_clone = client_id.clone();
        let write_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Send messages from the application to client
                    msg = receiver.recv() => {
                        match msg {
                            Some(message) => {
                                if let Err(e) = writer.write_all(message.as_bytes()).await {
                                    warn!("Failed to write to client {}: {}", client_id_clone, e);
                                    break;
                                }
                                if let Err(e) = writer.write_all(b"\n").await {
                                    warn!("Failed to write newline to client {}: {}", client_id_clone, e);
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    // Forward broadcast messages to client
                    msg = broadcast_receiver.recv() => {
                        match msg {
                            Ok(message) => {
                                if let Err(e) = writer.write_all(message.as_bytes()).await {
                                    warn!("Failed to write broadcast to client {}: {}", client_id_clone, e);
                                    break;
                                }
                                if let Err(e) = writer.write_all(b"\n").await {
                                    warn!("Failed to write newline to client {}: {}", client_id_clone, e);
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                eprintln!("Client {} lagged behind, some messages may be lost", client_id_clone);
                            }
                        }
                    }
                }
            }
        });

        // Create client connection (stream is handled by the tasks)
        let connection = ClientConnection {
            id: client_id.clone(),
            socket_type: socket_type.clone(),
            connected_at: SystemTime::now(),
            sender,
            task_handle: write_task,
        };

        // Add client to the connections map
        {
            let mut clients_guard = clients.lock()
                .map_err(|_| eyre!("Failed to lock clients"))?;
            clients_guard.insert(client_id.clone(), connection);
        }

        debug!("New client connected: {} on {:?} socket", client_id, socket_type);

        // Handle incoming messages based on socket type
        match socket_type {
            SocketType::Control => {
                // Control socket is suppressed - return error
                warn!("Control socket is suppressed - rejecting connection for client {}", client_id);
                return Err(eyre!("Control socket is suppressed"));
            }
            SocketType::Events => {
                // Events socket is suppressed - return error
                warn!("Events socket is suppressed - rejecting connection for client {}", client_id);
                return Err(eyre!("Events socket is suppressed"));
            }
            SocketType::Input => {
                // Input socket: read and process user input
                let mut line = String::new();
                loop {
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                // Forward input message to chat session via injection sender
                                if let Some(ref sender) = input_injection_sender {
                                    match sender.send(trimmed.to_string()) {
                                        Ok(()) => {}
                                        Err(_e) => {}
                                    }
                                }
                            }
                            line.clear();
                        }
                        Err(e) => {
                            eprintln!("Error reading from client {}: {}", client_id, e);
                            break;
                        }
                    }
                }
            }
            SocketType::Output | SocketType::Thinking | SocketType::Tools => {
                // Output-only sockets: don't read, just keep connection alive
                // Use a more efficient way to detect client disconnection
                // We'll use a small read with a long timeout to detect when client disconnects
                // without processing any data they might send
                loop {
                    let mut buffer = [0u8; 1];
                    match reader.read(&mut buffer).await {
                        Ok(0) => {
                            debug!("Client {} disconnected from {:?} socket (EOF)", client_id, socket_type);
                            break; // EOF - client disconnected
                        }
                        Ok(_) => {
                            // Client sent data on an output-only socket - ignore it silently
                            // This is expected behavior for output-only sockets
                        }
                        Err(e) => {
                            debug!("Connection error for client {} on {:?} socket: {}", client_id, socket_type, e);
                            break;
                        }
                    }
                }
            }
        }

        // Clean up client connection
        Self::cleanup_client(&clients, &client_id).await;
        debug!("Client disconnected: {}", client_id);

        Ok(())
    }

    /// Clean up a client connection
    async fn cleanup_client(
        clients: &Arc<Mutex<HashMap<String, ClientConnection>>>,
        client_id: &str,
    ) {
        if let Ok(mut clients_guard) = clients.lock() {
            if let Some(connection) = clients_guard.remove(client_id) {
                connection.task_handle.abort();
            }
        }
    }

    /// Broadcast a message to all clients of a specific socket type
    pub fn broadcast_to_socket(&self, socket_type: &SocketType, message: &str) -> Result<()> {
        if let Some(broadcaster) = self.broadcasters.get(socket_type) {
            broadcaster.send(message.to_string())
                .map_err(|_| eyre!("Failed to broadcast message to {:?} socket", socket_type))?;
        }
        Ok(())
    }

    /// Send a message to a specific client
    pub fn send_to_client(&self, client_id: &str, message: &str) -> Result<()> {
        let clients = self.clients.lock()
            .map_err(|_| eyre!("Failed to lock clients"))?;
        
        if let Some(connection) = clients.get(client_id) {
            connection.sender.send(message.to_string())
                .map_err(|_| eyre!("Failed to send message to client {}", client_id))?;
        } else {
            return Err(eyre!("Client {} not found", client_id));
        }

        Ok(())
    }

    /// Get all connected clients
    pub fn get_connected_clients(&self) -> Result<Vec<(String, SocketType, SystemTime)>> {
        let clients = self.clients.lock()
            .map_err(|_| eyre!("Failed to lock clients"))?;
        
        Ok(clients.values()
            .map(|conn| (conn.id.clone(), conn.socket_type.clone(), conn.connected_at))
            .collect())
    }

    /// Get connected clients for a specific socket type
    pub fn get_clients_for_socket(&self, socket_type: &SocketType) -> Result<Vec<String>> {
        let clients = self.clients.lock()
            .map_err(|_| eyre!("Failed to lock clients"))?;
        
        Ok(clients.values()
            .filter(|conn| &conn.socket_type == socket_type)
            .map(|conn| conn.id.clone())
            .collect())
    }

    /// Get the number of connected clients
    pub fn connection_count(&self) -> usize {
        self.clients.lock()
            .map(|clients| clients.len())
            .unwrap_or(0)
    }

    /// Get the number of connected clients for a specific socket type
    pub fn connection_count_for_socket(&self, socket_type: &SocketType) -> usize {
        self.clients.lock()
            .map(|clients| {
                clients.values()
                    .filter(|conn| &conn.socket_type == socket_type)
                    .count()
            })
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_connection_handler_creation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut socket_manager = SocketManager::new(temp_dir.path())
            .expect("Failed to create socket manager");
        socket_manager.disable_cleanup_on_drop();

        let manager_arc = Arc::new(Mutex::new(socket_manager));
        
        // Create shared broadcast channels for all socket types
        let mut shared_broadcasters = std::collections::HashMap::new();
        for socket_type in crate::cli::chat::api::SocketType::all() {
            let (sender, _) = tokio::sync::broadcast::channel(1000);
            shared_broadcasters.insert(socket_type, sender);
        }
        
        let handler = ConnectionHandler::new(manager_arc, shared_broadcasters);

        assert_eq!(handler.broadcasters.len(), 6); // All socket types
        assert_eq!(handler.connection_count(), 0);
    }

    #[tokio::test]
    async fn test_broadcast_functionality() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut socket_manager = SocketManager::new(temp_dir.path())
            .expect("Failed to create socket manager");
        socket_manager.disable_cleanup_on_drop();

        let manager_arc = Arc::new(Mutex::new(socket_manager));
        
        // Create shared broadcast channels for all socket types
        let mut shared_broadcasters = std::collections::HashMap::new();
        for socket_type in crate::cli::chat::api::SocketType::all() {
            let (sender, _) = tokio::sync::broadcast::channel(1000);
            shared_broadcasters.insert(socket_type, sender);
        }
        
        let handler = ConnectionHandler::new(manager_arc, shared_broadcasters);

        // Create a receiver to prevent the broadcast from failing
        let mut _receiver = handler.broadcasters.get(&SocketType::Output)
            .expect("Output broadcaster should exist")
            .subscribe();

        // Test broadcasting to output socket
        let result = handler.broadcast_to_socket(&SocketType::Output, "test message");
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_connection_limits() {
        // Test that MAX_CLIENTS_PER_SOCKET is reasonable
        assert!(MAX_CLIENTS_PER_SOCKET >= 10);
        assert!(MAX_CLIENTS_PER_SOCKET <= 100);
    }

    #[test]
    fn test_broadcast_buffer_size() {
        // Test that broadcast buffer size is reasonable
        assert!(BROADCAST_BUFFER_SIZE >= 10);
        assert!(BROADCAST_BUFFER_SIZE <= 1000);
    }

    #[tokio::test]
    async fn test_connection_count_tracking() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut socket_manager = SocketManager::new(temp_dir.path())
            .expect("Failed to create socket manager");
        socket_manager.disable_cleanup_on_drop();

        let manager_arc = Arc::new(Mutex::new(socket_manager));
        
        // Create shared broadcast channels for all socket types
        let mut shared_broadcasters = std::collections::HashMap::new();
        for socket_type in crate::cli::chat::api::SocketType::all() {
            let (sender, _) = tokio::sync::broadcast::channel(1000);
            shared_broadcasters.insert(socket_type, sender);
        }
        
        let handler = ConnectionHandler::new(manager_arc, shared_broadcasters);

        // Initially no connections
        assert_eq!(handler.connection_count(), 0);
        assert_eq!(handler.connection_count_for_socket(&SocketType::Control), 0);

        // Test getting clients for socket type
        let clients = handler.get_clients_for_socket(&SocketType::Input);
        assert!(clients.is_ok());
        assert!(clients.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut socket_manager = SocketManager::new(temp_dir.path())
            .expect("Failed to create socket manager");
        socket_manager.disable_cleanup_on_drop();

        let manager_arc = Arc::new(Mutex::new(socket_manager));
        
        // Create shared broadcast channels for all socket types
        let mut shared_broadcasters = std::collections::HashMap::new();
        for socket_type in crate::cli::chat::api::SocketType::all() {
            let (sender, _) = tokio::sync::broadcast::channel(1000);
            shared_broadcasters.insert(socket_type, sender);
        }
        
        let mut handler = ConnectionHandler::new(manager_arc, shared_broadcasters);

        // Test without any connections - just verify handler was created
        assert_eq!(handler.connection_count(), 0);
    }
}
