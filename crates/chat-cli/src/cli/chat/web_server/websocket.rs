use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use eyre::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::agent_env::session::Session;

use super::{
    api::ErrorResponse,
    events::{instant_to_unix_timestamp, WebUIEvent, WorkerLifecycleState as WebWorkerLifecycleState},
    AppState,
};

/// Commands sent from frontend to backend via WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketCommand {
    /// Send a prompt to the worker
    Prompt {
        worker_id: String,
        text: String,
    },
    /// Cancel the current job
    Cancel {
        worker_id: String,
    },
    /// Create a new worker
    CreateWorker {
        name: Option<String>,
        agent: String,
        working_directory: Option<String>,
    },
    /// Get all workers
    GetWorkers,
    /// Get conversation history for a worker
    GetConversationHistory {
        worker_id: String,
    },
    /// Ping to keep connection alive
    Ping,
}

impl WebSocketCommand {
    /// Validate the command
    pub fn validate(&self) -> Result<(), String> {
        match self {
            WebSocketCommand::Prompt { worker_id, text } => {
                if worker_id.trim().is_empty() {
                    return Err("Worker ID cannot be empty".to_string());
                }
                if text.trim().is_empty() {
                    return Err("Prompt text cannot be empty".to_string());
                }
                Ok(())
            }
            WebSocketCommand::Cancel { worker_id } => {
                if worker_id.trim().is_empty() {
                    return Err("Worker ID cannot be empty".to_string());
                }
                Ok(())
            }
            WebSocketCommand::CreateWorker { agent, .. } => {
                if agent.trim().is_empty() {
                    return Err("Agent name cannot be empty".to_string());
                }
                Ok(())
            }
            WebSocketCommand::GetWorkers => Ok(()),
            WebSocketCommand::GetConversationHistory { worker_id } => {
                if worker_id.trim().is_empty() {
                    return Err("Worker ID cannot be empty".to_string());
                }
                Ok(())
            }
            WebSocketCommand::Ping => Ok(()),
        }
    }
}

/// WebSocket upgrade handler
pub async fn websocket_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Upgrade to WebSocket (no worker validation needed)
    ws.on_upgrade(move |socket| handle_websocket(socket, state))
        .into_response()
}

/// Handle WebSocket connection
async fn handle_websocket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Publish WebSocketConnected event
    state.session.event_bus().publish(
        crate::agent_env::AgentEnvironmentEvent::WebUI(
            crate::agent_env::events::WebUIEvent::WebSocketConnected {
                timestamp: std::time::Instant::now(),
            }
        )
    );

    // Subscribe to events BEFORE sending snapshot to prevent race condition
    let mut event_rx = state.web_ui.subscribe();

    // Create channel for command responses
    let (response_tx, mut response_rx) = tokio::sync::mpsc::unbounded_channel::<WebUIEvent>();

    // Send initial snapshots (WorkersSnapshot, ConversationSnapshot for main worker)
    if let Err(e) = send_initial_snapshots(&mut sender, &state).await {
        tracing::error!("Failed to send initial snapshots: {}", e);
        return;
    }

    tracing::info!("WebSocket connected (global connection)");

    // Spawn task to forward events and responses to WebSocket
    let send_task = {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Forward events from EventBus
                    event_result = event_rx.recv() => {
                        match event_result {
                            Ok(event) => {
                                // Send ALL events (no filtering by worker_id)
                                // Frontend will handle routing to appropriate UI components

                                // Serialize to JSON
                                let json = match serde_json::to_string(&event) {
                                    Ok(json) => json,
                                    Err(e) => {
                                        tracing::error!("Failed to serialize event: {}", e);
                                        continue;
                                    }
                                };

                                // Send to WebSocket
                                if sender.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!("WebSocket lagged by {} events", n);
                                // Could send fresh snapshot here, but for MVP just log
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                break;
                            }
                        }
                    }
                    // Forward command responses
                    Some(response) = response_rx.recv() => {
                        let json = match serde_json::to_string(&response) {
                            Ok(json) => json,
                            Err(e) => {
                                tracing::error!("Failed to serialize response: {}", e);
                                continue;
                            }
                        };

                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        })
    };

    // Spawn task to handle incoming commands
    let session = state.session.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Err(e) = handle_command(&text, &session, &response_tx).await {
                    tracing::error!("Failed to handle command: {}", e);
                }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {
            tracing::info!("WebSocket send task completed");
        }
        _ = recv_task => {
            tracing::info!("WebSocket receive task completed");
        }
    }
}


