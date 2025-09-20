use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;

use agent_client_protocol as acp;
use acp::Client;
use clap::Parser;
use eyre::Result;
use serde_json::value::RawValue;
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::cli::agent::Agents;
use crate::cli::ConversationState;
use crate::cli::chat::tool_manager::ToolManager;
use crate::database::settings::Setting;
use crate::os::Os;


#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_harness;


#[derive(Debug, Parser, PartialEq)]
pub struct AcpArgs {
    /// Agent to use for ACP sessions
    #[arg(long)]
    pub agent: Option<String>,
}

pub struct AcpServerHandle {
    _shutdown_tx: oneshot::Sender<()>,
}

impl AcpServerHandle {
    pub async fn shutdown(self) {
        // Send shutdown signal (receiver will handle graceful shutdown)
        let _ = self._shutdown_tx.send(());
    }
}

/// Spawn an ACP server that communicates over stdio
/// Returns a handle that can be used to shut down the server
pub async fn spawn_acp_server(
    agent_name: String,
    os: Os,
) -> Result<AcpServerHandle> {
    let stdin = tokio::io::stdin().compat();
    let stdout = tokio::io::stdout().compat_write();
    spawn_acp_server_with_streams(agent_name, os, stdout, stdin).await
}

/// Spawn an ACP server with custom input/output streams
/// Useful for testing with in-memory streams
pub async fn spawn_acp_server_with_streams<W, R>(
    agent_name: String,
    os: Os,
    writer: W,
    reader: R,
) -> Result<AcpServerHandle> 
where
    W: futures::AsyncWrite + Unpin + Send + 'static,
    R: futures::AsyncRead + Unpin + Send + 'static,
{
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    
    // Spawn the ACP server in a LocalSet since ACP futures are !Send
    tokio::task::spawn_local(async move {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent = QAgent::new(agent_name, os, tx);
        
        let (connection, handle_io) = acp::AgentSideConnection::new(
            agent, 
            writer, 
            reader, 
            |fut| {
                tokio::task::spawn_local(fut);
            }
        );
        
        // Spawn background task to handle session notifications
        let mut shutdown_rx_clone = shutdown_rx;
        tokio::task::spawn_local(async move {
            loop {
                tokio::select! {
                    Some((session_notification, tx)) = rx.recv() => {
                        let result = connection.session_notification(session_notification).await;
                        if let Err(e) = result {
                            tracing::error!("Failed to send session notification: {}", e);
                            break;
                        }
                        tx.send(()).ok();
                    }
                    _ = &mut shutdown_rx_clone => {
                        tracing::info!("ACP server shutdown requested");
                        break;
                    }
                }
            }
        });
        
        tracing::info!("ACP server started, waiting for client connections...");
        
        // Run the connection (this will block until the client disconnects)
        if let Err(e) = handle_io.await {
            tracing::error!("ACP connection error: {}", e);
        }
        
        tracing::info!("ACP server shutting down gracefully");
    });
    
    Ok(AcpServerHandle {
        _shutdown_tx: shutdown_tx,
    })
}

struct QAgent {
    _agent_name: String,
    os: Arc<RwLock<Os>>,
    sessions: Arc<RwLock<HashMap<String, ConversationState>>>,
    session_update_tx: mpsc::UnboundedSender<(acp::SessionNotification, oneshot::Sender<()>)>,
}

impl QAgent {
    fn new(
        agent_name: String, 
        os: Os,
        session_update_tx: mpsc::UnboundedSender<(acp::SessionNotification, oneshot::Sender<()>)>,
    ) -> Self {
        Self { 
            _agent_name: agent_name,
            os: Arc::new(RwLock::new(os)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_update_tx,
        }
    }
}

impl acp::Agent for QAgent {
    async fn initialize(
        &self,
        arguments: acp::InitializeRequest,
    ) -> Result<acp::InitializeResponse, acp::Error> {
        tracing::info!("ACP initialize request: {arguments:?}");
        Ok(acp::InitializeResponse {
            protocol_version: acp::V1,
            agent_capabilities: acp::AgentCapabilities {
                load_session: true,
                prompt_capabilities: acp::PromptCapabilities::default(),
                mcp_capabilities: acp::McpCapabilities::default(),
                meta: None,
            },
            auth_methods: Vec::new(),
            meta: None,
        })
    }

