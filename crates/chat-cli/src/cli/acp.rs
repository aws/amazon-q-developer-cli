use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;

use agent_client_protocol as acp;
use clap::Parser;
use eyre::Result;
use serde_json::value::RawValue;
use tokio::sync::RwLock;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::cli::agent::Agents;
use crate::cli::ConversationState;
use crate::cli::chat::tool_manager::ToolManager;
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
    os: Arc<RwLock<Os>>,
    sessions: Arc<RwLock<HashMap<String, ConversationState>>>,
}

impl QAgent {
    fn new(agent_name: String, os: Os) -> Self {
        Self { 
            _agent_name: agent_name,
            os: Arc::new(RwLock::new(os)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
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
            agent_capabilities: acp::AgentCapabilities {
                load_session: true,
                prompt_capabilities: acp::PromptCapabilities::default(),
                mcp_capabilities: acp::McpCapabilities::default(),
                meta: None,
            },
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
        arguments: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        tracing::info!("ACP new_session request: {arguments:?}");
        
        // Generate a new session ID
        let session_id = uuid::Uuid::new_v4().to_string();
        
        // Get OS reference
        let mut os = self.os.write().await;
        
        // Create agents (using default for now)
        let agents = Agents::default();
        
        // Create tool manager
        let mut tool_manager = ToolManager::default();
        let tool_config = tool_manager.load_tools(&mut *os, &mut vec![]).await
            .map_err(|e| {
                tracing::error!("Failed to load tools: {}", e);
                acp::Error::internal_error()
            })?;
        
        // Create new conversation state
        let conversation = ConversationState::new(
            &session_id,
            agents,
            tool_config,
            tool_manager,
            None, // model_id
            &*os,
            false, // mcp_enabled for now
        ).await;
        
        // Store the session
        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), conversation);
        
        tracing::info!("Created new ACP session: {}", session_id);
        
        Ok(acp::NewSessionResponse {
            session_id: acp::SessionId(session_id.into()),
            modes: None,
            meta: None,
        })
    }

