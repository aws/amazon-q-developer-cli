//! ACP Session Actor - Per-session actor that owns conversation state

use agent_client_protocol as acp;
use tokio::sync::{mpsc, oneshot};
use eyre::Result;
use std::collections::HashMap;

use crate::os::Os;
use crate::cli::chat::{ConversationState, SendMessageStream, ResponseEvent};
use super::transport::AcpTransportHandle;

/// Convert channel errors to ACP errors
fn channel_to_acp_error<E>(_err: E) -> acp::Error {
    acp::Error::internal_error()
}

/// Handle to a session actor
#[derive(Clone)]
pub struct AcpSessionHandle {
    session_tx: mpsc::Sender<SessionMethod>,
}

/// Messages sent to session actors
enum SessionMethod {
    Prompt(acp::PromptRequest, oneshot::Sender<Result<acp::PromptResponse, acp::Error>>),
    Cancel(acp::CancelNotification, oneshot::Sender<Result<(), acp::Error>>),
    SetMode(acp::SetSessionModeRequest, oneshot::Sender<Result<acp::SetSessionModeResponse, acp::Error>>),
}

impl AcpSessionHandle {
    pub fn spawn(session_id: acp::SessionId, os: Os, transport: AcpTransportHandle) -> Self {
        let (session_tx, mut session_rx) = mpsc::channel(32);
        
        tokio::task::spawn_local(async move {
            tracing::debug!("Session actor started for session: {}", session_id.0);
            
            // TODO: Create ConversationState for this session
            // For now, we'll create it when we get the first prompt
            let mut conversation_state: Option<ConversationState> = None;
            
            while let Some(method) = session_rx.recv().await {
                match method {
                    SessionMethod::Prompt(args, tx) => {
                        let response = Self::handle_prompt(args, &transport, &os, &mut conversation_state).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("Prompt response receiver dropped, exiting session actor: {}", session_id.0);
                            break;
                        }
                    }
                    SessionMethod::Cancel(args, tx) => {
                        let response = Self::handle_cancel(args).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("Cancel response receiver dropped, exiting session actor: {}", session_id.0);
                            break;
                        }
                    }
                    SessionMethod::SetMode(args, tx) => {
                        let response = Self::handle_set_mode(args).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("SetMode response receiver dropped, exiting session actor: {}", session_id.0);
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
        self.session_tx.send(SessionMethod::Prompt(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn cancel(&self, args: acp::CancelNotification) -> Result<(), acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(SessionMethod::Cancel(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn set_mode(&self, args: acp::SetSessionModeRequest) -> Result<acp::SetSessionModeResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.session_tx.send(SessionMethod::SetMode(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    async fn handle_prompt(
        args: acp::PromptRequest, 
        transport: &AcpTransportHandle,
        os: &Os,
        conversation_state: &mut Option<ConversationState>,
    ) -> Result<acp::PromptResponse, acp::Error> {
        tracing::info!("Processing ACP prompt with {} content blocks", args.prompt.len());
        
        // Create ConversationState if this is the first prompt
        if conversation_state.is_none() {
            tracing::debug!("Creating new ConversationState for session");
            
            // Use simplified creation for now - TODO: get proper config from ACP session
            let conv_state = ConversationState::new(
                &args.session_id.0,
                Default::default(), // agents
                HashMap::new(),     // tool_config
                Default::default(), // tool_manager
                None,               // current_model_id
                os,
                false,              // mcp_enabled
            ).await;
            
            *conversation_state = Some(conv_state);
        }
        
        let conv_state = conversation_state.as_mut().unwrap();
        
        // Convert ACP prompt to string and set it
        let prompt_text = Self::convert_acp_prompt_to_string(args.prompt)?;
        conv_state.set_next_user_message(prompt_text).await;
        
        // Convert to API format
        let mut stderr = std::io::stderr();
        let api_conversation_state = conv_state.as_sendable_conversation_state(
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
        
        // Stream responses via transport
        loop {
            match stream.recv().await {
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
