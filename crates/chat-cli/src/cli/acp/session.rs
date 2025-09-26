//! ACP Session Actor - Per-session actor that owns conversation state

use agent_client_protocol as acp;
use tokio::sync::{mpsc, oneshot};
use eyre::Result;

use crate::os::Os;
use crate::cli::chat::ConversationState;
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
        _transport: &AcpTransportHandle,
        _os: &Os,
        _conversation_state: &mut Option<ConversationState>,
    ) -> Result<acp::PromptResponse, acp::Error> {
        tracing::info!("Processing ACP prompt with {} content blocks", args.prompt.len());
        
        // TODO: Implement full conversation processing
        // For now, return a simple response to test the actor system
        
        // Extract text from prompt
        let mut prompt_text = String::new();
        for block in args.prompt {
            if let acp::ContentBlock::Text(text_content) = block {
                prompt_text.push_str(&text_content.text);
                prompt_text.push(' ');
            }
        }
        
        tracing::info!("Received prompt: {}", prompt_text.trim());
        
        // Return a simple response
        Ok(acp::PromptResponse {
            stop_reason: acp::StopReason::EndTurn,
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
