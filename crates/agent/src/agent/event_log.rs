//! Event-sourced log for conversation state changes.
//!
//! All conversation state mutations are captured as append-only log entries.
//! The `Vec<Message>` is derived by replaying the log, not directly mutated.

use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};
use tracing::warn;

use super::agent_loop::types::{
    ContentBlock,
    Message,
    MessageMetadata,
    Role,
};
use super::compact::CompactStrategy;
use super::protocol::ToolCallResult;
use super::tools::Tool;
use super::tools::fs_read::backwards_compatibility::deserialize_tool_with_legacy_fallback;

/// Tool execution with its result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// The parsed tool. None if the tool use block failed to parse.
    #[serde(deserialize_with = "deserialize_tool_with_legacy_fallback")]
    pub tool: Option<Box<Tool>>,
    pub result: ToolCallResult,
}

/// Versioned log entry using tagged union pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum LogEntry {
    #[serde(rename = "v1")]
    V1(LogEntryV1),
}

/// V1 log entry variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum LogEntryV1 {
    /// User prompt added to conversation
    Prompt {
        message_id: String,
        content: Vec<ContentBlock>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<MessageMetadata>,
    },
    /// Assistant message (may contain text and/or tool uses)
    AssistantMessage {
        message_id: String,
        content: Vec<ContentBlock>,
    },
    /// Tool execution results
    ToolResults {
        message_id: String,
        /// Content sent as part of the user message
        content: Vec<ContentBlock>,
        /// Map from tool use id to tool result
        results: HashMap<String, ToolResult>,
    },
    /// Compaction snapshot - stores state after compaction applied
    Compaction {
        summary: String,
        strategy: CompactStrategy,
        messages_snapshot: Vec<Message>,
    },
    /// Reset conversation to a previous point
    ResetTo { target_index: usize },
    /// Cancels the last user prompt (removes the last user message from history)
    CancelledPrompt,
    /// Clear conversation - fresh start within same session
    Clear,
}

impl LogEntry {
    pub fn prompt(message_id: String, content: Vec<ContentBlock>, meta: Option<MessageMetadata>) -> Self {
        Self::V1(LogEntryV1::Prompt {
            message_id,
            content,
            meta,
        })
    }

    pub fn assistant_message(message_id: String, content: Vec<ContentBlock>) -> Self {
        Self::V1(LogEntryV1::AssistantMessage { message_id, content })
    }

    pub fn tool_results(message_id: String, content: Vec<ContentBlock>, results: HashMap<String, ToolResult>) -> Self {
        Self::V1(LogEntryV1::ToolResults {
            message_id,
            content,
            results,
        })
    }

    pub fn compaction(summary: String, strategy: CompactStrategy, messages_snapshot: Vec<Message>) -> Self {
        Self::V1(LogEntryV1::Compaction {
            summary,
            strategy,
            messages_snapshot,
        })
    }

    pub fn reset_to(target_index: usize) -> Self {
        Self::V1(LogEntryV1::ResetTo { target_index })
    }

    pub fn cancelled_prompt() -> Self {
        Self::V1(LogEntryV1::CancelledPrompt)
    }

    pub fn clear() -> Self {
        Self::V1(LogEntryV1::Clear)
    }

    /// Apply this log entry to update the messages vec incrementally.
    pub fn apply(&self, messages: &mut Vec<Message>, log: &EventLog) {
        match self {
            LogEntry::V1(LogEntryV1::Prompt {
                message_id,
                content,
                meta,
            }) => {
                messages.push(Message {
                    id: Some(message_id.clone()),
                    role: Role::User,
                    content: content.clone(),
                    meta: meta.clone(),
                });
            },
            LogEntry::V1(LogEntryV1::AssistantMessage { message_id, content }) => {
                messages.push(Message {
                    id: Some(message_id.clone()),
                    role: Role::Assistant,
                    content: content.clone(),
                    meta: None,
                });
            },
            LogEntry::V1(LogEntryV1::ToolResults {
                message_id, content, ..
            }) => {
                messages.push(Message {
                    id: Some(message_id.clone()),
                    role: Role::User,
                    content: content.clone(),
                    meta: None,
                });
            },
            LogEntry::V1(LogEntryV1::Compaction { messages_snapshot, .. }) => {
                *messages = messages_snapshot.clone();
            },
            LogEntry::V1(LogEntryV1::ResetTo { target_index }) => {
                *messages = log.derive_messages_up_to(*target_index);
            },
            LogEntry::V1(LogEntryV1::CancelledPrompt) => {
                if let Some(last_msg) = messages.last() {
                    if last_msg.role == Role::User {
                        messages.pop();
                    } else {
                        warn!(
                            "CancelledPrompt: expected last message to be user message, but found {:?}",
                            last_msg.role
                        );
                    }
                } else {
                    warn!("CancelledPrompt: no messages to cancel");
                }
            },
            LogEntry::V1(LogEntryV1::Clear) => {
                messages.clear();
            },
        }
    }
}

