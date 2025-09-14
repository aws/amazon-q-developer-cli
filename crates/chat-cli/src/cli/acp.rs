use std::process::ExitCode;
use std::sync::Arc;

use agent_client_protocol as acp;
use clap::Parser;
use eyre::Result;
use serde_json::value::RawValue;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::database::settings::Setting;
use crate::os::Os;

#[derive(Debug, Parser, PartialEq)]
pub struct AcpArgs {
    /// Agent to use for ACP sessions
    #[arg(long)]
    pub agent: Option<String>,
}

struct QAgent {
    _agent_name: String,
}

impl QAgent {
    fn new(agent_name: String) -> Self {
        Self { _agent_name: agent_name }
    }
}

impl acp::Agent for QAgent {
    async fn initialize(
        &self,
        arguments: acp::InitializeRequest,
    ) -> Result<acp::InitializeResponse, acp::Error> {
        tracing::info!("ACP initialize request: {arguments:?}");
        Ok(acp::InitializeResponse {
            protocol_version: acp::V1,
            agent_capabilities: acp::AgentCapabilities::default(),
            auth_methods: Vec::new(),
            meta: None,
        })
    }

    async fn authenticate(
        &self,
        _arguments: acp::AuthenticateRequest,
    ) -> Result<acp::AuthenticateResponse, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn new_session(
        &self,
        _arguments: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn load_session(
        &self,
        _arguments: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn prompt(
        &self,
        _arguments: acp::PromptRequest,
    ) -> Result<acp::PromptResponse, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn cancel(&self, _args: acp::CancelNotification) -> Result<(), acp::Error> {
        // Not implemented yet
        Ok(())
    }

    async fn set_session_mode(
        &self,
        _args: acp::SetSessionModeRequest,
    ) -> Result<acp::SetSessionModeResponse, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn ext_method(
        &self,
        _method: Arc<str>,
        _params: Arc<RawValue>,
    ) -> Result<Arc<RawValue>, acp::Error> {
        // Not implemented yet
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(
        &self,
        _method: Arc<str>,
        _params: Arc<RawValue>,
    ) -> Result<(), acp::Error> {
        // Not implemented yet
        Ok(())
    }
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
        
        // Create the Q Agent implementation
        let agent = QAgent::new(agent_name);
        
        // Set up stdio transport
        let stdin = tokio::io::stdin().compat();
        let stdout = tokio::io::stdout().compat_write();
        
        // Create ACP connection with LocalSet for non-Send futures
        let local_set = tokio::task::LocalSet::new();
        local_set.run_until(async move {
            let (_connection, handle_io) = acp::AgentSideConnection::new(
                agent, 
                stdout, 
                stdin, 
                |fut| {
                    tokio::task::spawn_local(fut);
                }
            );
            
            tracing::info!("ACP server started, waiting for client connections...");
            
            // Run the connection (this will block until the client disconnects)
            handle_io.await
                .map_err(|e| eyre::eyre!("ACP connection error: {}", e))
        }).await?;
        
        tracing::info!("ACP server shutting down");
        Ok(ExitCode::SUCCESS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_acp_command_disabled() {
        let mut os = Os::new().await.unwrap();
        
        // Explicitly disable the feature flag
        os.database.settings.set(Setting::EnabledAcp, false).await.unwrap();
        
        let args = AcpArgs { agent: None };
        let result = args.run(&mut os).await.unwrap();
        assert_eq!(result, ExitCode::FAILURE);
    }

    #[tokio::test]
    async fn test_q_agent_initialize() {
        use acp::Agent;
        
        let agent = QAgent::new("test-agent".to_string());
        
        let request = acp::InitializeRequest {
            protocol_version: acp::V1,
            client_capabilities: acp::ClientCapabilities::default(),
            meta: None,
        };
        
        let response = agent.initialize(request).await.unwrap();
        assert_eq!(response.protocol_version, acp::V1);
    }

    #[tokio::test]
    async fn test_q_agent_unimplemented_methods() {
        use acp::Agent;
        
        let agent = QAgent::new("test-agent".to_string());
        
        // Test that unimplemented methods return method_not_found
        let new_session_req = acp::NewSessionRequest {
            cwd: PathBuf::from("/tmp"),
            mcp_servers: Vec::new(),
            meta: None,
        };
        
        let result = agent.new_session(new_session_req).await;
        assert!(result.is_err());
        
        let load_session_req = acp::LoadSessionRequest {
            session_id: acp::SessionId("test".into()),
            cwd: PathBuf::from("/tmp"),
            mcp_servers: Vec::new(),
            meta: None,
        };
        
        let result = agent.load_session(load_session_req).await;
        assert!(result.is_err());
    }
}
