//! ACP Agent interface for Q CLI agent
//!
//! Usage (from workspace root):
//! ```bash
//! cargo run -p agent -- acp
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::process::ExitCode;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;

use agent::agent_loop::types::ToolUseBlock;
use agent::api_client::ApiClient;
use agent::mcp::McpManager;
use agent::protocol::{
    AgentEvent,
    AgentStopReason,
    ContentChunk,
    SendPromptArgs,
    ToolCallResult,
    UpdateEvent,
};
use agent::rts::{
    RtsModel,
    RtsModelState,
};
use agent::tools::BuiltInToolName;
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
    Diff,
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
    ToolCallContent,
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
use tracing::info;

/// ACP Session that processes requests using Amazon Q agent
struct AcpSession {
    agent: AgentHandle,
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

        Ok(Self { agent })
    }

    /// Handle user request from ACP client:
    ///  - submit the request to the agent
    ///  - poll agent update events, convert them to ACP events, and send them back to ACP client
    ///  - tell ACP client that the request is completed
    async fn handle_prompt_request(
        &self,
        request: PromptRequest,
        request_cx: JrRequestCx<PromptResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let mut agent = self.agent.clone();

        // Send user request to agent (non-blocking)
        self.send_request_async(&request).await?;

        // We want to avoid blocking the main event loop because it needs to do other work!
        // so spawn a new task and wait for end of turn
        let _ = request_cx.clone().spawn(async move {
            loop {
                match agent.recv().await {
                    Ok(event) => match event {
                        AgentEvent::Update(update_event) => {
                            handle_update_event(update_event, session_id.clone(), &request_cx)?;
                        },
                        AgentEvent::ApprovalRequest { id, tool_use, context } => {
                            info!(
                                "AgentEvent::ApprovalRequest: id={}, tool_use={:?}, context={:?}",
                                id, tool_use, context
                            );
                            handle_approval_request(id, tool_use, session_id.clone(), agent.clone(), &request_cx)?;
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

    /// Send user request to agent
    async fn send_request_async(&self, request: &PromptRequest) -> Result<(), sacp::Error> {
        // Convert ACP prompt request to agent format
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

/// Handle agent update event and forward to ACP client
fn handle_update_event(
    update_event: UpdateEvent,
    session_id: SessionId,
    request_cx: &JrRequestCx<PromptResponse>,
) -> Result<(), sacp::Error> {
    let session_update = match update_event {
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
            let acp_tool_call = ToolCall {
                id: ToolCallId(tool_call.id.into()),
                title: tool_call.tool_use_block.name.clone(),
                kind: get_tool_kind(&tool_call.tool_use_block.name),
                status: ToolCallStatus::InProgress,
                content: get_tool_content(&tool_call.tool_use_block.name, &tool_call.tool_use_block.input),
                locations: vec![], // TODO: We need line number for fs_write
                raw_input: Some(tool_call.tool_use_block.input.clone()),
                raw_output: None,
                meta: None,
            };
            Some(SessionUpdate::ToolCall(acp_tool_call))
        },
        UpdateEvent::ToolCallFinished { tool_call, result } => {
            let (status, raw_output) = match result {
                ToolCallResult::Success(output) => (ToolCallStatus::Completed, serde_json::to_value(output).ok()),
                ToolCallResult::Error(_) => (ToolCallStatus::Failed, None),
                ToolCallResult::Cancelled => (ToolCallStatus::Failed, None),
            };
            Some(SessionUpdate::ToolCallUpdate(ToolCallUpdate {
                id: ToolCallId(tool_call.id.into()),
                fields: ToolCallUpdateFields {
                    status: Some(status),
                    title: Some(tool_call.tool_use_block.name.clone()),
                    kind: Some(get_tool_kind(&tool_call.tool_use_block.name)),
                    raw_input: Some(tool_call.tool_use_block.input.clone()),
                    raw_output,
                    ..Default::default()
                },
                meta: None,
            }))
        },
        _ => None,
    };

    if let Some(update) = session_update {
        request_cx.send_notification(SessionNotification {
            session_id,
            update,
            meta: None,
        })?;
    }
    Ok(())
}

/// Get ToolKind for a tool name
fn get_tool_kind(tool_name: &str) -> ToolKind {
    if let Ok(builtin_tool) = BuiltInToolName::from_str(tool_name) {
        match builtin_tool {
            BuiltInToolName::FsRead => ToolKind::Read,
            BuiltInToolName::FsWrite => ToolKind::Edit,
            BuiltInToolName::ExecuteCmd => ToolKind::Execute,
            BuiltInToolName::ImageRead => ToolKind::Read,
            BuiltInToolName::Ls => ToolKind::Read,
        }
    } else {
        ToolKind::Other
    }
}

/// Get content for tool calls based on tool type
fn get_tool_content(tool_name: &str, input: &serde_json::Value) -> Vec<ToolCallContent> {
    if let Ok(builtin_tool) = BuiltInToolName::from_str(tool_name) {
        match builtin_tool {
            BuiltInToolName::FsWrite => {
                // for fs_write we need to populate "Diff" content
                let path = input["path"].as_str().unwrap();
                let command = input["command"].as_str().unwrap();

                let (old_text, new_text) = match command {
                    "create" => {
                        let content = input["content"].as_str().unwrap().to_string();
                        (None, content)
                    },
                    "strReplace" => {
                        let old_str = input["oldStr"].as_str().unwrap().to_string();
                        let new_str = input["newStr"].as_str().unwrap().to_string();
                        (Some(old_str), new_str)
                    },
                    _ => return vec![],
                };

                vec![ToolCallContent::Diff {
                    diff: Diff {
                        path: path.into(),
                        old_text,
                        new_text,
                        meta: None,
                    },
                }]
            },
            _ => vec![],
        }
    } else {
        vec![]
    }
}

/// Handle tool use approval request
fn handle_approval_request(
    id: String,
    tool_use: ToolUseBlock,
    session_id: SessionId,
    agent: AgentHandle,
    request_cx: &JrRequestCx<PromptResponse>,
) -> Result<(), sacp::Error> {
    let permission_request = RequestPermissionRequest {
        session_id,
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

    request_cx
        .send_request(permission_request)
        .await_when_result_received(|result| async move {
            info!("Permission request result: {:?}", result);
            let approval_result = match result {
                Ok(response) => match &response.outcome {
                    sacp::RequestPermissionOutcome::Selected { option_id } => {
                        if option_id.0.as_ref() == "allow" {
                            agent::protocol::ApprovalResult::Approve
                        } else {
                            agent::protocol::ApprovalResult::Deny { reason: None }
                        }
                    },
                    sacp::RequestPermissionOutcome::Cancelled => agent::protocol::ApprovalResult::Deny {
                        reason: Some("Cancelled".to_string()),
                    },
                },
                Err(_) => agent::protocol::ApprovalResult::Deny {
                    reason: Some("Request failed".to_string()),
                },
            };

            let _ = agent
                .send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                    id,
                    result: approval_result,
                })
                .await;
            Ok(())
        })
}

/// Entry point for SACP agent
pub async fn execute() -> Result<ExitCode> {
    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    // Create session manager
    let sessions = Rc::new(RefCell::new(HashMap::new()));

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
                    let sessions = Rc::clone(&sessions);
                    async move |_request: NewSessionRequest, request_cx| {
                        let session_id = SessionId(uuid::Uuid::new_v4().to_string().into());
                        let session = Rc::new(AcpSession::new().await.map_err(|_| sacp::Error::internal_error())?);

                        sessions.borrow_mut().insert(session_id.clone(), session);

                        request_cx.respond(NewSessionResponse {
                            session_id,
                            modes: None,
                            meta: None,
                        })
                    }
                })
                // Handle prompt request
                .on_receive_request({
                    let sessions = Rc::clone(&sessions);
                    async move |request: PromptRequest, request_cx| {
                        let session = sessions.borrow().get(&request.session_id).cloned();

                        match session {
                            Some(session) => session.handle_prompt_request(request, request_cx).await,
                            None => request_cx.respond_with_error(sacp::Error::invalid_request()),
                        }
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
