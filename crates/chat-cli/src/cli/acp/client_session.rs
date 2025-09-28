//! ACP Client Session Handle - Manages individual test sessions


use agent_client_protocol::{self as acp, PromptResponse, TextContent};
use eyre::Result;
use tokio::sync::{mpsc, oneshot};

use crate::cli::acp::{
    client_connection::{ClientCallback, ClientConnectionMethod},
    client_dispatch::AcpClientDispatchHandle,
    util::ignore_error,
};

/// Handle for a specific test session
#[derive(Debug)]
pub struct AcpClientSessionHandle {
    session_info: acp::NewSessionResponse,
    callback_rx: mpsc::Receiver<ClientCallback>,
    client_tx: mpsc::Sender<ClientConnectionMethod>,
}

impl AcpClientSessionHandle {
    pub(super) async fn new(
        session_info: acp::NewSessionResponse,
        client_dispatch: &AcpClientDispatchHandle,
        client_tx: mpsc::Sender<ClientConnectionMethod>,
    ) -> eyre::Result<Self> {
        let (callback_tx, callback_rx) = mpsc::channel(32);
        client_dispatch
            .register_session(&session_info.session_id, callback_tx)?;
        Ok(Self {
            session_info,
            callback_rx,
            client_tx,
        })
    }

    /// Send a message to the agent and read the complete response
    pub async fn prompt(&mut self, message: impl IntoPrompt) -> Result<String> {
        // Construct the prompt
        let prompt = acp::PromptRequest {
            session_id: self.session_info.session_id.clone(),
            prompt: message.into_prompt(),
            meta: None,
        };

        tracing::debug!(actor="client_session", event="prompt received", ?prompt);

        // Send the prompt over to the client connection. It will send the "stop reason" over
        // via the dispatch actor once it is done.
        self.client_tx.send(ClientConnectionMethod::Prompt(prompt)).await?;

        // Read notifications until we get the prompt response, then we can return.
        let mut response_text = String::new();
        while let Some(client_callback) = self.callback_rx.recv().await {
            match client_callback {
                ClientCallback::Notification(notification, tx) => {
                    self.handle_notification(notification, tx, &mut response_text)
                },
                ClientCallback::PromptResponse(session_id, response) => {
                    assert_eq!(self.session_info.session_id, session_id);

                    // Convert abnormal stop-reasons into errors
                    let PromptResponse { stop_reason, meta: _ } = response?;
                    match stop_reason {
                        acp::StopReason::EndTurn => return Ok(response_text),
                        acp::StopReason::MaxTokens => eyre::bail!("max tokens exceeded"),
                        acp::StopReason::MaxTurnRequests => eyre::bail!("max turn requests exceeded"),
                        acp::StopReason::Refusal => eyre::bail!("refused"),
                        acp::StopReason::Cancelled => eyre::bail!("canceled"),
                    }
                },
            }
        }

        eyre::bail!("callback_rx closed before we received stop reason");
    }

    fn handle_notification(
        &mut self,
        notification: acp::SessionNotification,
        tx: oneshot::Sender<Result<(), acp::Error>>,
        response_text: &mut String,
    ) {
        assert_eq!(self.session_info.session_id, notification.session_id);
        match notification.update {
            acp::SessionUpdate::AgentMessageChunk { content } => {
                ignore_error(tx.send(self.push_content(content, response_text)));
                return;
            },
            acp::SessionUpdate::AgentThoughtChunk { content } => {
                response_text.push_str("\n\n<thinking>");
                let result = self.push_content(content, response_text);
                response_text.push_str("</thinking>\n\n");
                ignore_error(tx.send(result));
                return;
            },
            acp::SessionUpdate::UserMessageChunk { content } => {
                response_text.push_str("\n\n<user>");
                if let acp::ContentBlock::Text(text_content) = content {
                    response_text.push_str(&text_content.text);
                }
                response_text.push_str("</user>\n\n");
                ignore_error(tx.send(Ok(())));
                return;
            },
            acp::SessionUpdate::ToolCall(_)
            | acp::SessionUpdate::ToolCallUpdate(_)
            | acp::SessionUpdate::Plan(_)
            | acp::SessionUpdate::AvailableCommandsUpdate { .. }
            | acp::SessionUpdate::CurrentModeUpdate { .. } => {
                ignore_error(tx.send(Err(acp::Error::internal_error())));
                return;
            },
        }
    }

    fn push_content(&mut self, content: acp::ContentBlock, response_text: &mut String) -> Result<(), acp::Error> {
        match content {
            acp::ContentBlock::Text(text_content) => {
                response_text.push_str(&text_content.text);
                Ok(())
            },
            acp::ContentBlock::Image(_)
            | acp::ContentBlock::Audio(_)
            | acp::ContentBlock::ResourceLink(_)
            | acp::ContentBlock::Resource(_) => Err(acp::Error::internal_error()),
        }
    }
}

pub trait IntoPrompt {
    fn into_prompt(self) -> Vec<acp::ContentBlock>;
}

impl IntoPrompt for String {
    fn into_prompt(self) -> Vec<acp::ContentBlock> {
        vec![acp::ContentBlock::Text(TextContent {
            annotations: None,
            text: self,
            meta: None,
        })]
    }
}

impl IntoPrompt for &str {
    fn into_prompt(self) -> Vec<acp::ContentBlock> {
        self.to_string().into_prompt()
    }
}
