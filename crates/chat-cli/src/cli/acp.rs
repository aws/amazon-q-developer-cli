//! Agent Client Protocol (ACP) implementation using actor pattern
//!
//! This module implements ACP server functionality using Alice Ryhl's actor pattern
//! for clean separation of concerns and message passing instead of shared state.
//!
//! ## Architecture Flow
//!
//! When an ACP client sends a prompt request:
//!
//! ```text
//! ACP Client                 AcpAgentForward           AcpServerActor           AcpSessionActor
//!     │                           │                         │                        │
//!     │ acp.prompt("Hi")          │                         │                        │
//!     ├──────JSON-RPC────────────→│                         │                        │
//!     │                           │ ServerMethod::Prompt    │                        │
//!     │                           ├────────channel─────────→│                        │
//!     │                           │                         │ SessionMethod::Prompt │
//!     │                           │                         ├───────channel────────→│
//!     │                           │                         │                        │ ConversationState
//!     │                           │                         │                        │ processes prompt
//!     │                           │                         │                        │ with LLM
//!     │                           │                         │                        │
//!     │                           │                         │ ←──────response───────│
//!     │                           │ ←──────response─────────│                        │
//!     │ ←────JSON-RPC─────────────│                         │                        │
//! ```
//!
//! ## Key Benefits
//!
//! - **No shared state**: Each actor owns its data (no RwLocks)
//! - **Natural backpressure**: Bounded channels prevent unbounded queuing
//! - **Clean separation**: Protocol handling, session management, and conversation processing are separate
//! - **Easy testing**: Each actor can be tested independently

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;

use agent_client_protocol as acp;
use clap::Parser;
use eyre::Result;
use serde_json::value::RawValue;
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::database::settings::Setting;
use crate::os::Os;

#[cfg(test)]
mod tests;

/// Convert channel errors to ACP errors
fn channel_to_acp_error<E>(_err: E) -> acp::Error {
    acp::Error::internal_error()
}

#[derive(Debug, Parser, PartialEq)]
pub struct AcpArgs {
    /// Agent to use for ACP sessions
    #[arg(long)]
    pub agent: Option<String>,
}

impl AcpArgs {
    pub async fn run(self, os: &mut Os) -> Result<ExitCode> {
        // Check feature flag
        if !os.database.settings.get_bool(Setting::EnabledAcp).unwrap_or(false) {
            eprintln!("ACP is disabled. Enable with: q settings acp.enabled true");
            return Ok(ExitCode::FAILURE);
        }

        let agent_name = self.agent.unwrap_or_else(|| "default".to_string());
        
        tracing::info!("Starting ACP server with agent: {}", agent_name);
        
        // Create ACP server with LocalSet for non-Send futures
        let local_set = tokio::task::LocalSet::new();
        local_set.run_until(async move {
            // Spawn the server actor
            let server_handle = AcpServerHandle::spawn(agent_name, os.clone());
            
            // Create forwarding agent
            let agent = AcpAgentForward::new(server_handle);
            
            // Set up ACP connection with stdio
            let stdin = tokio::io::stdin().compat();
            let stdout = tokio::io::stdout().compat_write();
            
            let (connection, handle_io) = acp::AgentSideConnection::new(
                agent,
                stdout,
                stdin,
                |fut| {
                    tokio::task::spawn_local(fut);
                }
            );
            
            tracing::info!("ACP server started, waiting for client connections...");
            
            // Run the connection (this will block until the client disconnects)
            if let Err(e) = handle_io.await {
                tracing::error!("ACP connection error: {}", e);
            }
            
            tracing::info!("ACP server shutting down gracefully");
            Ok::<(), eyre::Error>(())
        }).await?;
        
        Ok(ExitCode::SUCCESS)
    }
}

// ============================================================================
// Server Actor - Top-level coordinator that manages sessions
// ============================================================================

