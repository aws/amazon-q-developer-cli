//! ACP Client Dispatch Actor - Routes session notifications to the correct session

use std::collections::HashMap;

use agent_client_protocol::{self as acp};
use tokio::sync::mpsc::{self, error::SendError};

use crate::cli::acp::client_connection::ClientCallback;

#[derive(Clone)]
pub struct AcpClientDispatchHandle {
    /// Send a message to the dispatch actor.
    /// 
    /// This is intentionally *unbounded* so that message can
    /// be sent without blocking and so that there is a guaranteed
    /// total ordering. This is important because sometimes we have
    /// incoming notifications that have be received in order;
    /// if we were forcing senders to queue, then even if
    /// the first attempts to send were in order, later attempts
    /// might not be.
    /// 
    /// Note: This is related to how the ACP library works internally.
    /// It spawns out "threads" as data arrives to maintain responsiveness,
    /// which means that the ordering of callbacks is not guaranteed;
    /// I *think* this is inherently buggy, to be honest, but in practice
    /// it works ok if they don't block, from *what I can tell*.
    /// I am in conversation with the ACP team to discuss if there is a
    /// flaw in my analysis and, if so, how to fix it. However,
    /// the refactoring I would suggest would result in callbacks
    /// being *sync* not *async*, which again implies you would
    /// want to be able to enqueue without blocking. --nikomatsais
    dispatch_tx: mpsc::UnboundedSender<ClientDispatchMethod>,
}

#[derive(Debug)]
pub(super) enum ClientDispatchMethod {
    RegisterSession(acp::SessionId, mpsc::Sender<ClientCallback>),
    ClientCallback(ClientCallback),
}

impl AcpClientDispatchHandle {
    pub fn spawn_local() -> Self {
        let (dispatch_tx, mut dispatch_rx) = mpsc::unbounded_channel();

        tokio::task::spawn_local(async move {
            let mut sessions: HashMap<acp::SessionId, mpsc::Sender<ClientCallback>> = HashMap::new();

            while let Some(method) = dispatch_rx.recv().await {
                sessions.retain(|_, tx| !tx.is_closed());

                tracing::debug!(actor="client_dispatch", event="message received", ?method);
                match method {
                    ClientDispatchMethod::RegisterSession(session_id, tx) => {
                        tracing::debug!(actor="client_dispatch", event="registering session", session_id=%session_id.0);
                        sessions.insert(session_id, tx);
                    },
                    ClientDispatchMethod::ClientCallback(callback) => {
                        let session_id = callback.session_id();
                        if let Some(session_tx) = sessions.get(session_id) {
                            match session_tx
                                .send(callback)
                                .await
                            {
                                Ok(()) => (),
                                Err(SendError(callback)) => callback.fail(acp::Error::internal_error()),
                            }
                        } else {
                            tracing::debug!(actor="client_dispatch", event="session not found", ?session_id);
                            callback.fail(acp::Error::internal_error());
                        }
                    }
                }
            }

            tracing::info!("Client dispatch actor shutting down");
        });

        Self { dispatch_tx }
    }

    pub fn register_session(
        &self,
        session_id: &acp::SessionId,
        callback_tx: mpsc::Sender<ClientCallback>,
    ) -> eyre::Result<()> {
        self.dispatch_tx
            .send(ClientDispatchMethod::RegisterSession(session_id.clone(), callback_tx))
            .map_err(|_send_err| eyre::eyre!("Client dispatch actor has shut down"))
    }

    /// Route a callback to the correct place.
    pub fn client_callback(&self, callback: ClientCallback) {
        tracing::debug!(actor="client_dispatch", event="client callback", ?callback);
        match self.dispatch_tx.send(ClientDispatchMethod::ClientCallback(callback)) {
            Ok(()) => (),
            Err(SendError(ClientDispatchMethod::ClientCallback(callback))) => {
                callback.fail(acp::Error::internal_error())
            },
            Err(SendError(_)) => unreachable!(),
        }
    }
}
