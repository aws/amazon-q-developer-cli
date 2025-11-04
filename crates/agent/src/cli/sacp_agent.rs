//! SACP Agent that forwards prompts to Amazon Q agent
//!
//! This is a simplified version of acp_agent.rs using SACP's request context pattern.
//! No manual queues, event loops, or completion signaling needed!
//!
//! Usage (from workspace root):
//! ```bash
//! cargo run -p agent -- sacp
//! ```

use agent::api_client::ApiClient;
use agent::mcp::McpManager;
use agent::protocol::{AgentEvent, AgentStopReason, ContentChunk, SendPromptArgs, UpdateEvent};
use agent::rts::{RtsModel, RtsModelState};
use agent::types::AgentSnapshot;
use agent::{Agent, AgentHandle};
use eyre::Result;
use sacp::{JrConnection, JrRequestCx};
use sacp::{
    InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse,
    PromptRequest, PromptResponse, CancelNotification, SessionNotification,
    SessionUpdate, ContentChunk as SacpContentChunk, ContentBlock, TextContent, ToolCall, ToolCallId,
    ToolKind, ToolCallStatus, SessionId, V1, AgentCapabilities, Implementation,
    StopReason,
};
use std::process::ExitCode;
use std::sync::Arc;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// SACP Agent handler that processes requests using Amazon Q agent
struct SacpAgentHandler {
    agent: AgentHandle,
    session_id: SessionId,
}

impl SacpAgentHandler {
    /// Create a new SACP agent handler with Amazon Q backend
    async fn new() -> Result<Self> {
        // Create agent snapshot
        let snapshot = AgentSnapshot::default();

        // Create RTS model
        let rts_state = RtsModelState::new();
        let model = Arc::new(RtsModel::new(
            ApiClient::new().await?,
            rts_state.conversation_id,
            rts_state.model_id,
        ));

        // Spawn agent
        let agent = Agent::new(snapshot, model, McpManager::new().spawn())
            .await?
            .spawn();

        Ok(Self {
            agent,
            session_id: SessionId("42".into()),
        })
    }

    /// Handle prompt request - returns immediately after spawning background task
    async fn handle_prompt_request(
        &self,
        request: PromptRequest,
        request_cx: JrRequestCx<PromptResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = self.session_id.clone();
        let mut agent = self.agent.clone();

        // Send prompt to agent (non-blocking)
        self.send_to_agent(&request).await?;

        // Get connection context (Clone) for spawning
        // Move request_cx into the task for responding
        let conn_cx = request_cx.connection_cx();

        // Spawn task to handle agent events
        // This task will run until the agent completes or errors
        let _ = conn_cx.spawn(async move {
            loop {
                match agent.recv().await {
                    Ok(event) => match event {
                        AgentEvent::Update(update_event) => {
                            // Forward updates to ACP client via notifications
                            if let Some(session_update) = convert_update_event(update_event) {
                                request_cx.send_notification(SessionNotification {
                                    session_id: session_id.clone(),
                                    update: session_update,
                                    meta: None,
                                })?;
                            }
                        }
                        AgentEvent::EndTurn(_metadata) => {
                            // Conversation complete - respond and exit task
                            return request_cx.respond(PromptResponse {
                                stop_reason: StopReason::EndTurn,
                                meta: None,
                            });
                        }
                        AgentEvent::Stop(AgentStopReason::Error(_)) => {
                            // Agent error - respond with error
                        return request_cx.respond_with_error(sacp::Error::internal_error());
                        }
                        _ => {
                            // Handle other agent events if needed
                        }
                    },
                    Err(_) => {
                        // Agent channel closed unexpectedly
                        return request_cx.respond_with_error(sacp::Error::internal_error());
                    }
                }
            }
        });

        Ok(())
    }

    /// Send prompt to the Amazon Q agent
    async fn send_to_agent(&self, request: &PromptRequest) -> Result<(), sacp::Error> {
        // Convert SACP prompt to agent format
        let content: Vec<agent::protocol::ContentChunk> = request
            .prompt
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text(text_content) => {
                    Some(agent::protocol::ContentChunk::Text(text_content.text.clone()))
                }
                _ => None, // Skip non-text content for now
            })
            .collect();

        // Send prompt to agent asynchronously
        self.agent
            .send_prompt_async(SendPromptArgs {
                content,
                should_continue_turn: None,
            })
            .await
            .map_err(|_| sacp::Error::internal_error())?;

        Ok(())
    }
}

/// Convert agent UpdateEvent to SessionUpdate
fn convert_update_event(update_event: UpdateEvent) -> Option<SessionUpdate> {
    match update_event {
        UpdateEvent::AgentContent(ContentChunk::Text(text)) => {
            Some(SessionUpdate::AgentMessageChunk(SacpContentChunk {
                content: ContentBlock::Text(TextContent {
                    text,
                    annotations: None,
                    meta: None,
                }),
                meta: None,
            }))
        }
        UpdateEvent::ToolCall(tool_call) => {
            let sacp_tool_call = ToolCall {
                id: ToolCallId(tool_call.id.into()),
                title: tool_call.tool_use_block.name.clone(),
                kind: ToolKind::default(),
                status: ToolCallStatus::Pending,
                content: vec![],
                locations: vec![],
                raw_input: Some(tool_call.tool_use_block.input.clone()),
                raw_output: None,
                meta: None,
            };
            Some(SessionUpdate::ToolCall(sacp_tool_call))
        }
        _ => None, // Skip other events
    }
}

/// Entry point for SACP agent
pub async fn execute() -> Result<ExitCode> {
    eprintln!("Starting SACP agent");
    
    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    // Create handler
    let handler = Arc::new(SacpAgentHandler::new().await?);

    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            // Create SACP connection with handlers
            let connection = JrConnection::new(outgoing, incoming)
                // Handle initialize request
                .on_receive_request({
                    async move |_request: InitializeRequest, request_cx| {
                        eprintln!("Received initialize request");
                        request_cx.respond(InitializeResponse {
                            protocol_version: V1,
                            agent_capabilities: AgentCapabilities::default(),
                            auth_methods: Vec::new(),
                            agent_info: Some(Implementation {
                                name: "amazon-q-agent".to_string(),
                                title: Some("Amazon Q Agent".to_string()),
                                version: env!("CARGO_PKG_VERSION").to_string(),
                            }),
                            meta: None,
                        })
                    }
                })
                // Handle new_session request
                .on_receive_request({
                    async move |_request: NewSessionRequest, request_cx| {
                        request_cx.respond(NewSessionResponse {
                            session_id: SessionId("42".into()),
                            modes: None,
                            meta: None,
                        })
                    }
                })
                // Handle prompt request
                .on_receive_request({
                    let handler = Arc::clone(&handler);
                    async move |request: PromptRequest, request_cx| {
                        eprintln!("Received prompt request");
                        handler.handle_prompt_request(request, request_cx).await
                    }
                })
                // Handle cancel notification
                .on_receive_notification({
                    async move |_notification: CancelNotification, _cx| {
                        // TODO: Implement cancellation if needed
                        Ok(())
                    }
                });

            // Run the connection
            eprintln!("Starting SACP connection");
            connection
                .serve()
                .await
                .map_err(|e| eyre::eyre!("Connection error: {}", e))
        })
        .await?;

    Ok(ExitCode::SUCCESS)
}