/// Handle to the ACP server actor
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
    pub fn spawn(agent_name: String, os: Os) -> Self {
        let (server_tx, mut server_rx) = mpsc::channel(32);
        
        tokio::task::spawn_local(async move {
            let mut sessions: HashMap<String, AcpSessionHandle> = HashMap::new();
            
            while let Some(method) = server_rx.recv().await {
                match method {
                    ServerMethod::Initialize(args, tx) => {
                        let response = Self::handle_initialize(args).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("Initialize response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                    ServerMethod::Authenticate(args, tx) => {
                        let response = Self::handle_authenticate(args).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("Authenticate response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                    ServerMethod::NewSession(args, tx) => {
                        let response = Self::handle_new_session(args, &agent_name, &os, &mut sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("NewSession response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                    ServerMethod::LoadSession(args, tx) => {
                        let response = Self::handle_load_session(args, &sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("LoadSession response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                    ServerMethod::SetSessionMode(args, tx) => {
                        let response = Self::handle_set_session_mode(args, &sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("SetSessionMode response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                    ServerMethod::Prompt(args, tx) => {
                        let response = Self::handle_prompt(args, &sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("Prompt response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                    ServerMethod::Cancel(args, tx) => {
                        let response = Self::handle_cancel(args, &sessions).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("Cancel response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                    ServerMethod::ExtMethod(method, params, tx) => {
                        let response = Self::handle_ext_method(method, params).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("ExtMethod response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                    ServerMethod::ExtNotification(method, params, tx) => {
                        let response = Self::handle_ext_notification(method, params).await;
                        if tx.send(response).is_err() {
                            tracing::debug!("ExtNotification response receiver dropped, exiting server actor");
                            break;
                        }
                    }
                }
            }
            
            tracing::info!("Server actor shutting down");
        });
        
        Self { server_tx }
    }

    async fn handle_initialize(_args: acp::InitializeRequest) -> Result<acp::InitializeResponse, acp::Error> {
        Ok(acp::InitializeResponse {
            protocol_version: acp::ProtocolVersion::V1,
            agent_capabilities: acp::AgentCapabilities::default(),
            auth_methods: Vec::new(),
            meta: None,
        })
    }

    async fn handle_authenticate(_args: acp::AuthenticateRequest) -> Result<acp::AuthenticateResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn handle_new_session(
        args: acp::NewSessionRequest,
        _agent_name: &str,
        os: &Os,
        sessions: &mut HashMap<String, AcpSessionHandle>,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        // Generate a new session ID
        let session_id = uuid::Uuid::new_v4().to_string();
        let acp_session_id = acp::SessionId(session_id.clone().into());
        
        tracing::info!("Creating new ACP session: {}", session_id);
        
        // Spawn session actor
        let session_handle = AcpSessionHandle::spawn(acp_session_id.clone(), os.clone());
        
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
        sessions: &HashMap<String, AcpSessionHandle>,
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
            tracing::warn!("Session not found: {}", session_id);
            Err(acp::Error::invalid_params())
        }
    }

    async fn handle_set_session_mode(
        args: acp::SetSessionModeRequest,
        sessions: &HashMap<String, AcpSessionHandle>,
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
        sessions: &HashMap<String, AcpSessionHandle>,
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
        sessions: &HashMap<String, AcpSessionHandle>,
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
}

// ============================================================================
// Session Actor - Per-session actor that owns conversation state
// ============================================================================

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
    pub fn spawn(session_id: acp::SessionId, os: Os) -> Self {
        let (session_tx, mut session_rx) = mpsc::channel(32);
        
        tokio::task::spawn_local(async move {
            tracing::debug!("Session actor started for session: {}", session_id.0);
            
            while let Some(method) = session_rx.recv().await {
                match method {
                    SessionMethod::Prompt(args, tx) => {
                        let response = Self::handle_prompt(args).await;
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

    async fn handle_prompt(_args: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        // TODO: Process prompt with conversation state
        Err(acp::Error::method_not_found())
    }

    async fn handle_cancel(_args: acp::CancelNotification) -> Result<(), acp::Error> {
        // TODO: Cancel ongoing operations
        Ok(())
    }

    async fn handle_set_mode(_args: acp::SetSessionModeRequest) -> Result<acp::SetSessionModeResponse, acp::Error> {
        // TODO: Set session mode
        Err(acp::Error::method_not_found())
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
}

// ============================================================================
// Forwarding Agent - Thin layer implementing acp::Agent
// ============================================================================

/// Forwarding implementation of acp::Agent that sends all calls to server actor
pub struct AcpAgentForward {
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
        self.server_handle.prompt(arguments).await
    }

    async fn cancel(&self, args: acp::CancelNotification) -> Result<(), acp::Error> {
        self.server_handle.cancel(args).await
    }

    async fn set_session_mode(&self, args: acp::SetSessionModeRequest) -> Result<acp::SetSessionModeResponse, acp::Error> {
        self.server_handle.set_session_mode(args).await
    }

    async fn ext_method(&self, method: Arc<str>, params: Arc<RawValue>) -> Result<Arc<RawValue>, acp::Error> {
        self.server_handle.ext_method(method, params).await
    }

    async fn ext_notification(&self, method: Arc<str>, params: Arc<RawValue>) -> Result<(), acp::Error> {
        self.server_handle.ext_notification(method, params).await
    }
}

// ============================================================================
// Handler Functions - Now moved to associated methods on actor handles
// ============================================================================
