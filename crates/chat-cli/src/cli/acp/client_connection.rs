//! ACP Client Actor - Manages client-side ACP connection with actor pattern
//!
//! ## Architecture Overview
//!
//! The client side mirrors the server's actor architecture but adds a dispatch layer
//! for routing incoming notifications to the correct session.
//!
//! **Actor Hierarchy:**
//! - `AcpClientConnectionHandle` - Owns the `ClientSideConnection`, sends prompts to server
//! - `AcpClientDispatchHandle` - Routes incoming notifications to sessions by session_id
//! - `AcpClientSessionHandle` - Per-session handle that sends prompts and receives notifications
//!
//! ## Message Flow
//!
//! When a test sends a prompt and receives the response:
//!
//! ```text
//!  Session Handle    Connection Handle   ClientSideConnection   AcpClientForward   Dispatch Handle
//!  ──────────────    ─────────────────   ────────────────────   ────────────────   ───────────────
//!       │                   │                     │                     │                  │
//!       │  prompt("Hi")     │                     │                     │                  │
//!       ├─────────────────→ │                     │                     │                  │
//!       │   Prompt(req,tx)  │                     │                     │                  │
//!       │   via channel     │                     │                     │                  │
//!       │                   │  client_conn        │                     │                  │
//!       │                   │    .prompt()        │                     │                  │
//!       │                   ├───────────────────→ │                     │                  │
//!       │                   │   async call        │                     │                  │
//!       │                   │                     │                     │                  │
//!       │                   │     (sends JSON-RPC to server)            │                  |
//!       │                   │                     │                     │                  │
//!       │                   │                     │                     │                  │
//!  ╔════════════════════════ NOTIFICATION LOOP (repeats) ══════════════════════════════════╗
//!  ║    │                   │                     │                     │                  │
//!  ║    │                   │     (receives session_notification        |                  |
//!  ║    │                   │              over JSON-RPC)               │                  |
//!  ║    │                   │                     │                     │                  │
//!  ║    │                   │                     │  session_           │                  │
//!  ║    │                   │                     │   notification()    │                  │
//!  ║    │                   │                     ├───────────────────→ │                  │
//!  ║    │                   │                     │   async call        │                  │
//!  ║    │                   │                     │                     │                  │
//!  ║    │                   │                     │                     │  ClientCallback  │
//!  ║    │                   │                     │                     ├────────────────→ │
//!  ║    │                   │                     │                     │  via channel     │
//!  ║    │                   │                     │                     │                  │
//!  ║    │                   │                     │                     │   lookup by      │
//!  ║    │                   │                     │                     │   session_id     │
//!  ║    │                   │                     │                     │                  │
//!  ║    │ ← ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│
//!  ║    │  ClientCallback   │                     │                     │                  │
//!  ║    │  via callback_rx  │                     │                     │                  │
//!  ║    │                   │                     │                     │                  │
//!  ║    │  (accumulate)     │                     │                     │                  │
//!  ║    │                   │                     │                     │                  │
//!  ╚═══════════════════════════════════════════════════════════════════════════════════════╝
//!       │                   │                     │                     │                  │
//!       │                   │  (receives prompt response over JSON-RPC) │                  │
//!       │                   │                     │                     │                  │
//!       │                   │← - ─ ─ ─ ─ ─ ─ ─ ─ ─│                     │                  │
//!       │                   │   PromptResponse    │                     │                  │
//!       │                   │   returned from     │                     │                  │
//!       │                   │   async call        │                     │                  │
//!       │                   │                     │                     │                  │
//!       │← ─ ─ ─ ─ ─ ─ ─ ─ -│                     │                     │                  │
//!       │  PromptResponse   │                     │                     │                  │
//!       │  via oneshot rx   │                     │                     │                  │
//!       │                   │                     │                     │                  │
//!       │  return text      │                     │                     │                  │
//!       ▼                   │                     │                     │                  │
//! ```
//!
//! ## Key Design Decisions
//!
//! **Why ClientCallback instead of raw notifications?**
//! The `ClientCallback` enum bundles each notification with a oneshot response channel.
//! This allows the session to acknowledge receipt and allows the dispatch actor to detect
//! when sessions fail to process notifications.
//!
//! **Why dispatch actor instead of broadcast?**
//! The original design used `broadcast::channel` to fan out all notifications to all sessions,
//! with each session filtering by session_id. This was simple but inefficient (wakes all sessions,
//! risks lag/drops). The dispatch actor uses a HashMap to route notifications directly to the
//! relevant session, providing better performance and clearer error handling.
//!
//! **Session registration:**
//! When a session is created, it registers itself with dispatch by sending its session_id
//! and a `mpsc::Sender<ClientCallback>`. The dispatch actor uses `retain()` on each message
//! to automatically clean up closed sessions without explicit unregistration.

