//! Basic ACP Agent that echoes back prompt content.
//!
//! Usage (from workspace root):
//! ```bash
//! cargo run -p agent -- acp
//! ```
//!
//! The agent communicates over stdin/stdout and will echo back any prompt content received.

use std::process::ExitCode;
use agent_client_protocol as acp;
use eyre::Result;
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Session that processes user requests in an event loop
struct AcpSession {
    session_id: acp::SessionId,
    request_rx: mpsc::UnboundedReceiver<(acp::PromptRequest, oneshot::Sender<()>)>,
    session_update_tx: mpsc::UnboundedSender<(acp::SessionNotification, oneshot::Sender<()>)>,
    session_update_rx: mpsc::UnboundedReceiver<(acp::SessionNotification, oneshot::Sender<()>)>,
    conn: acp::AgentSideConnection,
}

impl AcpSession {
    fn new(
        session_id: acp::SessionId,
        request_rx: mpsc::UnboundedReceiver<(acp::PromptRequest, oneshot::Sender<()>)>,
        session_update_tx: mpsc::UnboundedSender<(acp::SessionNotification, oneshot::Sender<()>)>,
        session_update_rx: mpsc::UnboundedReceiver<(acp::SessionNotification, oneshot::Sender<()>)>,
        conn: acp::AgentSideConnection,
    ) -> Self {
        Self {
            session_id,
            request_rx,
            session_update_tx,
            session_update_rx,
            conn,
        }
    }

    /// Event loop that processes user requests and output events
    /// - Receives user requests from request_rx and processes them
    /// - Receives session updates from session_update_rx and sends notifications to ACP client
    async fn run(&mut self) -> Result<(), acp::Error> {
        loop {
            tokio::select! {
                request = self.request_rx.recv() => {
                    match request {
                        Some((request, done_tx)) => {
                            self.process_request(request).await?;
                            // TODO: Only call done_tx.send when the conversation is truly done,
                            // not immediately after processing the request
                            done_tx.send(()).ok(); // Signal completion
                        }
                        None => break, // Channel closed
                    }
                }
                update = self.session_update_rx.recv() => {
                    match update {
                        Some((session_notification, tx)) => {
                            if acp::Client::session_notification(&self.conn, session_notification).await.is_err() {
                                break;
                            }
                            tx.send(()).ok();
                        }
                        None => break, // Channel closed
                    }
                }
            }
        }
        Ok(())
    }

    /// Process a user request (prompt or slash command)
    async fn process_request(&self, request: acp::PromptRequest) -> Result<(), acp::Error> {
        // Echo back the request content
        for content in request.prompt {
            self.session_update_tx
                .send((
                    acp::SessionNotification {
                        session_id: self.session_id.clone(),
                        update: acp::SessionUpdate::AgentMessageChunk { content }
                    },
                    oneshot::channel().0,
                ))
                .map_err(|_| acp::Error::internal_error())?;
        }
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

    let (session_update_tx, session_update_rx) = tokio::sync::mpsc::unbounded_channel::<(acp::SessionNotification, oneshot::Sender<()>)>();
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
            session_update_tx,
            session_update_rx,
            conn,
        );
        tokio::task::spawn_local(async move {
            if let Err(e) = session.run().await {
                eprintln!("Session error: {}", e);
            }
        });

        handle_io.await.map_err(|e| eyre::eyre!("IO error: {}", e))
    }).await?;

    Ok(ExitCode::SUCCESS)
}

