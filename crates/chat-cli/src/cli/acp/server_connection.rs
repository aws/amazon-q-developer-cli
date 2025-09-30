//! ACP Transport Actor - Owns the ACP connection and handles notifications

use std::sync::Arc;

use agent_client_protocol::{self as acp, Client};
use eyre::Result;
use futures::{AsyncRead, AsyncWrite};
use serde_json::value::RawValue;
use tokio::sync::{mpsc, oneshot};

use crate::{
    cli::acp::{AcpServerHandle},
    os::Os,
};

/// Handle to the "server connection" actor, which owns the actual connection to the underlying transport.
#[derive(Clone)]
pub struct AcpServerConnectionHandle {
    transport_tx: mpsc::Sender<TransportMethod>,
}

/// Messages sent to the transport actor
#[derive(Debug)]
enum TransportMethod {
    SessionNotification(acp::SessionNotification, oneshot::Sender<Result<(), acp::Error>>),
}

impl AcpServerConnectionHandle {
    /// Execute a new ACP "server connection" actor that
    /// accepts server requests over `incoming_bytes`
    /// and responds over `outgoing_bytes`.
    pub async fn execute(
        agent_name: String,
        os: &Os,
        outgoing_bytes: impl Unpin + AsyncWrite,
        incoming_bytes: impl Unpin + AsyncRead,
    ) -> eyre::Result<()> {
        let (transport_tx, mut transport_rx) = mpsc::channel(32);

        // Create the handle to the (yet to be launched) transport actor.
        let transport_handle = Self { transport_tx };
        
        // Spawn the server actor with transport handle
        let server_handle = AcpServerHandle::spawn(agent_name, os.clone(), transport_handle.clone());

        // Create connection to bytes
        let (connection, handle_io) = agent_client_protocol::AgentSideConnection::new(
            AcpAgentForward::new(server_handle),
            outgoing_bytes,
            incoming_bytes,
            |fut| {
                tokio::task::spawn_local(fut);
            },
        );

        // Launch the "transport actor", which owns the connection.
        tokio::task::spawn_local(async move {
            tracing::debug!(actor="server_connection", event="started");

            while let Some(method) = transport_rx.recv().await {
                tracing::debug!(actor="server_connection", event="message received", ?method);
                match method {
                    TransportMethod::SessionNotification(notification, tx) => {
                        let result = connection.session_notification(notification).await;
                        tracing::debug!(actor="server_connection", event="notification delivered");
                        if tx.send(result).is_err() {
                            tracing::debug!(actor="server_connection", event="response receiver dropped");
                        }
                    },
                }
            }

            tracing::info!("Transport actor shutting down");
        });

        match handle_io.await {
            Ok(()) => Ok(()),
            Err(err) => eyre::bail!("{err}"),
        }
    }

    pub async fn session_notification(&self, notification: acp::SessionNotification) -> Result<()> {
        tracing::debug!(actor="server_connection", event="session_notification", ?notification);
        let (tx, rx) = oneshot::channel();
        self.transport_tx
            .send(TransportMethod::SessionNotification(notification, tx))
            .await
            .map_err(|_send_err| eyre::eyre!("Transport actor has shut down"))?;
        let acp_result = rx.await.map_err(|_recv_err| eyre::eyre!("Transport actor dropped response"))?;
        acp_result.map_err(|e| eyre::eyre!("ACP error: {:?}", e))?;
        tracing::debug!(actor="server_connection", event="session_notification succeeded");
        Ok(())
    }
}

/// Forwarding implementation of acp::Agent that sends all calls to server actor
struct AcpAgentForward {
    server_handle: AcpServerHandle,
}

impl AcpAgentForward {
    pub fn new(server_handle: AcpServerHandle) -> Self {
        Self { server_handle }
    }
}

impl acp::Agent for AcpAgentForward {
    async fn initialize(&self, arguments: acp::InitializeRequest) -> Result<acp::InitializeResponse, acp::Error> {
        self.server_handle.initialize(arguments).await
    }

    async fn authenticate(&self, arguments: acp::AuthenticateRequest) -> Result<acp::AuthenticateResponse, acp::Error> {
        self.server_handle.authenticate(arguments).await
    }

    async fn new_session(&self, arguments: acp::NewSessionRequest) -> Result<acp::NewSessionResponse, acp::Error> {
        self.server_handle.new_session(arguments).await
    }

    async fn load_session(&self, arguments: acp::LoadSessionRequest) -> Result<acp::LoadSessionResponse, acp::Error> {
        self.server_handle.load_session(arguments).await
    }

    async fn prompt(&self, arguments: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        tracing::debug!(actor="server_connection", event="prompt", ?arguments);
        let result = self.server_handle.prompt(arguments).await;
        tracing::debug!(actor="server_connection", event="prompt complete", ?result);
        result
    }

    async fn cancel(&self, args: acp::CancelNotification) -> Result<(), acp::Error> {
        self.server_handle.cancel(args).await
    }

    async fn set_session_mode(
        &self,
        args: acp::SetSessionModeRequest,
    ) -> Result<acp::SetSessionModeResponse, acp::Error> {
        self.server_handle.set_session_mode(args).await
    }

    async fn ext_method(&self, method: Arc<str>, params: Arc<RawValue>) -> Result<Arc<RawValue>, acp::Error> {
        self.server_handle.ext_method(method, params).await
    }

    async fn ext_notification(&self, method: Arc<str>, params: Arc<RawValue>) -> Result<(), acp::Error> {
        self.server_handle.ext_notification(method, params).await
    }
}