use std::sync::Arc;

use agent_client_protocol::{self as acp, Agent, SessionId};
use eyre::Result;
use futures::{AsyncRead, AsyncWrite};
use serde_json::value::RawValue;
use tokio::sync::{mpsc, oneshot};

use crate::cli::acp::client_dispatch::AcpClientDispatchHandle;
use crate::cli::acp::util::ignore_error;

use super::client_session::AcpClientSessionHandle;

/// Handle to the ACP client actor
#[derive(Clone)]
pub struct AcpClientConnectionHandle {
    client_tx: mpsc::Sender<ClientConnectionMethod>,
}

/// Messages sent to the client actor
#[derive(Debug)]
pub(super) enum ClientConnectionMethod {
    Initialize(
        acp::InitializeRequest,
        oneshot::Sender<Result<acp::InitializeResponse, acp::Error>>,
    ),
    NewSession(
        acp::NewSessionRequest,
        oneshot::Sender<Result<AcpClientSessionHandle, acp::Error>>,
    ),

    // Subtle: the response to a prompt request is always sent to the
    // "dispatch" actor, which will route it to the appropriate session.
    // This ensures that the prompt termination is ordered with respect
    // to the other notifications that are routed to that same session.
    Prompt(acp::PromptRequest),
    
    #[allow(dead_code)] // Will be used when client-side cancellation is implemented
    Cancel(
        acp::CancelNotification,
        oneshot::Sender<Result<(), acp::Error>>,
    ),
}

impl AcpClientConnectionHandle {
    /// Spawn a new ACP client connection that communicates over the given streams
    /// Returns a handle that can be used to interact with the ACP server
    pub async fn spawn_local(
        outgoing_bytes: impl Unpin + AsyncWrite + 'static,
        incoming_bytes: impl Unpin + AsyncRead + 'static,
    ) -> Result<Self> {
        // Channel to send messages to the client connection actor:
        let (client_tx, mut client_rx) = mpsc::channel(32);

        // Channel to receive notifications from the client connection actor:
        let client_dispatch = AcpClientDispatchHandle::spawn_local();

        let handle = Self {
            client_tx: client_tx.clone(),
        };

        // Create an actor to own the connection
        tokio::task::spawn_local(async move {
            // Create client callbacks that forward to the actor
            let callbacks = AcpClientForward::new(client_dispatch.clone());

            // Set up client-side ACP connection
            let (client_conn, client_handle_io) =
                acp::ClientSideConnection::new(callbacks, outgoing_bytes, incoming_bytes, |fut| {
                    tokio::task::spawn_local(fut);
                });

            // Start the client I/O handler
            tokio::task::spawn_local(async move {
                if let Err(e) = client_handle_io.await {
                    tracing::error!("ACP client I/O handler failed: {}", e);
                }
            });

            while let Some(method) = client_rx.recv().await {
                tracing::debug!(actor="client_connection", event="message received", ?method);

                match method {
                    ClientConnectionMethod::Initialize(initialize_request, sender) => {
                        let response = client_conn.initialize(initialize_request).await;
                        tracing::debug!(actor="client_connection", event="sending response", ?response);
                        ignore_error(sender.send(response));
                    },
                    ClientConnectionMethod::NewSession(new_session_request, sender) => {
                        match client_conn.new_session(new_session_request).await {
                            Ok(session_info) => {
                                let result = AcpClientSessionHandle::new(
                                    session_info,
                                    &client_dispatch,
                                    client_tx.clone(),
                                )
                                .await
                                .map_err(|_err| acp::Error::internal_error());
                                tracing::debug!(actor="client_connection", event="sending response", ?result);
                                ignore_error(sender.send(result));
                            },
                            Err(err) => {
                                tracing::debug!(actor="client_connection", event="sending response", ?err);
                                ignore_error(sender.send(Err(err)));
                            },
                        }
                    },
                    ClientConnectionMethod::Prompt(prompt_request) => {
                        let session_id = prompt_request.session_id.clone();
                        let response = client_conn.prompt(prompt_request).await;
                        tracing::debug!(actor="client_connection", event="sending response", ?session_id, ?response);
                        client_dispatch.client_callback(ClientCallback::PromptResponse(session_id, response));
                    },
                    ClientConnectionMethod::Cancel(cancel_notification, sender) => {
                        let response = client_conn.cancel(cancel_notification).await;
                        tracing::debug!(actor="client_connection", event="sending response", ?response);
                        ignore_error(sender.send(response));
                    },
                }
            }
        });

        Ok(handle)
    }

