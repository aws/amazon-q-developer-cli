//! ACP Agent interface for Q CLI agent
//!
//! Usage (from workspace root):
//! ```bash
//! cargo run -p chat_cli -- acp
//! or
//! ./target/debug/chat_cli acp
//! ```

use std::collections::HashMap;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::{
    Arc,
    Mutex,
};

use agent::agent_loop::types::ToolUseBlock;
use agent::mcp::McpManager;
use agent::protocol::{
    AgentEvent,
    AgentStopReason,
    ContentChunk,
    SendPromptArgs,
    ToolCallResult,
    UpdateEvent,
};
use agent::tools::fs_write::FsWrite;
use agent::tools::{
    BuiltInTool,
    BuiltInToolName,
    Tool,
    ToolKind as AgentToolKind,
};
use agent::types::AgentSnapshot;
use agent::{
    Agent,
    AgentHandle,
};
use eyre::Result;
use sacp::schema::{
    AgentCapabilities,
    CancelNotification,
    ContentBlock,
    ContentChunk as SacpContentChunk,
    Diff,
    InitializeRequest,
    InitializeResponse,
    NewSessionRequest,
    NewSessionResponse,
    PermissionOption,
    PermissionOptionId,
    PermissionOptionKind,
    PromptRequest,
    PromptResponse,
    ProtocolVersion,
    RequestPermissionOutcome,
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
};
use sacp::{
    AgentToClient,
    JrConnectionCx,
    JrRequestCx,
};
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};
use tracing::{
    error,
    info,
};

use crate::agent::rts::{
    RtsModel,
    RtsModelState,
};
use crate::os::Os;

/// ACP Session that processes requests using Kiro Agent
struct AcpSession {
    agent: AgentHandle,
}