/// Handle incoming command
async fn handle_command(
    text: &str,
    session: &Arc<Session>,
    response_tx: &tokio::sync::mpsc::UnboundedSender<WebUIEvent>,
) -> Result<()> {
    let command: WebSocketCommand = serde_json::from_str(text)?;

    // Validate command
    if let Err(e) = command.validate() {
        tracing::warn!("Invalid command: {}", e);
        return Ok(()); // Don't fail, just log
    }

    match command {
        WebSocketCommand::Prompt { worker_id, text } => {
            // Parse worker_id
            let worker_uuid = match Uuid::parse_str(&worker_id) {
                Ok(uuid) => uuid,
                Err(_) => {
                    tracing::error!("Invalid worker ID format: {}", worker_id);
                    let error_event = WebUIEvent::Error {
                        command: "prompt".to_string(),
                        message: "Invalid worker ID format".to_string(),
                        timestamp: super::serialization::current_unix_timestamp(),
                    };
                    let _ = response_tx.send(error_event);
                    return Ok(());
                }
            };

            // Get worker
            let worker = match session.get_worker(worker_uuid) {
                Some(w) => w,
                None => {
                    tracing::error!("Worker not found: {}", worker_id);
                    let error_event = WebUIEvent::Error {
                        command: "prompt".to_string(),
                        message: format!("Worker not found: {}", worker_id),
                        timestamp: super::serialization::current_unix_timestamp(),
                    };
                    let _ = response_tx.send(error_event);
                    return Ok(());
                }
            };

            // Add message to conversation history
            worker
                .context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(text.clone());

            // Publish WebUI event for prompt received
            session.event_bus().publish(crate::agent_env::AgentEnvironmentEvent::WebUI(
                crate::agent_env::events::WebUIEvent::PromptReceived {
                    worker_id: worker_uuid,
                    text: text.clone(),
                    timestamp: std::time::Instant::now(),
                }
            ));

            // Launch agent loop
            use crate::agent_env::worker_tasks::agent_loop::AgentLoopInput;
            if let Err(e) = session.run_task__agent_loop(worker, AgentLoopInput {}) {
                tracing::error!("Failed to start agent loop: {}", e);
                let error_event = WebUIEvent::Error {
                    command: "prompt".to_string(),
                    message: format!("Failed to start agent loop: {}", e),
                    timestamp: super::serialization::current_unix_timestamp(),
                };
                let _ = response_tx.send(error_event);
                return Ok(());
            }

            tracing::info!("Started agent loop for worker {}", worker_uuid);
        }
        WebSocketCommand::Cancel { worker_id } => {
            // Parse worker_id
            let worker_uuid = match Uuid::parse_str(&worker_id) {
                Ok(uuid) => uuid,
                Err(_) => {
                    tracing::error!("Invalid worker ID format: {}", worker_id);
                    let error_event = WebUIEvent::Error {
                        command: "cancel".to_string(),
                        message: "Invalid worker ID format".to_string(),
                        timestamp: super::serialization::current_unix_timestamp(),
                    };
                    let _ = response_tx.send(error_event);
                    return Ok(());
                }
            };

            // Cancel current job
            if let Err(e) = session.cancel_worker_jobs(worker_uuid) {
                tracing::error!("Failed to cancel job: {}", e);
                let error_event = WebUIEvent::Error {
                    command: "cancel".to_string(),
                    message: format!("Failed to cancel job: {}", e),
                    timestamp: super::serialization::current_unix_timestamp(),
                };
                let _ = response_tx.send(error_event);
                return Ok(());
            }
            
            tracing::info!("Cancelled job for worker {}", worker_uuid);
        }
        WebSocketCommand::CreateWorker { name, agent, working_directory } => {
            tracing::info!("CreateWorker command received: name={:?}, agent={}, working_directory={:?}", 
                name, agent, working_directory);
            
            // Generate worker name if not provided
            let worker_name = name.unwrap_or_else(|| generate_worker_name(&agent, session));
            
            // Create worker using Session's build_worker (simplified for MVP)
            // Note: For MVP, we're using the simple build_worker instead of WorkerBuilder
            // to avoid complexity with agent loading. This can be enhanced later.
            let worker = session.build_worker(worker_name.clone());
            
            // Store agent name and working directory in task_metadata for future use
            {
                let mut metadata = worker.task_metadata.lock().unwrap();
                metadata.insert("agent".to_string(), serde_json::Value::String(agent.clone()));
                if let Some(wd) = working_directory {
                    metadata.insert("working_directory".to_string(), serde_json::Value::String(wd));
                }
            }
            
            tracing::info!("Created worker: {} with agent: {}", worker_name, agent);
            // WorkerCreated event is automatically published by session.build_worker()
        }
        WebSocketCommand::GetWorkers => {
            tracing::info!("GetWorkers command received");
            
            // Get all workers from session
            let workers = session.get_workers();
            
            // Convert to WorkerMetadataJson
            use super::serialization::{convert_worker_metadata, current_unix_timestamp};
            let workers_metadata: Vec<_> = workers
                .iter()
                .map(|w| convert_worker_metadata(w))
                .collect();
            
            // Create WorkersSnapshot event
            let snapshot = WebUIEvent::WorkersSnapshot {
                workers: workers_metadata,
                timestamp: current_unix_timestamp(),
            };
            
            // Send via response channel
            if let Err(e) = response_tx.send(snapshot) {
                tracing::error!("Failed to send WorkersSnapshot: {}", e);
            }
        }
        WebSocketCommand::GetConversationHistory { worker_id } => {
            tracing::info!("GetConversationHistory command received for worker {}", worker_id);
            
            // Parse worker_id
            let worker_uuid = match Uuid::parse_str(&worker_id) {
                Ok(uuid) => uuid,
                Err(_) => {
                    tracing::error!("Invalid worker ID format: {}", worker_id);
                    let error_event = WebUIEvent::Error {
                        command: "get_conversation_history".to_string(),
                        message: "Invalid worker ID format".to_string(),
                        timestamp: super::serialization::current_unix_timestamp(),
                    };
                    let _ = response_tx.send(error_event);
                    return Ok(());
                }
            };
            
            // Get worker from session
            let worker = match session.get_worker(worker_uuid) {
                Some(w) => w,
                None => {
                    tracing::error!("Worker not found: {}", worker_id);
                    let error_event = WebUIEvent::Error {
                        command: "get_conversation_history".to_string(),
                        message: format!("Worker not found: {}", worker_id),
                        timestamp: super::serialization::current_unix_timestamp(),
                    };
                    let _ = response_tx.send(error_event);
                    return Ok(());
                }
            };
            
            // Get conversation history from worker's context container
            let history = worker
                .context_container
                .conversation_history
                .lock()
                .unwrap();
            
            // Convert entries to ConversationEntryJson
            use super::serialization::{convert_conversation_entry, current_unix_timestamp};
            let entries: Vec<_> = history
                .get_entries()
                .iter()
                .map(convert_conversation_entry)
                .collect();
            
            // Create ConversationSnapshot event
            let snapshot = WebUIEvent::ConversationSnapshot {
                worker_id,
                entries,
                timestamp: current_unix_timestamp(),
            };
            
            // Send via response channel
            if let Err(e) = response_tx.send(snapshot) {
                tracing::error!("Failed to send ConversationSnapshot: {}", e);
            }
        }
        WebSocketCommand::Ping => {
            // No-op, just keep connection alive
            tracing::debug!("Ping received");
        }
    }

    Ok(())
}

