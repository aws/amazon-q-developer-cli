//! Handler for session management tool requests from the agent.
//!
//! Routes SessionToolRequest events to the SessionManager's orchestration
//! infrastructure (inbox, permissions, naming, groups).

use agent::AgentHandle;
use agent::tools::session::{
    GroupAction,
    MessagePriority,
    SessionFilter,
    SessionTool,
    SessionToolRequest,
    SessionToolResponse,
};
use agent::tools::{
    ToolExecutionOutput,
    ToolExecutionOutputItem,
};
use sacp::schema::SessionId;
use tracing::{
    error,
    info,
};

use super::session_manager::SessionManagerHandle;

/// Handle a session tool request from the agent.
pub async fn handle_session_tool_request(
    request: SessionToolRequest,
    session_tx: SessionManagerHandle,
    caller_session_id: SessionId,
    _agent: AgentHandle,
) {
    info!(
        caller = %caller_session_id,
        command = ?std::mem::discriminant(&request.request),
        "session_management tool invoked"
    );

    let result = match &request.request {
        SessionTool::SpawnSession {
            agent_name,
            task,
            name,
            role,
            group,
            persistent,
        } => {
            handle_spawn_session(
                &session_tx,
                &caller_session_id,
                agent_name,
                task,
                name.as_deref(),
                role.as_deref(),
                group.as_deref(),
                persistent.unwrap_or(false),
            )
            .await
        },
        SessionTool::SendMessage {
            target,
            message,
            priority,
        } => {
            let is_escalation = *priority == MessagePriority::Escalation;
            handle_send_message(
                &session_tx,
                &caller_session_id,
                target.as_deref(),
                message,
                is_escalation,
            )
            .await
        },
        SessionTool::ReadMessages { limit } => handle_read_messages(&session_tx, &caller_session_id, *limit).await,
        SessionTool::ListSessions { filter } => handle_list_sessions(&session_tx, *filter).await,
        SessionTool::GetSessionStatus { target, verbose } => {
            handle_get_session_status(&session_tx, target, verbose.unwrap_or(false)).await
        },
        SessionTool::Interrupt { target, message } => {
            handle_interrupt(&session_tx, &caller_session_id, target, message).await
        },
        SessionTool::InjectContext { target, context } => {
            handle_inject_context(&session_tx, &caller_session_id, target, context).await
        },
        SessionTool::ManageGroup {
            action,
            group,
            target,
            role,
            message,
        } => {
            handle_manage_group(
                &session_tx,
                &caller_session_id,
                *action,
                group.as_deref(),
                target.as_deref(),
                role.as_deref(),
                message.as_deref(),
            )
            .await
        },
        SessionTool::ReviveSession { target, task } => {
            handle_revive_session(&session_tx, &caller_session_id, target, task).await
        },
        SessionTool::RegisterPendingStages { group, pending_stages } => {
            session_tx
                .register_pending_stages(group.clone(), pending_stages.clone())
                .await;
            Ok("Pending stages registered".to_string())
        },
        SessionTool::WaitForGroup { group } => {
            let results = session_tx.wait_for_group_completion(group.clone()).await;
            let formatted: Vec<serde_json::Value> = results
                .iter()
                .map(|(name, result)| {
                    serde_json::json!({
                        "name": name,
                        "result": result.as_deref().unwrap_or("No result")
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "status": "completed",
                "group": group,
                "results": formatted
            })
            .to_string())
        },
    };

    let response = match result {
        Ok(output) => Ok(SessionToolResponse {
            output: ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(output)]),
        }),
        Err(e) => {
            error!("Session tool error: {}", e);
            Err(e)
        },
    };

    if let Err(e) = request.response_tx.send(response).await {
        error!("Failed to send session tool response: {}", e);
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_spawn_session(
    session_tx: &SessionManagerHandle,
    caller_session_id: &SessionId,
    agent_name: &str,
    task: &str,
    name: Option<&str>,
    role: Option<&str>,
    group: Option<&str>,
    persistent: bool,
) -> Result<String, String> {
    let result = session_tx
        .spawn_orchestrated_session(
            caller_session_id,
            agent_name.to_string(),
            task.to_string(),
            name.map(String::from),
            role.map(String::from),
            group.map(String::from),
            persistent,
        )
        .await
        .map_err(|e| format!("Failed to spawn session: {}", e))?;

    Ok(serde_json::json!({
        "session_id": result.session_id,
        "name": result.name,
        "status": "spawned",
        "next_step": "Tell the user what you started. Results arrive in your inbox automatically."
    })
    .to_string())
}

async fn handle_send_message(
    session_tx: &SessionManagerHandle,
    caller_session_id: &SessionId,
    target: Option<&str>,
    message: &str,
    is_escalation: bool,
) -> Result<String, String> {
    info!(
        from = %caller_session_id.to_string(),
        to = ?target,
        escalation = is_escalation,
        "Sending inter-session message"
    );

    session_tx
        .send_orchestration_message(caller_session_id, target, message, is_escalation)
        .await
        .map_err(|e| format!("{}", e))?;

    Ok(serde_json::json!({"status": "delivered", "target": target, "escalation": is_escalation}).to_string())
}

async fn handle_read_messages(
    session_tx: &SessionManagerHandle,
    session_id: &SessionId,
    limit: usize,
) -> Result<String, String> {
    let messages = session_tx
        .read_orchestration_messages(session_id, limit)
        .await
        .map_err(|e| format!("{}", e))?;

    let formatted: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "from_name": m.from_name,
                "from_session_id": m.from_session.to_string(),
                "message": m.message,
                "timestamp": format!("{:?}", m.timestamp),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "messages": formatted,
        "count": messages.len(),
    })
    .to_string())
}

