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

struct AcpAgent {
    session_update_tx: mpsc::UnboundedSender<(acp::SessionNotification, oneshot::Sender<()>)>,
}

impl AcpAgent {
    fn new(session_update_tx: mpsc::UnboundedSender<(acp::SessionNotification, oneshot::Sender<()>)>) -> Self {
        Self {
            session_update_tx,
        }
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
        // Echo back the prompt content
        for content in args.prompt {
            let (tx, rx) = oneshot::channel();
            self.session_update_tx
                .send((
                    acp::SessionNotification {
                        session_id: args.session_id.clone(),
                        update: acp::SessionUpdate::AgentMessageChunk { content },
                    },
                    tx,
                ))
                .map_err(|_| acp::Error::internal_error())?;
            rx.await.map_err(|_| acp::Error::internal_error())?;
        }
        
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

    let local_set = tokio::task::LocalSet::new();
    local_set.run_until(async move {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (conn, handle_io) = acp::AgentSideConnection::new(
            AcpAgent::new(tx),
            outgoing,
            incoming,
            |fut| { tokio::task::spawn_local(fut); }
        );

        tokio::task::spawn_local(async move {
            while let Some((session_notification, tx)) = rx.recv().await {
                if acp::Client::session_notification(&conn, session_notification).await.is_err() {
                    break;
                }
                tx.send(()).ok();
            }
        });

        handle_io.await.map_err(|e| eyre::eyre!("IO error: {}", e))
    }).await?;

    Ok(ExitCode::SUCCESS)
}