    async fn authenticate(
        &self,
        _arguments: acp::AuthenticateRequest,
    ) -> Result<acp::AuthenticateResponse, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn new_session(
        &self,
        arguments: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        tracing::info!("ACP new_session request: {arguments:?}");
        
        // Generate a new session ID
        let session_id = uuid::Uuid::new_v4().to_string();
        
        // Get OS reference
        let mut os = self.os.write().await;
        
        // Create agents (using default for now)
        let agents = Agents::default();
        
        // Create tool manager
        let mut tool_manager = ToolManager::default();
        let tool_config = tool_manager.load_tools(&mut *os, &mut vec![]).await
            .map_err(|e| {
                tracing::error!("Failed to load tools: {}", e);
                acp::Error::internal_error()
            })?;
        
        // Create new conversation state
        let conversation = ConversationState::new(
            &session_id,
            agents,
            tool_config,
            tool_manager,
            None, // model_id
            &*os,
            false, // mcp_enabled for now
        ).await;
        
        // Store the session
        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), conversation);
        
        tracing::info!("Created new ACP session: {}", session_id);
        
        Ok(acp::NewSessionResponse {
            session_id: acp::SessionId(session_id.into()),
            modes: None,
            meta: None,
        })
    }

    async fn load_session(
        &self,
        arguments: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        tracing::info!("ACP load_session request: {arguments:?}");
        
        let session_id = arguments.session_id.0.as_ref();
        
        // Check if session exists
        let sessions = self.sessions.read().await;
        if sessions.contains_key(session_id) {
            tracing::info!("Loaded existing ACP session: {}", session_id);
            Ok(acp::LoadSessionResponse {
                modes: None,
                meta: None,
            })
        } else {
            tracing::warn!("Session not found: {}", session_id);
            Err(acp::Error::invalid_params())
        }
    }

    async fn prompt(
        &self,
        arguments: acp::PromptRequest,
    ) -> Result<acp::PromptResponse, acp::Error> {
        tracing::info!("ACP prompt request: session_id={}", arguments.session_id.0);
        
        let session_id = arguments.session_id.0.as_ref();
        
        // Convert ACP ContentBlocks to a single prompt string
        let mut prompt_text = String::new();
        for content_block in arguments.prompt {
            match content_block {
                acp::ContentBlock::Text(text_content) => {
                    if !prompt_text.is_empty() {
                        prompt_text.push('\n');
                    }
                    prompt_text.push_str(&text_content.text);
                },
                acp::ContentBlock::ResourceLink(resource_link) => {
                    // For now, just include the URI as text
                    if !prompt_text.is_empty() {
                        prompt_text.push('\n');
                    }
                    prompt_text.push_str(&format!("Resource: {}", resource_link.uri));
                },
                acp::ContentBlock::Resource(embedded_resource) => {
                    // Include the resource contents
                    if !prompt_text.is_empty() {
                        prompt_text.push('\n');
                    }
                    match &embedded_resource.resource {
                        acp::EmbeddedResourceResource::TextResourceContents(text_resource) => {
                            prompt_text.push_str(&format!("Resource {}: {}", 
                                text_resource.uri, 
                                text_resource.text));
                        },
                        acp::EmbeddedResourceResource::BlobResourceContents(blob_resource) => {
                            prompt_text.push_str(&format!("Resource {}: [Binary content]", 
                                blob_resource.uri));
                        },
                    }
                },
                acp::ContentBlock::Image(_) | acp::ContentBlock::Audio(_) => {
                    // Not supported yet - skip or add placeholder
                    if !prompt_text.is_empty() {
                        prompt_text.push('\n');
                    }
                    prompt_text.push_str("[Unsupported content type]");
                },
            }
        }
        
        // Store the message in the session
        {
            let mut sessions = self.sessions.write().await;
            let conversation = sessions.get_mut(session_id)
                .ok_or_else(|| {
                    tracing::warn!("Session not found: {}", session_id);
                    acp::Error::invalid_params()
                })?;
            
            // Add the prompt to the conversation state
            conversation.set_next_user_message(prompt_text.clone()).await;
        }
        
        // For now, send a simple echo response to test streaming
        let response_text = format!("Echo: {}", prompt_text);
        
        // Send the response as chunks to test streaming
        for chunk in response_text.chars().collect::<Vec<_>>().chunks(10) {
            let chunk_text: String = chunk.iter().collect();
            
            let (tx, rx) = oneshot::channel();
            let notification = acp::SessionNotification {
                session_id: arguments.session_id.clone(),
                update: acp::SessionUpdate::AgentMessageChunk {
                    content: acp::ContentBlock::Text(acp::TextContent {
                        text: chunk_text,
                        annotations: None,
                        meta: None,
                    }),
                },
                meta: None,
            };
            
            // Send notification via channel
            if let Err(_) = self.session_update_tx.send((notification, tx)) {
                // Channel closed - likely in test mode without receiver
                tracing::debug!("Notification channel closed, skipping notification");
                continue;
            }
            
            // Wait for acknowledgment with timeout for testing
            match tokio::time::timeout(tokio::time::Duration::from_millis(100), rx).await {
                Ok(Ok(())) => {
                    // Normal case - acknowledgment received
                }
                Ok(Err(_)) => {
                    tracing::debug!("Notification acknowledgment channel closed");
                    break;
                }
                Err(_) => {
                    // Timeout - likely in test mode, continue anyway
                    tracing::debug!("Notification acknowledgment timeout, continuing");
                }
            }
            
            // Small delay to simulate streaming
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        
        tracing::info!("ACP prompt completed for session: {}", session_id);
        Ok(acp::PromptResponse {
            stop_reason: acp::StopReason::EndTurn,
            meta: None,
        })
    }

    async fn cancel(&self, _args: acp::CancelNotification) -> Result<(), acp::Error> {
        // Not implemented yet
        Ok(())
    }

    async fn set_session_mode(
        &self,
        _args: acp::SetSessionModeRequest,
    ) -> Result<acp::SetSessionModeResponse, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn ext_method(
        &self,
        _method: Arc<str>,
        _params: Arc<RawValue>,
    ) -> Result<Arc<RawValue>, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(
        &self,
        _method: Arc<str>,
        _params: Arc<RawValue>,
    ) -> Result<(), acp::Error> {
        // Not implemented yet
        Ok(())
    }
}

impl AcpArgs {
    pub async fn run(self, os: &mut Os) -> Result<ExitCode> {
        // Check feature flag
        if !os.database.settings.get_bool(Setting::EnabledAcp).unwrap_or(false) {
            eprintln!("ACP is disabled. Enable with: q settings acp.enabled true");
            return Ok(ExitCode::FAILURE);
        }

        let agent_name = self.agent.unwrap_or_else(|| "default".to_string());
        
        tracing::info!("Starting ACP server with agent: {}", agent_name);
        
        // Create ACP server with LocalSet for non-Send futures
        let local_set = tokio::task::LocalSet::new();
        local_set.run_until(async move {
            let _handle = spawn_acp_server(agent_name, os.clone()).await?;
            
            // Wait indefinitely (until Ctrl+C or client disconnects)
            // The handle will automatically shut down when dropped
            tokio::signal::ctrl_c().await.map_err(|e| eyre::eyre!("Failed to listen for ctrl+c: {}", e))?;
            
            tracing::info!("ACP server shutting down");
            Ok::<(), eyre::Error>(())
        }).await?;
        
        Ok(ExitCode::SUCCESS)
    }
}