async fn handle_list_sessions(
    session_tx: &SessionManagerHandle,
    filter: Option<SessionFilter>,
) -> Result<String, String> {
    let sessions = session_tx
        .list_orchestrated_sessions(filter)
        .await
        .map_err(|e| format!("{}", e))?;

    let formatted: Vec<serde_json::Value> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "session_id": s.session_id.to_string(),
                "name": s.name,
                "role": s.role,
                "agent_name": s.agent_name,
                "task": s.task,
                "status": s.status,
                "group": s.group,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "sessions": formatted,
        "count": sessions.len(),
    })
    .to_string())
}

async fn handle_get_session_status(
    session_tx: &SessionManagerHandle,
    target: &str,
    verbose: bool,
) -> Result<String, String> {
    let status = session_tx
        .get_orchestrated_session_status(target)
        .await
        .map_err(|e| format!("{}", e))?;

    if verbose {
        let live_activity = session_tx.get_session_live_activity(target).await.unwrap_or_default();

        Ok(serde_json::json!({
            "session_id": status.session_id.to_string(),
            "name": status.name,
            "role": status.role,
            "agent_name": status.agent_name,
            "task": status.task,
            "status": status.status,
            "group": status.group,
            "human_attached": status.human_attached,
            "last_activity": live_activity,
        })
        .to_string())
    } else {
        Ok(serde_json::json!({
            "session_id": status.session_id.to_string(),
            "name": status.name,
            "status": status.status,
            "task": status.task,
        })
        .to_string())
    }
}

async fn handle_interrupt(
    session_tx: &SessionManagerHandle,
    caller_session_id: &SessionId,
    target: &str,
    message: &str,
) -> Result<String, String> {
    info!(
        from = %caller_session_id.to_string(),
        to = target,
        "Interrupting session"
    );

    session_tx
        .interrupt_orchestrated_session(caller_session_id, target, message)
        .await
        .map_err(|e| format!("{}", e))?;

    Ok(serde_json::json!({"status": "interrupted", "target": target}).to_string())
}

async fn handle_inject_context(
    session_tx: &SessionManagerHandle,
    caller_session_id: &SessionId,
    target: &str,
    context: &str,
) -> Result<String, String> {
    info!(
        from = %caller_session_id.to_string(),
        to = target,
        "Injecting context"
    );

    session_tx
        .inject_orchestration_context(caller_session_id, target, context)
        .await
        .map_err(|e| format!("{}", e))?;

    Ok(serde_json::json!({"status": "injected", "target": target}).to_string())
}

async fn handle_manage_group(
    session_tx: &SessionManagerHandle,
    caller_session_id: &SessionId,
    action: GroupAction,
    group: Option<&str>,
    target: Option<&str>,
    role: Option<&str>,
    message: Option<&str>,
) -> Result<String, String> {
    session_tx
        .manage_orchestration_group(caller_session_id, action, group, target, role, message)
        .await
        .map_err(|e| format!("{}", e))
}

async fn handle_revive_session(
    session_tx: &SessionManagerHandle,
    caller_session_id: &SessionId,
    target: &str,
    task: &str,
) -> Result<String, String> {
    info!(
        from = %caller_session_id,
        target = target,
        "Reviving terminated session"
    );

    let result = session_tx
        .revive_orchestrated_session(caller_session_id, target, task)
        .await
        .map_err(|e| format!("{}", e))?;

    Ok(serde_json::json!({
        "session_id": result.session_id,
        "name": result.name,
        "status": "revived"
    })
    .to_string())
}
