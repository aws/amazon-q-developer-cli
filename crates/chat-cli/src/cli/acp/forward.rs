//! ACP Forwarding Agent - Thin layer implementing acp::Agent

use std::sync::Arc;

use agent_client_protocol as acp;
use serde_json::value::RawValue;

use super::server::AcpServerHandle;

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
