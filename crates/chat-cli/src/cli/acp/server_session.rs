//! ACP Session Actor - Per-session actor that owns conversation state

use agent_client_protocol as acp;
use tokio::sync::{mpsc, oneshot};
use eyre::Result;
use std::collections::HashMap;

use crate::os::Os;
use crate::cli::chat::{ConversationState, SendMessageStream, ResponseEvent};
use super::server_connection::AcpServerConnectionHandle;

/// Convert channel errors to ACP errors
fn channel_to_acp_error<E>(_err: E) -> acp::Error {
    acp::Error::internal_error()
}

/// Handle to a session actor
#[derive(Clone)]
pub struct AcpServerSessionHandle {
    session_tx: mpsc::Sender<ServerSessionMethod>,
}

/// Messages sent to session actors
#[derive(Debug)]
enum ServerSessionMethod {
    Prompt(acp::PromptRequest, oneshot::Sender<Result<acp::PromptResponse, acp::Error>>),
    Cancel(acp::CancelNotification, oneshot::Sender<Result<(), acp::Error>>),
    SetMode(acp::SetSessionModeRequest, oneshot::Sender<Result<acp::SetSessionModeResponse, acp::Error>>),
}

impl AcpServerSessionHandle {
    pub fn spawn_local(session_id: acp::SessionId, os: Os, transport: AcpServerConnectionHandle) -> Self {
        let (session_tx, mut session_rx) = mpsc::channel(32);
        
        tokio::task::spawn_local(async move {
            tracing::debug!(actor="session", event="started", session_id=%session_id.0);
            
            let mut conversation_state = ConversationState::new(
                &session_id.0,
                Default::default(), // agents
                HashMap::new(),     // tool_config
                Default::default(), // tool_manager
                None,               // current_model_id
                &os,
                false,              // mcp_enabled
            ).await;
            
            while let Some(method) = session_rx.recv().await {
                match method {
                    ServerSessionMethod::Prompt(args, tx) => {
                        let response = Self::handle_prompt(args, &transport, &os, &mut conversation_state, &mut session_rx).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="session", event="response receiver dropped", method="prompt", session_id=%session_id.0);
                            break;
                        }
                    }
                    ServerSessionMethod::Cancel(args, tx) => {
                        let response = Self::handle_cancel(args).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="session", event="response receiver dropped", method="cancel", session_id=%session_id.0);
                            break;
                        }
                    }
                    ServerSessionMethod::SetMode(args, tx) => {
                        let response = Self::handle_set_mode(args).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="session", event="response receiver dropped", method="set_mode", session_id=%session_id.0);
                            break;
                        }
                    }
                }
            }
            
            tracing::info!("Session actor shutting down for session: {}", session_id.0);
        });
        
        Self { session_tx }
    }

    pub async fn prompt(&self, args: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(ServerSessionMethod::Prompt(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn cancel(&self, args: acp::CancelNotification) -> Result<(), acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(ServerSessionMethod::Cancel(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn set_mode(&self, args: acp::SetSessionModeRequest) -> Result<acp::SetSessionModeResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(ServerSessionMethod::SetMode(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    async fn handle_prompt(
        args: acp::PromptRequest, 
        transport: &AcpServerConnectionHandle,
        os: &Os,
        conversation_state: &mut ConversationState,
        session_rx: &mut mpsc::Receiver<ServerSessionMethod>,
    ) -> Result<acp::PromptResponse, acp::Error> {
        tracing::info!("Processing ACP prompt with {} content blocks", args.prompt.len());
        
        // Convert ACP prompt to string and set it
        let prompt_text = Self::convert_acp_prompt_to_string(args.prompt)?;
        conversation_state.set_next_user_message(prompt_text).await;
        
        // Convert to API format
        let mut stderr = std::io::stderr();
        let api_conversation_state = conversation_state.as_sendable_conversation_state(
            os,
            &mut stderr,
            false,
        ).await.map_err(|e| {
            tracing::error!("Failed to create sendable conversation state: {}", e);
            acp::Error::internal_error()
        })?;
        
        // Send to LLM and stream responses
        let mut stream = SendMessageStream::send_message(
            &os.client,
            api_conversation_state,
            std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            None,
        ).await.map_err(|e| {
            tracing::error!("Failed to send message: {}", e);
            acp::Error::internal_error()
        })?;
        
        // Stream responses via transport with cancellation support
        loop {
            tokio::select! {
                stream_result = stream.recv() => {
                    match stream_result {
                        Some(Ok(event)) => {
                            match &event {
                                ResponseEvent::EndStream { .. } => {
                                    // Stream is complete
                                    break;
                                }
                                _ => {
                                    // Convert and send notification
                                    let notification = Self::convert_response_to_acp_notification(&args.session_id, event)?;
                                    if let Err(e) = transport.session_notification(notification).await {
                                        tracing::error!("Failed to send notification: {}", e);
                                        return Err(acp::Error::internal_error());
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => {
                            tracing::error!("Stream error: {:?}", e);
                            return Err(acp::Error::internal_error());
                        }
                        None => {
                            // Stream ended unexpectedly
                            tracing::warn!("Stream ended without EndStream event");
                            break;
                        }
                    }
                }
                msg = session_rx.recv() => {
                    match msg {
                        Some(ServerSessionMethod::Cancel(_args, _tx)) => {
                            tracing::info!("Prompt cancelled for session: {}", args.session_id.0);
                            // Drop stream to trigger cancellation
                            drop(stream);
                            // Reset conversation state
                            conversation_state.reset_next_user_message();
                            // Send cancelled response
                            return Ok(acp::PromptResponse {
                                stop_reason: acp::StopReason::Cancelled,
                                meta: None,
                            });
                        }
                        Some(other_method) => {
                            // Respond with error for non-cancel messages during prompt processing
                            tracing::warn!("Received non-cancel message during prompt processing: {:?}", other_method);
                            match other_method {
                                ServerSessionMethod::Prompt(_, tx) => {
                                    let _ = tx.send(Err(acp::Error::invalid_params()));
                                }
                                ServerSessionMethod::SetMode(_, tx) => {
                                    let _ = tx.send(Err(acp::Error::invalid_params()));
                                }
                                ServerSessionMethod::Cancel(_, _) => {
                                    // This case is already handled above, shouldn't reach here
                                    unreachable!("Cancel should be handled in the previous match arm");
                                }
                            }
                        }
                        None => {
                            // Session is shutting down
                            tracing::warn!("Session shutting down during prompt processing");
                            return Err(acp::Error::internal_error());
                        }
                    }
                }
            }
        }

        conversation_state.reset_next_user_message();
        
        Ok(acp::PromptResponse {
            stop_reason: acp::StopReason::EndTurn,
            meta: None,
        })
    }
    
    fn convert_acp_prompt_to_string(prompt: Vec<acp::ContentBlock>) -> Result<String, acp::Error> {
        let mut content = String::new();
        
        for block in prompt {
            match block {
                acp::ContentBlock::Text(text_content) => {
                    content.push_str(&text_content.text);
                    content.push('\n');
                }
                _ => {
                    tracing::warn!("Unsupported ACP content block type, skipping");
                }
            }
        }
        
        Ok(content.trim().to_string())
    }
    
    fn convert_response_to_acp_notification(
        session_id: &acp::SessionId, 
        event: ResponseEvent
    ) -> Result<acp::SessionNotification, acp::Error> {
        let content = match event {
            ResponseEvent::AssistantText(text) => {
                acp::ContentBlock::Text(acp::TextContent {
                    text,
                    annotations: None,
                    meta: None,
                })
            }
            ResponseEvent::ToolUseStart { name } => {
                acp::ContentBlock::Text(acp::TextContent {
                    text: format!("[Tool: {}]", name),
                    annotations: None,
                    meta: None,
                })
            }
            ResponseEvent::ToolUse(_tool_use) => {
                // TODO: Convert tool use to proper ACP format
                acp::ContentBlock::Text(acp::TextContent {
                    text: "[Tool execution]".to_string(),
                    annotations: None,
                    meta: None,
                })
            }
            ResponseEvent::EndStream { .. } => {
                // This shouldn't be called for EndStream
                return Err(acp::Error::internal_error());
            }
        };
        
        Ok(acp::SessionNotification {
            session_id: session_id.clone(),
            update: acp::SessionUpdate::AgentMessageChunk { content },
            meta: None,
        })
    }

    async fn handle_cancel(_args: acp::CancelNotification) -> Result<(), acp::Error> {
        // TODO: Cancel ongoing operations
        Ok(())
    }

    async fn handle_set_mode(_args: acp::SetSessionModeRequest) -> Result<acp::SetSessionModeResponse, acp::Error> {
        // TODO: Set session mode
        Err(acp::Error::method_not_found())
    }
}
