//! ACP Server Actor - Top-level coordinator that manages sessions

use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol as acp;
use serde_json::value::RawValue;
use tokio::sync::{mpsc, oneshot};

use crate::os::Os;
use super::{server_session::AcpServerSessionHandle, server_connection::AcpServerConnectionHandle};

/// Convert channel errors to ACP errors
fn channel_to_acp_error<E>(_err: E) -> acp::Error {
    acp::Error::internal_error()
}

/// Handle to the ACP "server" actor.
/// 
/// This actor receives messages modeled after the ACP server methods and processes them.
/// It follows the typical oneshot-based RPC method but also takes a 
/// [`ACPServerConnectionHandle`][] that it can to send notifications back over the transport.
#[derive(Clone)]
pub struct AcpServerHandle {
    server_tx: mpsc::Sender<ServerMethod>,
}

/// Messages sent to the server actor
/// 
/// Each variant contains:
/// - Request parameters (the input)
/// - oneshot::Sender (the "return address" where the actor sends the response back)
enum ServerMethod {
    Initialize(acp::InitializeRequest, oneshot::Sender<Result<acp::InitializeResponse, acp::Error>>),
    Authenticate(acp::AuthenticateRequest, oneshot::Sender<Result<acp::AuthenticateResponse, acp::Error>>),
    NewSession(acp::NewSessionRequest, oneshot::Sender<Result<acp::NewSessionResponse, acp::Error>>),
    LoadSession(acp::LoadSessionRequest, oneshot::Sender<Result<acp::LoadSessionResponse, acp::Error>>),
    SetSessionMode(acp::SetSessionModeRequest, oneshot::Sender<Result<acp::SetSessionModeResponse, acp::Error>>),
    Prompt(acp::PromptRequest, oneshot::Sender<Result<acp::PromptResponse, acp::Error>>),
    Cancel(acp::CancelNotification, oneshot::Sender<Result<(), acp::Error>>),
    ExtMethod(Arc<str>, Arc<RawValue>, oneshot::Sender<Result<Arc<RawValue>, acp::Error>>),
    ExtNotification(Arc<str>, Arc<RawValue>, oneshot::Sender<Result<(), acp::Error>>),
}