    pub async fn initialize(&self, args: acp::InitializeRequest) -> Result<acp::InitializeResponse> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.client_tx
            .send(ClientConnectionMethod::Initialize(args, tx))
            .await?;
        Ok(rx.await??)
    }

    pub async fn new_session(&self, args: acp::NewSessionRequest) -> Result<AcpClientSessionHandle> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.client_tx
            .send(ClientConnectionMethod::NewSession(args, tx))
            .await?;
        Ok(rx.await??)
    }

    #[allow(dead_code)] // Will be used when client-side cancellation is implemented
    pub async fn cancel(&self, args: acp::CancelNotification) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.client_tx
            .send(ClientConnectionMethod::Cancel(args, tx))
            .await?;
        Ok(rx.await??)
    }
}

/// Forwarding implementation of acp::Client that sends all calls to client actor
struct AcpClientForward {
    client_dispatch: AcpClientDispatchHandle,
}

impl AcpClientForward {
    fn new(client_dispatch: AcpClientDispatchHandle) -> Self {
        Self { client_dispatch }
    }
}

impl acp::Client for AcpClientForward {
    async fn request_permission(
        &self,
        _args: acp::RequestPermissionRequest,
    ) -> Result<acp::RequestPermissionResponse, acp::Error> {
        todo!()
    }

    async fn write_text_file(&self, _args: acp::WriteTextFileRequest) -> Result<acp::WriteTextFileResponse, acp::Error> {
        todo!()
    }

    async fn read_text_file(&self, _args: acp::ReadTextFileRequest) -> Result<acp::ReadTextFileResponse, acp::Error> {
        todo!()
    }

    async fn session_notification(&self, args: acp::SessionNotification) -> Result<(), acp::Error> {
        tracing::debug!(actor="client_connection", event="session_notification", ?args);
        let (tx, rx) = oneshot::channel();
        self.client_dispatch
            .client_callback(ClientCallback::Notification(args, tx));
        let result = rx.await;
        tracing::debug!(actor="client_connection", event="session_notification complete", ?result);
        result.map_err(acp::Error::into_internal_error)?
    }

    async fn create_terminal(
        &self,
        _args: acp::CreateTerminalRequest,
    ) -> Result<acp::CreateTerminalResponse, acp::Error> {
        todo!()
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> Result<acp::TerminalOutputResponse, acp::Error> {
        todo!()
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> Result<acp::ReleaseTerminalResponse, acp::Error> {
        todo!()
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> Result<acp::WaitForTerminalExitResponse, acp::Error> {
        todo!()
    }

    async fn kill_terminal_command(
        &self,
        _args: acp::KillTerminalCommandRequest,
    ) -> Result<acp::KillTerminalCommandResponse, acp::Error> {
        todo!()
    }

    async fn ext_method(&self, _method: Arc<str>, _params: Arc<RawValue>) -> Result<Arc<RawValue>, acp::Error> {
        todo!()
    }

    async fn ext_notification(&self, _method: Arc<str>, _params: Arc<RawValue>) -> Result<(), acp::Error> {
        todo!()
    }
}

#[derive(Debug)]
pub(super) enum ClientCallback {
    Notification(acp::SessionNotification, oneshot::Sender<Result<(), acp::Error>>),
    PromptResponse(acp::SessionId, Result<acp::PromptResponse, acp::Error>),
}

impl ClientCallback {
    pub fn session_id(&self) -> &SessionId {
        match self {
            ClientCallback::Notification(session_notification, _) => &session_notification.session_id,
            ClientCallback::PromptResponse(session_id, _) => session_id,
        }
    }
    pub fn fail(self, error: acp::Error) {
        match self {
            ClientCallback::Notification(_session_notification, sender) => {
                ignore_error(sender.send(Err(error)));
            },
            ClientCallback::PromptResponse(_, _prompt_response) => (),
        }
    }
}

