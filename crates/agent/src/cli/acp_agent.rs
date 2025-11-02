//! Basic ACP Agent that echoes back prompt content.
//!
//! Usage (from workspace root):
//! ```bash
//! cargo run -p agent -- acp
//! ```
//!
//! The agent communicates over stdin/stdout and will echo back any prompt content received.

use std::process::ExitCode;
use std::sync::Arc;
use agent_client_protocol as acp;
use eyre::Result;
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use agent::{Agent, AgentHandle};
use agent::api_client::ApiClient;
use agent::mcp::McpManager;
use agent::rts::{RtsModel, RtsModelState};
use agent::types::AgentSnapshot;
use agent::protocol::{ContentChunk, SendPromptArgs, AgentEvent, UpdateEvent};

/// Session that processes user requests in an event loop
struct AcpSession {
    session_id: acp::SessionId,
    request_rx: mpsc::UnboundedReceiver<(acp::PromptRequest, oneshot::Sender<()>)>,
    conn: acp::AgentSideConnection,
    agent: AgentHandle,
}

impl AcpSession {
    async fn new(
        session_id: acp::SessionId,
        request_rx: mpsc::UnboundedReceiver<(acp::PromptRequest, oneshot::Sender<()>)>,
        conn: acp::AgentSideConnection,
    ) -> Result<Self> {
        // Create agent snapshot
        let snapshot = AgentSnapshot::default();

        // Create RTS model
        let rts_state = RtsModelState::new();
        let model = Arc::new(RtsModel::new(
            ApiClient::new().await?,
            rts_state.conversation_id,
            rts_state.model_id,
        ));

        // Spawn agent
        let agent = Agent::new(snapshot, model, McpManager::new().spawn()).await?.spawn();

        Ok(Self {
            session_id,
            request_rx,
            conn,
            agent,
        })
    }

    /// Event loop that processes user requests and agent events
    /// - Receives user requests from request_rx and processes them
    /// - Receives agent events and sends updates to ACP client
    async fn run(&mut self) -> Result<(), acp::Error> {
        loop {
            tokio::select! {
                request = self.request_rx.recv() => {
                    match request {
                        Some((request, done_tx)) => {
                            self.process_request(request).await?;
                            // Signal completion since send_prompt blocks until conversation is done
                            done_tx.send(()).ok();
                        }
                        None => break, // Channel closed
                    }
                }
                agent_event = self.agent.recv() => {
                    match agent_event {
                        Ok(AgentEvent::Update(update_event)) => {
                            self.handle_agent_update(update_event).await?;
                        }
                        Ok(_) => {
                            // Handle other agent events if needed
                        }
                        Err(_) => break, // Agent channel closed
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle agent update events and send to ACP client
    async fn handle_agent_update(&self, update_event: UpdateEvent) -> Result<(), acp::Error> {
        let content = match update_event {
            UpdateEvent::AgentContent(ContentChunk::Text(text)) => {
                acp::ContentBlock::Text(acp::TextContent {
                    text,
                    annotations: None,
                })
            }
            _ => return Ok(()), // Skip non-text updates for now
        };

        let session_notification = acp::SessionNotification {
            session_id: self.session_id.clone(),
            update: acp::SessionUpdate::AgentMessageChunk { content },
        };

        acp::Client::session_notification(&self.conn, session_notification)
            .await
            .map_err(|_| acp::Error::internal_error())?;

        Ok(())
    }

    /// Process a user request (prompt or slash command)
    async fn process_request(&self, request: acp::PromptRequest) -> Result<(), acp::Error> {
        // Convert ACP prompt to agent format
        let content: Vec<ContentChunk> = request.prompt
            .into_iter()
            .filter_map(|block| match block {
                acp::ContentBlock::Text(text_content) => Some(ContentChunk::Text(text_content.text)),
                _ => None, // Skip non-text content for now
            })
            .collect();

        // Send prompt to agent (this blocks until conversation is complete)
        self.agent
            .send_prompt(SendPromptArgs {
                content,
                should_continue_turn: None,
            })
            .await
            .map_err(|_| acp::Error::internal_error())?;

        Ok(())
    }
}

/// ACP Agent that forwards requests to the session
struct AcpAgent {
    request_tx: mpsc::UnboundedSender<(acp::PromptRequest, oneshot::Sender<()>)>,
}

impl AcpAgent {
    fn new(request_tx: mpsc::UnboundedSender<(acp::PromptRequest, oneshot::Sender<()>)>) -> Self {
        Self { request_tx }
    }
}

impl acp::Agent for AcpAgent {
    async fn initialize(&self, _args: acp::InitializeRequest) -> Result<acp::InitializeResponse, acp::Error> {
        Ok(acp::InitializeResponse {
            protocol_version: acp::V1,
            agent_capabilities: acp::AgentCapabilities::default(),
            auth_methods: Vec::new(),
        })
    }

    async fn authenticate(&self, _args: acp::AuthenticateRequest) -> Result<(), acp::Error> {
        Ok(())
    }

    async fn new_session(&self, _args: acp::NewSessionRequest) -> Result<acp::NewSessionResponse, acp::Error> {
        Ok(acp::NewSessionResponse {
            session_id: acp::SessionId("42".into()),
        })
    }

    async fn load_session(&self, _args: acp::LoadSessionRequest) -> Result<(), acp::Error> {
        Ok(())
    }

    async fn prompt(&self, args: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send((args, tx))
            .map_err(|_| acp::Error::internal_error())?;
        rx.await.map_err(|_| acp::Error::internal_error())?;
        
        Ok(acp::PromptResponse {
            stop_reason: acp::StopReason::EndTurn,
        })
    }

    async fn cancel(&self, _args: acp::CancelNotification) -> Result<(), acp::Error> {
        Ok(())
    }
}

pub async fn execute() -> Result<ExitCode> {
    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    // request from ACP Client
    let (request_tx, request_rx) = tokio::sync::mpsc::unbounded_channel();
    let agent = AcpAgent::new(request_tx);

    let local_set = tokio::task::LocalSet::new();
    local_set.run_until(async move {
        let (conn, handle_io) = acp::AgentSideConnection::new(
            agent,
            outgoing,
            incoming,
            |fut| { tokio::task::spawn_local(fut); }
        );

        // Session event loop
        let mut session = AcpSession::new(
            acp::SessionId("42".into()),
            request_rx,
            conn,
        ).await?;
        tokio::task::spawn_local(async move {
            if let Err(e) = session.run().await {
                eprintln!("Session error: {}", e);
            }
        });

        handle_io.await.map_err(|e| eyre::eyre!("IO error: {}", e))
    }).await?;

    Ok(ExitCode::SUCCESS)
}