/// Generate a unique worker name based on agent name and existing workers
fn generate_worker_name(agent: &str, session: &Arc<Session>) -> String {
    let workers = session.get_workers();
    
    // Count workers with same agent prefix
    let count = workers.iter()
        .filter(|w| w.name.starts_with(agent))
        .count();
    
    format!("{}-{}", agent, count + 1)
}

/// Send initial snapshots on WebSocket connection
async fn send_initial_snapshots(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &AppState,
) -> Result<()> {
    use super::serialization::{convert_worker_metadata, convert_conversation_entry, current_unix_timestamp};
    
    // Send WorkersSnapshot
    let workers = state.session.get_workers();
    let workers_metadata: Vec<_> = workers
        .iter()
        .map(|w| convert_worker_metadata(w))
        .collect();
    
    let workers_snapshot = WebUIEvent::WorkersSnapshot {
        workers: workers_metadata,
        timestamp: current_unix_timestamp(),
    };
    
    let json = serde_json::to_string(&workers_snapshot)?;
    sender.send(Message::Text(json)).await?;
    
    // Send ConversationSnapshot for main worker (if exists)
    if let Some(main_worker) = workers.iter().find(|w| w.name == "main") {
        let entries: Vec<_> = {
            let history = main_worker
                .context_container
                .conversation_history
                .lock()
                .unwrap();
            
            history
                .get_entries()
                .iter()
                .map(convert_conversation_entry)
                .collect()
        }; // history lock dropped here
        
        let conversation_snapshot = WebUIEvent::ConversationSnapshot {
            worker_id: main_worker.id.to_string(),
            entries,
            timestamp: current_unix_timestamp(),
        };
        
        let json = serde_json::to_string(&conversation_snapshot)?;
        sender.send(Message::Text(json)).await?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_prompt_command() {
        let json = r#"{"type": "prompt", "text": "Hello, world!"}"#;
        let cmd: WebSocketCommand = serde_json::from_str(json).unwrap();
        
        match cmd {
            WebSocketCommand::Prompt { text } => {
                assert_eq!(text, "Hello, world!");
            }
            _ => panic!("Expected Prompt command"),
        }
    }

    #[test]
    fn test_parse_cancel_command() {
        let json = r#"{"type": "cancel"}"#;
        let cmd: WebSocketCommand = serde_json::from_str(json).unwrap();
        
        matches!(cmd, WebSocketCommand::Cancel);
    }

    #[test]
    fn test_parse_ping_command() {
        let json = r#"{"type": "ping"}"#;
        let cmd: WebSocketCommand = serde_json::from_str(json).unwrap();
        
        matches!(cmd, WebSocketCommand::Ping);
    }

    #[test]
    fn test_validate_prompt_empty() {
        let cmd = WebSocketCommand::Prompt {
            text: "   ".to_string(),
        };
        assert!(cmd.validate().is_err());
    }

    #[test]
    fn test_validate_prompt_valid() {
        let cmd = WebSocketCommand::Prompt {
            text: "Hello".to_string(),
        };
        assert!(cmd.validate().is_ok());
    }

    #[test]
    fn test_validate_cancel() {
        let cmd = WebSocketCommand::Cancel;
        assert!(cmd.validate().is_ok());
    }

    #[test]
    fn test_validate_ping() {
        let cmd = WebSocketCommand::Ping;
        assert!(cmd.validate().is_ok());
    }
}
