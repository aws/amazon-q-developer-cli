//! ACP Transport Actor - Owns the ACP connection and handles notifications

use agent_client_protocol::{self as acp, Client};
use tokio::sync::{mpsc, oneshot};
use eyre::Result;

/// Handle to the transport actor
#[derive(Clone)]
pub struct AcpTransportHandle {
    transport_tx: mpsc::Sender<TransportMethod>,
}

/// Messages sent to the transport actor
enum TransportMethod {
    SetConnection(acp::AgentSideConnection, oneshot::Sender<()>),
    SessionNotification(acp::SessionNotification, oneshot::Sender<Result<(), acp::Error>>),
}

impl AcpTransportHandle {
    pub fn new() -> Self {
        let (transport_tx, mut transport_rx) = mpsc::channel(32);
        
        tokio::task::spawn_local(async move {
            tracing::debug!("Transport actor started, waiting for connection");
            
            let mut connection: Option<acp::AgentSideConnection> = None;
            
            while let Some(method) = transport_rx.recv().await {
                match method {
                    TransportMethod::SetConnection(conn, tx) => {
                        tracing::debug!("Transport actor received connection");
                        connection = Some(conn);
                        let _ = tx.send(());
                    }
                    TransportMethod::SessionNotification(notification, tx) => {
                        let result = if let Some(ref mut conn) = connection {
                            conn.session_notification(notification).await
                        } else {
                            tracing::error!("Attempted to send notification before connection was set");
                            Err(acp::Error::internal_error())
                        };
                        if tx.send(result).is_err() {
                            tracing::debug!("Session notification response receiver dropped");
                        }
                    }
                }
            }
            
            tracing::info!("Transport actor shutting down");
        });
        
        Self { transport_tx }
    }

    pub async fn set_connection(&self, connection: acp::AgentSideConnection) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.transport_tx.send(TransportMethod::SetConnection(connection, tx)).await
            .map_err(|_| eyre::eyre!("Transport actor has shut down"))?;
        rx.await.map_err(|_| eyre::eyre!("Transport actor dropped response"))?;
        Ok(())
    }

    pub async fn session_notification(&self, notification: acp::SessionNotification) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.transport_tx.send(TransportMethod::SessionNotification(notification, tx)).await
            .map_err(|_| eyre::eyre!("Transport actor has shut down"))?;
        let acp_result = rx.await.map_err(|_| eyre::eyre!("Transport actor dropped response"))?;
        acp_result.map_err(|e| eyre::eyre!("ACP error: {:?}", e))?;
        Ok(())
    }
}