/// Append-only event log for conversation state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventLog {
    entries: Vec<LogEntry>,
}

impl EventLog {
    pub fn new(entries: Vec<LogEntry>) -> Self {
        Self { entries }
    }

    /// Append an entry to the log.
    pub fn append(&mut self, entry: LogEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Replay log to derive messages up to (exclusive) the given index.
    pub fn derive_messages_up_to(&self, end_index: usize) -> Vec<Message> {
        let entries = &self.entries[..end_index.min(self.entries.len())];

        // Find last compaction before end_index
        let (start_idx, mut messages) = entries
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, entry)| match entry {
                LogEntry::V1(LogEntryV1::Compaction { messages_snapshot, .. }) => {
                    Some((i + 1, messages_snapshot.clone()))
                },
                LogEntry::V1(_) => None,
            })
            .unwrap_or((0, Vec::new()));

        for entry in &entries[start_idx..] {
            entry.apply(&mut messages, self);
        }

        messages
    }

    /// Replay entire log to derive messages.
    pub fn derive_messages(&self) -> Vec<Message> {
        self.derive_messages_up_to(self.entries.len())
    }

    /// Get the latest compaction summary, if any.
    pub fn latest_summary(&self) -> Option<&str> {
        self.entries.iter().rev().find_map(|entry| match entry {
            LogEntry::V1(LogEntryV1::Compaction { summary, .. }) => Some(summary.as_str()),
            LogEntry::V1(_) => None,
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_content(s: &str) -> ContentBlock {
        ContentBlock::Text(s.to_string())
    }

    fn user_message(s: &str) -> Message {
        Message {
            id: Some("test-id".to_string()),
            role: Role::User,
            content: vec![text_content(s)],
            meta: None,
        }
    }

    fn assistant_msg(s: &str) -> Message {
        Message {
            id: Some("test-id".to_string()),
            role: Role::Assistant,
            content: vec![text_content(s)],
            meta: None,
        }
    }

    #[test]
    fn test_log_entry_serialization_roundtrip() {
        let entry = LogEntry::prompt("msg-1".to_string(), vec![text_content("hello")], None);
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: LogEntry = serde_json::from_str(&json).unwrap();

        assert!(json.contains(r#""version":"v1""#));
        assert!(json.contains(r#""kind":"Prompt""#));

        match parsed {
            LogEntry::V1(LogEntryV1::Prompt {
                message_id, content, ..
            }) => {
                assert_eq!(message_id, "msg-1");
                assert_eq!(content.len(), 1);
            },
            _other @ LogEntry::V1(_) => panic!("unexpected variant"),
        }
    }

    #[test]
    fn test_derive_messages() {
        let mut log = EventLog::new(Vec::new());

        log.append(LogEntry::prompt("msg-1".to_string(), vec![text_content("hello")], None));
        log.append(LogEntry::assistant_message("msg-2".to_string(), vec![text_content(
            "hi there",
        )]));
        log.append(LogEntry::prompt(
            "msg-3".to_string(),
            vec![text_content("how are you")],
            None,
        ));
        assert_eq!(log.len(), 3);

        let messages = log.derive_messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[0].id, Some("msg-1".to_string()));
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[1].id, Some("msg-2".to_string()));
        assert_eq!(messages[2].role, Role::User);
        assert_eq!(messages[2].id, Some("msg-3".to_string()));

        // Test CancelledPrompt
        log.append(LogEntry::cancelled_prompt());

        let messages_after_cancel = log.derive_messages();
        assert_eq!(
            messages_after_cancel.len(),
            2,
            "Should have 2 messages after cancelling msg-3"
        );
        assert_eq!(messages_after_cancel[0].role, Role::User);
        assert_eq!(messages_after_cancel[0].id, Some("msg-1".to_string()));
        assert_eq!(messages_after_cancel[1].role, Role::Assistant);
        assert_eq!(messages_after_cancel[1].id, Some("msg-2".to_string()));
        // msg-3 should be removed
    }

    #[test]
    fn test_derive_messages_from_compaction() {
        let mut log = EventLog::new(Vec::new());

        log.append(LogEntry::prompt(
            "old-1".to_string(),
            vec![text_content("old message 1")],
            None,
        ));
        log.append(LogEntry::assistant_message("old-2".to_string(), vec![text_content(
            "old response",
        )]));

        let snapshot = vec![user_message("summary context"), assistant_msg("acknowledged")];
        log.append(LogEntry::compaction(
            "Summary of conversation".to_string(),
            CompactStrategy::default_strategy(),
            snapshot,
        ));

        log.append(LogEntry::prompt(
            "new-1".to_string(),
            vec![text_content("new message")],
            None,
        ));

        let messages = log.derive_messages();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].content[0].text().unwrap().contains("summary context"));
    }

    #[test]
    fn test_deserialize_legacy_ls_tool_result() {
        use crate::agent::tools::fs_read::FsReadOperation;
        use crate::agent::tools::{
            BuiltInTool,
            ToolKind,
        };

        let json = r#"{
            "version": "v1",
            "kind": "ToolResults",
            "data": {
                "message_id": "test-msg",
                "content": [{"kind": "toolResult", "data": {"toolUseId": "tu1", "content": [{"kind": "text", "data": "ok"}], "status": "success"}}],
                "results": {
                    "tu1": {
                        "tool": {
                            "tool_use_purpose": "List directory",
                            "kind": {"BuiltIn": {"Ls": {"path": ".", "depth": null, "ignore": null}}}
                        },
                        "result": "Cancelled"
                    }
                }
            }
        }"#;
        let entry: LogEntry = serde_json::from_str(json).expect("should deserialize legacy Ls");
        let LogEntry::V1(LogEntryV1::ToolResults { results, .. }) = entry else {
            panic!("expected ToolResults");
        };
        let tool = results["tu1"].tool.as_ref().expect("tool should be Some");
        assert_eq!(tool.tool_use_purpose.as_deref(), Some("List directory"));
        let ToolKind::BuiltIn(BuiltInTool::FileRead(fs_read)) = &tool.kind else {
            panic!("expected FileRead, got {:?}", tool.kind);
        };
        assert_eq!(fs_read.operations.len(), 1);
        assert!(matches!(&fs_read.operations[0], FsReadOperation::Directory(d) if d.path == "."));
    }

    #[test]
    fn test_deserialize_legacy_image_read_tool_result() {
        use crate::agent::tools::fs_read::FsReadOperation;
        use crate::agent::tools::{
            BuiltInTool,
            ToolKind,
        };

        let json = r#"{
            "version": "v1",
            "kind": "ToolResults",
            "data": {
                "message_id": "test-msg",
                "content": [{"kind": "toolResult", "data": {"toolUseId": "tu2", "content": [{"kind": "text", "data": "ok"}], "status": "success"}}],
                "results": {
                    "tu2": {
                        "tool": {
                            "tool_use_purpose": "Read image",
                            "kind": {"BuiltIn": {"ImageRead": {"paths": ["/tmp/img.png"]}}}
                        },
                        "result": "Cancelled"
                    }
                }
            }
        }"#;
        let entry: LogEntry = serde_json::from_str(json).expect("should deserialize legacy ImageRead");
        let LogEntry::V1(LogEntryV1::ToolResults { results, .. }) = entry else {
            panic!("expected ToolResults");
        };
        let tool = results["tu2"].tool.as_ref().expect("tool should be Some");
        assert_eq!(tool.tool_use_purpose.as_deref(), Some("Read image"));
        let ToolKind::BuiltIn(BuiltInTool::FileRead(fs_read)) = &tool.kind else {
            panic!("expected FileRead, got {:?}", tool.kind);
        };
        assert_eq!(fs_read.operations.len(), 1);
        assert!(matches!(&fs_read.operations[0], FsReadOperation::Image(img) if img.paths == vec!["/tmp/img.png"]));
    }

    #[test]
    fn test_deserialize_legacy_file_read_with_ops() {
        use crate::agent::tools::fs_read::FsReadOperation;
        use crate::agent::tools::{
            BuiltInTool,
            ToolKind,
        };

        let json = r#"{
            "version": "v1",
            "kind": "ToolResults",
            "data": {
                "message_id": "test-msg",
                "content": [{"kind": "toolResult", "data": {"toolUseId": "tu3", "content": [{"kind": "text", "data": "ok"}], "status": "success"}}],
                "results": {
                    "tu3": {
                        "tool": {
                            "tool_use_purpose": "Read file",
                            "kind": {"BuiltIn": {"FileRead": {"ops": [{"path": "src/main.rs", "limit": 50, "offset": 10}]}}}
                        },
                        "result": "Cancelled"
                    }
                }
            }
        }"#;
        let entry: LogEntry = serde_json::from_str(json).expect("should deserialize legacy FileRead with ops");
        let LogEntry::V1(LogEntryV1::ToolResults { results, .. }) = entry else {
            panic!("expected ToolResults");
        };
        let tool = results["tu3"].tool.as_ref().expect("tool should be Some");
        assert_eq!(tool.tool_use_purpose.as_deref(), Some("Read file"));
        let ToolKind::BuiltIn(BuiltInTool::FileRead(fs_read)) = &tool.kind else {
            panic!("expected FileRead, got {:?}", tool.kind);
        };
        assert_eq!(fs_read.operations.len(), 1);
        assert!(
            matches!(&fs_read.operations[0], FsReadOperation::Line(f) if f.path == "src/main.rs" && f.limit == Some(50) && f.offset == Some(10))
        );
    }
}