    async fn load_session(
        &self,
        arguments: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        tracing::info!("ACP load_session request: {arguments:?}");
        
        let session_id = arguments.session_id.0.as_ref();
        
        // Check if session exists
        let sessions = self.sessions.read().await;
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

    async fn prompt(
        &self,
        arguments: acp::PromptRequest,
    ) -> Result<acp::PromptResponse, acp::Error> {
        tracing::info!("ACP prompt request: session_id={}", arguments.session_id.0);
        
        let session_id = arguments.session_id.0.as_ref();
        
        // Convert ACP ContentBlocks to a single prompt string
        let mut prompt_text = String::new();
        for content_block in arguments.prompt {
            match content_block {
                acp::ContentBlock::Text(text_content) => {
                    if !prompt_text.is_empty() {
                        prompt_text.push('\n');
                    }
                    prompt_text.push_str(&text_content.text);
                },
                acp::ContentBlock::ResourceLink(resource_link) => {
                    // For now, just include the URI as text
                    if !prompt_text.is_empty() {
                        prompt_text.push('\n');
                    }
                    prompt_text.push_str(&format!("Resource: {}", resource_link.uri));
                },
                acp::ContentBlock::Resource(embedded_resource) => {
                    // Include the resource contents
                    if !prompt_text.is_empty() {
                        prompt_text.push('\n');
                    }
                    match &embedded_resource.resource {
                        acp::EmbeddedResourceResource::TextResourceContents(text_resource) => {
                            prompt_text.push_str(&format!("Resource {}: {}", 
                                text_resource.uri, 
                                text_resource.text));
                        },
                        acp::EmbeddedResourceResource::BlobResourceContents(blob_resource) => {
                            prompt_text.push_str(&format!("Resource {}: [Binary content]", 
                                blob_resource.uri));
                        },
                    }
                },
                acp::ContentBlock::Image(_) | acp::ContentBlock::Audio(_) => {
                    // Not supported yet - skip or add placeholder
                    if !prompt_text.is_empty() {
                        prompt_text.push('\n');
                    }
                    prompt_text.push_str("[Unsupported content type]");
                },
            }
        }
        
        // Get the session and add the prompt
        {
            let mut sessions = self.sessions.write().await;
            let conversation = sessions.get_mut(session_id)
                .ok_or_else(|| {
                    tracing::warn!("Session not found: {}", session_id);
                    acp::Error::invalid_params()
                })?;
            
            // Add the prompt to the conversation state
            conversation.set_next_user_message(prompt_text).await;
        } // Release the lock before returning
        
        tracing::info!("Added prompt to ACP session: {}", session_id);
        
        // For now, just return EndTurn - actual LLM processing will come in Phase 3
        Ok(acp::PromptResponse {
            stop_reason: acp::StopReason::EndTurn,
            meta: None,
        })
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
        
        // Create the Q Agent implementation (move os into the agent)
        let agent = QAgent::new(agent_name, os.clone());
        
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
        
        let os = Os::new().await.unwrap();
        let agent = QAgent::new("test-agent".to_string(), os);
        
        let request = acp::InitializeRequest {
            protocol_version: acp::V1,
            client_capabilities: acp::ClientCapabilities::default(),
            meta: None,
        };
        
        let response = agent.initialize(request).await.unwrap();
        assert_eq!(response.protocol_version, acp::V1);
    }

    #[tokio::test]
    async fn test_q_agent_session_lifecycle() {
        use acp::Agent;
        
        let os = Os::new().await.unwrap();
        let agent = QAgent::new("test-agent".to_string(), os);
        
        // Test new session
        let new_session_req = acp::NewSessionRequest {
            cwd: PathBuf::from("/tmp"),
            mcp_servers: Vec::new(),
            meta: None,
        };
        
        let new_session_resp = agent.new_session(new_session_req).await.unwrap();
        let session_id = new_session_resp.session_id.clone();
        
        // Verify session was created
        assert!(!session_id.0.is_empty());
        
        // Test load session with existing session
        let load_session_req = acp::LoadSessionRequest {
            session_id: session_id.clone(),
            cwd: PathBuf::from("/tmp"),
            mcp_servers: Vec::new(),
            meta: None,
        };
        
        let load_session_resp = agent.load_session(load_session_req).await;
        assert!(load_session_resp.is_ok());
        
        // Test load session with non-existent session
        let load_nonexistent_req = acp::LoadSessionRequest {
            session_id: acp::SessionId("nonexistent".into()),
            cwd: PathBuf::from("/tmp"),
            mcp_servers: Vec::new(),
            meta: None,
        };
        
        let result = agent.load_session(load_nonexistent_req).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore] // TODO: Fix hanging issue in set_next_user_message
    async fn test_q_agent_prompt_handling() {
        use acp::Agent;
        
        let os = Os::new().await.unwrap();
        let agent = QAgent::new("test-agent".to_string(), os);
        
        // First create a session
        let new_session_req = acp::NewSessionRequest {
            cwd: PathBuf::from("/tmp"),
            mcp_servers: Vec::new(),
            meta: None,
        };
        
        let new_session_resp = agent.new_session(new_session_req).await.unwrap();
        let session_id = new_session_resp.session_id.clone();
        
        // Test prompt with text content
        let prompt_req = acp::PromptRequest {
            session_id: session_id.clone(),
            prompt: vec![
                acp::ContentBlock::Text(acp::TextContent {
                    annotations: None,
                    text: "Hello, world!".to_string(),
                    meta: None,
                })
            ],
            meta: None,
        };
        
        let prompt_resp = agent.prompt(prompt_req).await.unwrap();
        assert_eq!(prompt_resp.stop_reason, acp::StopReason::EndTurn);
        
        // Test prompt with non-existent session (should fail quickly)
        let invalid_prompt_req = acp::PromptRequest {
            session_id: acp::SessionId("nonexistent".into()),
            prompt: vec![
                acp::ContentBlock::Text(acp::TextContent {
                    annotations: None,
                    text: "This should fail".to_string(),
                    meta: None,
                })
            ],
            meta: None,
        };
        
        let result = agent.prompt(invalid_prompt_req).await;
        assert!(result.is_err());
    }
}