impl AcpServerHandle {
    pub fn spawn(agent_name: String, os: Os, transport: AcpServerConnectionHandle) -> Self {
        let (server_tx, mut server_rx) = mpsc::channel(32);
        
        tokio::task::spawn_local(async move {
            let mut sessions: HashMap<String, AcpServerSessionHandle> = HashMap::new();
            
            while let Some(method) = server_rx.recv().await {
                match method {
                    ServerMethod::Initialize(args, tx) => {
                        let response = Self::handle_initialize(args).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="initialize");
                            break;
                        }
                    }
                    ServerMethod::Authenticate(args, tx) => {
                        let response = Self::handle_authenticate(args).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="authenticate");
                            break;
                        }
                    }
                    ServerMethod::NewSession(args, tx) => {
                        let response = Self::handle_new_session(args, &agent_name, &os, &mut sessions, &transport).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="new_session");
                            break;
                        }
                    }
                    ServerMethod::LoadSession(args, tx) => {
                        let response = Self::handle_load_session(args, &sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="load_session");
                            break;
                        }
                    }
                    ServerMethod::SetSessionMode(args, tx) => {
                        let response = Self::handle_set_session_mode(args, &sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="set_session_mode");
                            break;
                        }
                    }
                    ServerMethod::Prompt(args, tx) => {
                        let response = Self::handle_prompt(args, &sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="prompt");
                            break;
                        }
                    }
                    ServerMethod::Cancel(args, tx) => {
                        let response = Self::handle_cancel(args, &sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="cancel");
                            break;
                        }
                    }
                    ServerMethod::ExtMethod(method, params, tx) => {
                        let response = Self::handle_ext_method(method, params).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="ext_method");
                            break;
                        }
                    }
                    ServerMethod::ExtNotification(method, params, tx) => {
                        let response = Self::handle_ext_notification(method, params).await;
                        if tx.send(response).is_err() {
                            tracing::debug!(actor="server", event="response receiver dropped", method="ext_notification");
                            break;
                        }
                    }
                }
            }
            
            tracing::info!("Server actor shutting down");
        });
        
        Self { server_tx }
    }

    pub async fn initialize(&self, args: acp::InitializeRequest) -> Result<acp::InitializeResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::Initialize(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn authenticate(&self, args: acp::AuthenticateRequest) -> Result<acp::AuthenticateResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::Authenticate(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn new_session(&self, args: acp::NewSessionRequest) -> Result<acp::NewSessionResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::NewSession(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn load_session(&self, args: acp::LoadSessionRequest) -> Result<acp::LoadSessionResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::LoadSession(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn set_session_mode(&self, args: acp::SetSessionModeRequest) -> Result<acp::SetSessionModeResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::SetSessionMode(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn prompt(&self, args: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::Prompt(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn cancel(&self, args: acp::CancelNotification) -> Result<(), acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::Cancel(args, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn ext_method(&self, method: Arc<str>, params: Arc<RawValue>) -> Result<Arc<RawValue>, acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::ExtMethod(method, params, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    pub async fn ext_notification(&self, method: Arc<str>, params: Arc<RawValue>) -> Result<(), acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.server_tx.send(ServerMethod::ExtNotification(method, params, tx)).await
            .map_err(channel_to_acp_error)?;
        rx.await.map_err(channel_to_acp_error)?
    }

    async fn handle_initialize(_args: acp::InitializeRequest) -> Result<acp::InitializeResponse, acp::Error> {
        Ok(acp::InitializeResponse {
            protocol_version: acp::V1,
            agent_capabilities: acp::AgentCapabilities::default(),
            auth_methods: Vec::new(),
            meta: None,
        })
    }

    async fn handle_authenticate(_args: acp::AuthenticateRequest) -> Result<acp::AuthenticateResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn handle_new_session(
        _args: acp::NewSessionRequest,
        _agent_name: &str,
        os: &Os,
        sessions: &mut HashMap<String, AcpServerSessionHandle>,
        transport: &AcpServerConnectionHandle,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        // Generate a new session ID
        let session_id = uuid::Uuid::new_v4().to_string();
        let acp_session_id = acp::SessionId(session_id.clone().into());
        
        tracing::info!("Creating new ACP session: {}", session_id);

        // FIXME: we need to take `_args` into account

        // Spawn session actor with transport handle
        let session_handle = AcpServerSessionHandle::spawn_local(acp_session_id.clone(), os.clone(), transport.clone());
        
        // Store session handle
        sessions.insert(session_id.clone(), session_handle);
        
        tracing::info!("Created new ACP session: {}", session_id);
        
        Ok(acp::NewSessionResponse {
            session_id: acp_session_id,
            modes: None,
            meta: None,
        })
    }

    async fn handle_load_session(
        args: acp::LoadSessionRequest,
        sessions: &HashMap<String, AcpServerSessionHandle>,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        let session_id = args.session_id.0.as_ref();
        
        // Check if session exists
        if sessions.contains_key(session_id) {
            tracing::info!("Loaded existing ACP session: {}", session_id);
            Ok(acp::LoadSessionResponse {
                modes: None,
                meta: None,
            })
        } else {
            // FIXME: we need to load the session from the database and replay it
            tracing::warn!("Session not found: {}", session_id);
            Err(acp::Error::invalid_params())
        }
    }

    async fn handle_set_session_mode(
        args: acp::SetSessionModeRequest,
        sessions: &HashMap<String, AcpServerSessionHandle>,
    ) -> Result<acp::SetSessionModeResponse, acp::Error> {
        let session_id = args.session_id.0.as_ref();
        
        // Find the session actor
        if let Some(session_handle) = sessions.get(session_id) {
            // Forward to session actor
            session_handle.set_mode(args).await
        } else {
            tracing::warn!("Session not found for set_mode: {}", session_id);
            Err(acp::Error::invalid_params())
        }
    }

    async fn handle_prompt(
        args: acp::PromptRequest,
        sessions: &HashMap<String, AcpServerSessionHandle>,
    ) -> Result<acp::PromptResponse, acp::Error> {
        let session_id = args.session_id.0.as_ref();
        
        // Find the session actor
        if let Some(session_handle) = sessions.get(session_id) {
            // Forward to session actor
            session_handle.prompt(args).await
        } else {
            tracing::warn!("Session not found for prompt: {}", session_id);
            Err(acp::Error::invalid_params())
        }
    }

    async fn handle_cancel(
        args: acp::CancelNotification,
        sessions: &HashMap<String, AcpServerSessionHandle>,
    ) -> Result<(), acp::Error> {
        let session_id = args.session_id.0.as_ref();
        
        // Find the session actor
        if let Some(session_handle) = sessions.get(session_id) {
            // Forward to session actor
            session_handle.cancel(args).await
        } else {
            tracing::warn!("Session not found for cancel: {}", session_id);
            // Cancel is a notification, so we don't return an error
            Ok(())
        }
    }

    async fn handle_ext_method(_method: Arc<str>, _params: Arc<RawValue>) -> Result<Arc<RawValue>, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn handle_ext_notification(_method: Arc<str>, _params: Arc<RawValue>) -> Result<(), acp::Error> {
        Ok(())
    }
}
