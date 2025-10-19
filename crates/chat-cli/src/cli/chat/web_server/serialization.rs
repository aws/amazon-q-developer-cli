//! Conversation and worker metadata serialization for WebUI
//!
//! This module provides simplified JSON types for conversation history and worker metadata,
//! along with conversion functions from internal types.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::agent_env::{
    context_container::ConversationEntry,
    events::WorkerLifecycleState,
    worker::Worker,
};
use crate::cli::chat::{AssistantMessage, AssistantToolUse, UserMessage};
use crate::cli::chat::message::UserMessageContent;

/// Get current Unix timestamp
pub fn current_unix_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

/// Tool use information for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseJson {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
}

/// Conversation entry for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationEntryJson {
    UserMessage {
        content: String,
        timestamp: f64,
    },
    AssistantMessage {
        content: String,
        timestamp: f64,
    },
    ToolUse {
        content: String,
        tool_uses: Vec<ToolUseJson>,
        timestamp: f64,
    },
}

/// Worker metadata for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerMetadataJson {
    pub id: String,
    pub name: String,
    pub agent: String,
    pub state: WorkerLifecycleState,
}

/// Convert ConversationEntry to ConversationEntryJson
pub fn convert_conversation_entry(entry: &ConversationEntry) -> ConversationEntryJson {
    // Try user message first
    if let Some(user_msg) = &entry.user {
        let content = extract_user_content(user_msg);
        let timestamp = user_msg
            .timestamp
            .as_ref()
            .map(|dt| dt.timestamp() as f64)
            .unwrap_or_else(current_unix_timestamp);
        
        return ConversationEntryJson::UserMessage { content, timestamp };
    }

    // Try assistant message
    if let Some(assistant_msg) = &entry.assistant {
        match assistant_msg {
            AssistantMessage::Response { content, .. } => {
                let timestamp = current_unix_timestamp(); // Approximate
                return ConversationEntryJson::AssistantMessage { 
                    content: content.clone(), 
                    timestamp 
                };
            }
            AssistantMessage::ToolUse { tool_uses, .. } => {
                let content = format!("Using {} tool(s)", tool_uses.len());
                let tool_uses_json = tool_uses.iter().map(convert_tool_use).collect();
                let timestamp = current_unix_timestamp(); // Approximate
                return ConversationEntryJson::ToolUse {
                    content,
                    tool_uses: tool_uses_json,
                    timestamp,
                };
            }
        }
    }

    // Empty entry (shouldn't happen, but be defensive)
    ConversationEntryJson::UserMessage {
        content: String::new(),
        timestamp: current_unix_timestamp(),
    }
}

/// Extract user message content as string
fn extract_user_content(user_msg: &UserMessage) -> String {
    match &user_msg.content {
        UserMessageContent::Prompt { prompt } => prompt.clone(),
        UserMessageContent::CancelledToolUses { prompt, .. } => {
            prompt.clone().unwrap_or_else(|| "[Cancelled]".to_string())
        }
        UserMessageContent::ToolUseResults { .. } => "[Tool Results]".to_string(),
    }
}

/// Convert AssistantToolUse to ToolUseJson
fn convert_tool_use(tool_use: &AssistantToolUse) -> ToolUseJson {
    ToolUseJson {
        tool_name: tool_use.name.clone(),
        tool_input: tool_use.args.clone(),
    }
}

/// Convert Worker to WorkerMetadataJson
pub fn convert_worker_metadata(worker: &Worker) -> WorkerMetadataJson {
    let state = *worker.lifecycle_state.lock().unwrap();
    
    // Get agent name from task_metadata, default to "default" if not set
    let agent = worker
        .task_metadata
        .lock()
        .unwrap()
        .get("agent")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    WorkerMetadataJson {
        id: worker.id.to_string(),
        name: worker.name.clone(),
        agent,
        state,
    }
}