impl AcpSession {
    /// Create a new ACP session handler with Kiro Agent backend
    async fn new(os: Os) -> Result<Self> {
        // Create agent snapshot
        let snapshot = AgentSnapshot::default();

        // Create RTS model
        let rts_state = RtsModelState::new();
        let model = Arc::new(RtsModel::new(
            os.client.clone(),
            rts_state.conversation_id,
            rts_state.model_id,
        ));

        // Spawn agent
        let agent = Agent::new(snapshot, None, None, model, McpManager::default().spawn(), false)
            .await?
            .spawn();

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
        mut cx: JrConnectionCx<sacp::AgentToClient>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let mut agent = self.agent.clone();

        // Send user request to agent
        self.send_request(&request).await?;

        // We want to avoid blocking the main event loop because it needs to do other work!
        // so spawn a new task and wait for end of turn
        let _ = cx.clone().spawn(async move {
            loop {
                match agent.recv().await {
                    Ok(event) => match event {
                        AgentEvent::Update(update_event) => {
                            handle_update_event(update_event, session_id.clone(), &request_cx, &mut cx)?;
                        },
                        AgentEvent::ApprovalRequest { id, tool_use, context } => {
                            info!(
                                "AgentEvent::ApprovalRequest: id={}, tool_use={:?}, context={:?}",
                                id, tool_use, context
                            );
                            handle_approval_request(id, tool_use, session_id.clone(), agent.clone(), &request_cx, &mut cx)?;
                        },
                        AgentEvent::EndTurn(_metadata) => {
                            // Conversation complete - respond and exit task
                            return request_cx.respond(PromptResponse::new(StopReason::EndTurn));
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
    async fn send_request(&self, request: &PromptRequest) -> Result<(), sacp::Error> {
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

        // Send prompt to agent
        self.agent
            .send_prompt(SendPromptArgs {
                content,
                should_continue_turn: None,
            })
            .await
            .map_err(|_e| sacp::Error::internal_error())?;

        Ok(())
    }
}

/// Handle agent update event and forward to ACP client
fn handle_update_event(
    update_event: UpdateEvent,
    session_id: SessionId,
    _request_cx: &JrRequestCx<PromptResponse>,
    cx: &mut JrConnectionCx<sacp::AgentToClient>,
) -> Result<(), sacp::Error> {
    let session_update = match update_event {
        UpdateEvent::AgentContent(ContentChunk::Text(text)) => {
            Some(SessionUpdate::AgentMessageChunk(SacpContentChunk::new(
                ContentBlock::Text(TextContent::new(text))
            )))
        },
        UpdateEvent::ToolCall(tool_call) => {
            let acp_tool_call = ToolCall::new(
                ToolCallId::new(tool_call.id),
                tool_call.tool_use_block.name.clone()
            )
            .kind(get_tool_kind(&tool_call.tool_use_block.name))
            .status(ToolCallStatus::InProgress)
            .content(get_tool_content(&tool_call.tool))
            .raw_input(Some(tool_call.tool_use_block.input.clone()));
            Some(SessionUpdate::ToolCall(acp_tool_call))
        },
        UpdateEvent::ToolCallFinished { tool_call, result } => {
            let (status, raw_output) = match result {
                ToolCallResult::Success(output) => (ToolCallStatus::Completed, serde_json::to_value(output).ok()),
                ToolCallResult::Error(_) => (ToolCallStatus::Failed, None),
                ToolCallResult::Cancelled => (ToolCallStatus::Failed, None),
            };
            Some(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_call.id),
                ToolCallUpdateFields::new()
                    .status(Some(status))
                    .title(Some(tool_call.tool_use_block.name.clone()))
                    .kind(Some(get_tool_kind(&tool_call.tool_use_block.name)))
                    .raw_input(Some(tool_call.tool_use_block.input.clone()))
                    .raw_output(raw_output)
            )))
        },
        _ => None,
    };

    if let Some(update) = session_update {
        cx.send_notification(SessionNotification::new(session_id, update))?;
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
            BuiltInToolName::Summary => ToolKind::Other,
        }
    } else {
        ToolKind::Other
    }
}

/// Get content for tool calls based on tool type
fn get_tool_content(tool: &Tool) -> Vec<ToolCallContent> {
    match &tool.kind {
        AgentToolKind::BuiltIn(BuiltInTool::FileWrite(fs_write)) => {
            let path = fs_write.path();
            let (old_text, new_text) = match fs_write {
                FsWrite::Create(create) => (None, create.content.clone()),
                FsWrite::StrReplace(str_replace) => (Some(str_replace.old_str.clone()), str_replace.new_str.clone()),
                FsWrite::Insert(_) => return vec![],
            };

            vec![ToolCallContent::Diff(Diff::new(
                path,
                new_text
            ).old_text(old_text))]
        },
        _ => vec![],
    }
}

/// Handle tool use approval request
fn handle_approval_request(
    id: String,
    tool_use: ToolUseBlock,
    session_id: SessionId,
    agent: AgentHandle,
    _request_cx: &JrRequestCx<PromptResponse>,
    cx: &mut JrConnectionCx<sacp::AgentToClient>,
) -> Result<(), sacp::Error> {
    let permission_request = RequestPermissionRequest::new(
        session_id,
        ToolCallUpdate::new(
            ToolCallId::new(tool_use.tool_use_id.clone()),
            ToolCallUpdateFields::new()
                .status(Some(ToolCallStatus::Pending))
                .title(Some(tool_use.name.clone()))
                .raw_input(Some(tool_use.input.clone()))
        ),
        vec![
            PermissionOption::new(
                PermissionOptionId::new("allow"),
                "Allow".to_string(),
                PermissionOptionKind::AllowOnce
            ),
            PermissionOption::new(
                PermissionOptionId::new("deny"),
                "Deny".to_string(),
                PermissionOptionKind::RejectOnce
            ),
        ]
    );

    cx
        .send_request(permission_request)
        .on_receiving_result(|result| async move {
            info!("Permission request result: {:?}", result);
            let approval_result = match result {
                Ok(response) => match &response.outcome {
                    RequestPermissionOutcome::Selected(option_id) => {
                        if option_id.option_id.0.as_ref() == "allow" {
                            agent::protocol::ApprovalResult::Approve
                        } else {
                            agent::protocol::ApprovalResult::Deny { reason: None }
                        }
                    },
                    RequestPermissionOutcome::Cancelled => agent::protocol::ApprovalResult::Deny {
                        reason: Some("Cancelled".to_string()),
                    },
                    _ => agent::protocol::ApprovalResult::Deny {
                        reason: Some("Unknown outcome".to_string()),
                    },
                },
                Err(_) => agent::protocol::ApprovalResult::Deny {
                    reason: Some("Request failed".to_string()),
                },
            };

            agent
                .send_tool_use_approval_result(agent::protocol::SendApprovalResultArgs {
                    id,
                    result: approval_result,
                })
                .await
                .map_err(|e| {
                    error!("Failed to send approval result to agent: {:?}", e);
                    e
                })
                .ok();
            Ok(())
        })?;

    Ok(())
}

/// Entry point for SACP agent
pub async fn execute(os: &mut Os) -> Result<ExitCode> {
    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    // Create session manager
    let sessions = Arc::new(Mutex::new(HashMap::new()));
    let os = Arc::new(Mutex::new(os.clone()));

    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            // Create SACP connection with handlers
            AgentToClient::builder()
                .name("kiro-cli-agent")
                // Handle initialize request
                .on_receive_request(
                    async |_request: InitializeRequest, request_cx: JrRequestCx<InitializeResponse>, _cx: JrConnectionCx<sacp::AgentToClient>| {
                        request_cx.respond(InitializeResponse::new(ProtocolVersion::LATEST)
                            .agent_capabilities(AgentCapabilities::default()))
                    },
                    sacp::on_receive_request!()
                )
                // Handle new_session request
                .on_receive_request(
                    {
                        let sessions = Arc::clone(&sessions);
                        let os = Arc::clone(&os);
                        async move |_request: NewSessionRequest, request_cx: JrRequestCx<NewSessionResponse>, _cx: JrConnectionCx<sacp::AgentToClient>| {
                            let session_id = SessionId::new(uuid::Uuid::new_v4().to_string());
                            let os_clone = os.lock().unwrap().clone();
                            let session = Arc::new(AcpSession::new(os_clone).await.map_err(|_e| sacp::Error::internal_error())?);
                            sessions.lock().unwrap().insert(session_id.clone(), session);

                            request_cx.respond(NewSessionResponse::new(session_id))
                        }
                    },
                    sacp::on_receive_request!()
                )
                // Handle prompt request
                .on_receive_request(
                    {
                        let sessions = Arc::clone(&sessions);
                        async move |request: PromptRequest, request_cx: JrRequestCx<PromptResponse>, cx: JrConnectionCx<sacp::AgentToClient>| {
                            let session = sessions.lock().expect("not poisoned").get(&request.session_id).cloned();

                            match session {
                                Some(session) => session.handle_prompt_request(request, request_cx, cx).await,
                                None => request_cx.respond_with_error(sacp::Error::invalid_request()),
                            }
                        }
                    },
                    sacp::on_receive_request!()
                )
                // Handle cancel notification
                .on_receive_notification(
                    async |_notification: CancelNotification, _cx: JrConnectionCx<sacp::AgentToClient>| {
                        // TODO: Implement cancellation
                        Ok(())
                    },
                    sacp::on_receive_notification!()
                )
                // Run the connection
                .serve(sacp::ByteStreams::new(outgoing, incoming))
                .await
                .map_err(|e| eyre::eyre!("Connection error: {}", e))
        })
        .await?;

    Ok(ExitCode::SUCCESS)
}
