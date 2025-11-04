//! SACP Agent that forwards prompts to Amazon Q agent
//!
//! This is a simplified version of acp_agent.rs using SACP's request context pattern.
//! No manual queues, event loops, or completion signaling needed!
//!
//! Usage (from workspace root):
//! ```bash
//! cargo run -p agent -- acp
//! ```

use std::process::ExitCode;
use std::sync::Arc;

use agent::api_client::ApiClient;
use agent::mcp::McpManager;
use agent::protocol::{
    AgentEvent,
    AgentStopReason,
    ContentChunk,
    SendPromptArgs,
    UpdateEvent,
};
use agent::rts::{
    RtsModel,
    RtsModelState,
};
use agent::types::AgentSnapshot;
use agent::{
    Agent,
    AgentHandle,
};
use eyre::Result;
use sacp::{
    AgentCapabilities,
    CancelNotification,
    ContentBlock,
    ContentChunk as SacpContentChunk,
    Implementation,
    InitializeRequest,
    InitializeResponse,
    JrConnection,
    JrRequestCx,
    NewSessionRequest,
    NewSessionResponse,
    PermissionOption,
    PermissionOptionId,
    PermissionOptionKind,
    PromptRequest,
    PromptResponse,
    RequestPermissionRequest,
    SessionId,
    SessionNotification,
    SessionUpdate,
    StopReason,
    TextContent,
    ToolCall,
    ToolCallId,
    ToolCallStatus,
    ToolCallUpdate,
    ToolCallUpdateFields,
    ToolKind,
    V1,
};
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};

/// ACP Session that processes requests using Amazon Q agent
struct AcpSession {
    agent: AgentHandle,
    session_id: SessionId,
}

impl AcpSession {
    /// Create a new ACP session handler with Amazon Q backend
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
        let agent = Agent::new(snapshot, model, McpManager::new().spawn()).await?.spawn();

        Ok(Self {
            agent,
            session_id: SessionId("42".into()),
        })
    }

    /// Handle prompt request. Overall we do the following:
    ///  - submit the request to the agent
    ///  - convert agent update events to ACP update events and send them back to ACP client
    ///  - tell ACP client that the request is completed
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

        // AVOID blocking the main event loop because it needs to do other work!
        // Wait for the conversation turn to be completed in a different task
        let _ = conn_cx.spawn(async move {
            loop {
                match agent.recv().await {
                    Ok(event) => match event {
                        AgentEvent::Update(update_event) => {
                            eprintln!("Received update_event: {:?}", update_event);
                            // Forward updates to ACP client via notifications
                            if let Some(session_update) = convert_update_event(update_event) {
                                request_cx.send_notification(SessionNotification {
                                    session_id: session_id.clone(),
                                    update: session_update,
                                    meta: None,
                                })?;
                            }
                        },
                        AgentEvent::ApprovalRequest { id, tool_use, context } => {
                            eprintln!("Received ApprovalRequest: id={}, tool_use={:?}, context={:?}", id, tool_use, context);
                            
                            let permission_request = RequestPermissionRequest {
                                session_id: session_id.clone(),
                                tool_call: ToolCallUpdate {
                                    id: ToolCallId(tool_use.tool_use_id.clone().into()),
                                    fields: ToolCallUpdateFields {
                                        status: Some(ToolCallStatus::Pending),
                                        title: Some(tool_use.name.clone()),
                                        raw_input: Some(tool_use.input.clone()),
                                        ..Default::default()
                                    },
                                    meta: None,
                                },
                                options: vec![
                                    PermissionOption {
                                        id: PermissionOptionId("allow".into()),
                                        name: "Allow".to_string(),
                                        kind: PermissionOptionKind::AllowOnce,
                                        meta: None,
                                    },
                                    PermissionOption {
                                        id: PermissionOptionId("deny".into()),
                                        name: "Deny".to_string(),
                                        kind: PermissionOptionKind::RejectOnce,
                                        meta: None,
                                    },
                                ],
                                meta: None,
                            };
                            
                            eprintln!("Sending permission_request: {:?}", permission_request);
                            
                            let agent_for_approval = agent.clone();
                            request_cx.send_request(permission_request).await_when_result_received(|result| async move {
                                match result {
                                    Ok(response) => {
                                        match &response.outcome {
                                            sacp::RequestPermissionOutcome::Selected { option_id } => {
                                                let approval_result = if option_id.0.as_ref() == "allow" {
                                                    agent::protocol::ApprovalResult::Approve
                                                } else {
                                                    agent::protocol::ApprovalResult::Deny { reason: None }
                                                };
                                                
                                                if let Err(e) = agent_for_approval.send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                                                    id: id.clone(),
                                                    result: approval_result,
                                                }).await {
                                                    eprintln!("Failed to send approval result: {:?}", e);
                                                }
                                            },
                                            sacp::RequestPermissionOutcome::Cancelled => {
                                                if let Err(e) = agent_for_approval.send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                                                    id: id.clone(),
                                                    result: agent::protocol::ApprovalResult::Deny { reason: Some("Cancelled".to_string()) },
                                                }).await {
                                                    eprintln!("Failed to send cancellation result: {:?}", e);
                                                }
                                            },
                                        }
                                        eprintln!("Permission response: {:?}", response);
                                    },
                                    Err(err) => {
                                        eprintln!("Permission request failed: {:?}", err);
                                        if let Err(e) = agent_for_approval.send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                                            id: id.clone(),
                                            result: agent::protocol::ApprovalResult::Deny { reason: Some("Request failed".to_string()) },
                                        }).await {
                                            eprintln!("Failed to send error result: {:?}", e);
                                        }
                                    }
                                }
                                Ok(())
                            })?;

                            eprintln!("End permission_request");
                        },
                        AgentEvent::EndTurn(_metadata) => {
                            // Conversation complete - respond and exit task
                            return request_cx.respond(PromptResponse {
                                stop_reason: StopReason::EndTurn,
                                meta: None,
                            });
                        },
                        AgentEvent::Stop(AgentStopReason::Error(_)) => {
                            // Agent error - respond with error
                            return request_cx.respond_with_error(sacp::Error::internal_error());
                        },
                        _ => {
                            // Handle other agent events if needed
                        },
                    },
                    Err(_) => {
                        // Agent channel closed unexpectedly
                        return request_cx.respond_with_error(sacp::Error::internal_error());
                    },
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
                },
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
        },
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
        },
        _ => None, // Skip other events
    }
}

/// Entry point for SACP agent
pub async fn execute() -> Result<ExitCode> {
    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    // Create handler
    let session = Arc::new(AcpSession::new().await?);

    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            // Create SACP connection with handlers
            let connection = JrConnection::new(outgoing, incoming)
                // Handle initialize request
                .on_receive_request({
                    async move |_request: InitializeRequest, request_cx| {
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
                    let session = Arc::clone(&session);
                    async move |request: PromptRequest, request_cx| {
                        session.handle_prompt_request(request, request_cx).await
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
            connection
                .serve()
                .await
                .map_err(|e| eyre::eyre!("Connection error: {}", e))
        })
        .await?;

    Ok(ExitCode::SUCCESS)
}
