//! V1 → V2 conversation export.
//!
//! Converts V1 `ConversationState` (SQLite) into V2 session files (`.json` + `.jsonl`).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use agent::agent_loop::types::{
    ContentBlock,
    ImageBlock as AgentImageBlock,
    ImageFormat as AgentImageFormat,
    ImageSource as AgentImageSource,
    MessageMetadata,
    ToolResultBlock,
    ToolResultContentBlock,
    ToolResultStatus as AgentToolResultStatus,
    ToolUseBlock,
};
use agent::event_log::LogEntry;
use agent::permissions::RuntimePermissions;
use agent::types::ConversationMetadata;
use chat_cli_v2::agent::rts::RtsStateSnapshot;
use chat_cli_v2::agent::session::legacy_compat::{
    LegacyExportError,
    LegacySessionExporter,
    LegacySessionInfo,
};
use chat_cli_v2::agent::session::{
    SessionData,
    SessionState,
    SessionStateV1,
    create_session_title,
};
use chrono::Utc;
use tracing::debug;
use uuid::Uuid;

use super::conversation::ConversationState;
use super::message::{
    AssistantMessage,
    ToolUseResult,
    ToolUseResultBlock,
    UserMessage,
    UserMessageContent,
};
use crate::api_client::model::{
    ImageBlock as V1ImageBlock,
    ImageFormat as V1ImageFormat,
    ImageSource as V1ImageSource,
};
use crate::database::Database;

/// V1 session exporter backed by the shared SQLite database.
#[derive(Debug)]
pub struct LegacySessionExporterImpl {
    database: Arc<Database>,
}

impl LegacySessionExporterImpl {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl LegacySessionExporter for LegacySessionExporterImpl {
    fn list_sessions(&self, cwd: &Path) -> Result<Vec<LegacySessionInfo>, LegacyExportError> {
        let convos = self
            .database
            .list_conversations_by_path(cwd)
            .map_err(|e| LegacyExportError {
                message: "failed to list V1 conversations".into(),
                source: Some(Box::new(e)),
            })?;

        Ok(convos
            .into_iter()
            .map(|(id, state, _created, updated)| {
                let title = first_user_prompt_title(&state).map(|t| format!("(legacy) {t}"));
                LegacySessionInfo {
                    conversation_id: id,
                    cwd: cwd.to_path_buf(),
                    title,
                    updated_at: chrono::DateTime::from_timestamp_millis(updated).unwrap_or_default(),
                    // HistoryEntry contains both the user and assistant message together, so
                    // multiply by 2
                    message_count: state.history().len() * 2,
                }
            })
            .collect())
    }

    /// Export a V1 conversation to V2 session format on disk.
    ///
    /// ## What is preserved
    ///
    /// - **Conversation history** (`ConversationState::history`): Each `HistoryEntry`
    ///   (user/assistant pair) is converted to V2 `LogEntry` pairs. User prompts, tool use results,
    ///   cancelled tool uses, and assistant responses/tool calls are all faithfully converted.
    /// - **`additional_context`**: In V1, populated from `UserPromptSubmit` hook output and baked
    ///   into each history message via `content_with_context()`. Preserved because it was part of
    ///   the text the model saw and responded to.
    /// - **`timestamp`**: In V1, each `UserMessage` carries the wall-clock time of the original
    ///   prompt, formatted into the message text via `content_with_context()`. Preserved because it
    ///   was part of the text the model saw. Note: V2 does not inject timestamps into prompts, so
    ///   only imported V1 messages will have them.
    /// - **Compaction summary** (`ConversationState::latest_summary`): Injected as a context entry
    ///   in the first user prompt's text, since V2 has no other way to recover it from V1 state.
    /// - **Model info** (`ConversationState::model_info`): Copied into `SessionData.session_state`
    ///   for display in the session picker.
    /// - **Images**: Image attachments on `Prompt` messages are preserved.
    ///
    /// ## What is intentionally omitted
    ///
    /// - **`env_context`**: In V1, each `UserMessage` carries an `env_state` (OS, cwd, env vars)
    ///   sent as a separate `UserInputMessageContext` API field, not part of the text content. V2
    ///   does not use this field.
    /// - **`agent_name`**: V1 does not persist agent name in `ConversationState` (the `agents`
    ///   field is `#[serde(skip)]`). Set to `None`.
    /// - **`tools`, `context_manager`, `tool_manager`**: Runtime state rebuilt by V2.
    /// - **`tangent_state`**: Tangent mode checkpoints are not migrated.
    /// - **`transcript`**: Terminal display history, not relevant to V2.
    /// - **`request_metadata`**: Per-turn API metadata
    fn export_session(&self, conversation_id: &str, sessions_dir: &Path) -> Result<(), LegacyExportError> {
        // Idempotent: skip if already exported
        if chat_cli_v2::agent::session::session_exists(sessions_dir, conversation_id) {
            debug!(?conversation_id, "session already exists, skipping export");
            return Ok(());
        }

        // Acquire a session lock to prevent concurrent exports
        let _lock = chat_cli_v2::agent::session::acquire_lock(sessions_dir, conversation_id).map_err(|e| {
            LegacyExportError {
                message: format!("failed to acquire lock for session {conversation_id}"),
                source: Some(Box::new(e)),
            }
        })?;

        // Safety: re-check after acquiring lock since another process may have exported
        if chat_cli_v2::agent::session::session_exists(sessions_dir, conversation_id) {
            return Ok(());
        }

        let (cwd, state) = self
            .database
            .get_conversation_by_id_with_cwd(conversation_id)
            .map_err(|e| LegacyExportError {
                message: format!("failed to read V1 conversation {conversation_id}"),
                source: Some(Box::new(e)),
            })?
            .ok_or_else(|| LegacyExportError {
                message: format!("V1 conversation not found: {conversation_id}"),
                source: None,
            })?;

        write_v1_session(&state, conversation_id, &cwd, sessions_dir, None)
    }

    fn try_export_from_json(
        &self,
        json_content: &str,
        session_id: &str,
        cwd: &Path,
        sessions_dir: &Path,
        imported_from: Option<&Path>,
    ) -> Result<(), LegacyExportError> {
        let state: ConversationState = serde_json::from_str(json_content).map_err(|e| LegacyExportError {
            message: "failed to deserialize as V1 ConversationState".into(),
            source: Some(Box::new(e)),
        })?;

        if chat_cli_v2::agent::session::session_exists(sessions_dir, session_id) {
            return Ok(());
        }

        let _lock =
            chat_cli_v2::agent::session::acquire_lock(sessions_dir, session_id).map_err(|e| LegacyExportError {
                message: format!("failed to acquire lock for session {session_id}"),
                source: Some(Box::new(e)),
            })?;

        if chat_cli_v2::agent::session::session_exists(sessions_dir, session_id) {
            return Ok(());
        }

        write_v1_session(&state, session_id, &cwd.to_string_lossy(), sessions_dir, imported_from)
    }
}

/// Shared helper: convert a V1 `ConversationState` and write V2 session files to disk.
fn write_v1_session(
    state: &ConversationState,
    conversation_id: &str,
    cwd: &str,
    sessions_dir: &Path,
    imported_from: Option<&Path>,
) -> Result<(), LegacyExportError> {
    let (log_entries, title) = convert_conversation(state);

    let model_info = state
        .model_info
        .as_ref()
        .map(|m| chat_cli_v2::cli::chat::legacy::model::ModelInfo {
            model_name: m.model_name.clone(),
            description: m.description.clone(),
            model_id: m.model_id.clone(),
            context_window_tokens: m.context_window_tokens,
            rate_multiplier: m.rate_multiplier,
            rate_unit: m.rate_unit.clone(),
        });
    let session_data = SessionData {
        session_id: conversation_id.to_string(),
        cwd: PathBuf::from(cwd),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        title,
        exported_from_v1: true,
        imported_from: imported_from.map(|p| p.to_string_lossy().into_owned()),
        session_state: SessionState::V1(SessionStateV1 {
            conversation_metadata: ConversationMetadata::default(),
            rts_model_state: RtsStateSnapshot {
                conversation_id: conversation_id.to_string(),
                model_info,
                context_usage_percentage: None,
            },
            permissions: RuntimePermissions::default().with_cwd(cwd),
            agent_name: None,
        }),
    };

    fs::create_dir_all(sessions_dir).map_err(|e| LegacyExportError {
        message: format!("failed to create sessions directory {}", sessions_dir.display()),
        source: Some(Box::new(e)),
    })?;

    let log_path = sessions_dir.join(format!("{conversation_id}.jsonl"));
    let meta_path = sessions_dir.join(format!("{conversation_id}.json"));
    let meta_content = serde_json::to_string_pretty(&session_data).map_err(|e| LegacyExportError {
        message: "failed to serialize session metadata".into(),
        source: Some(Box::new(e)),
    })?;

    let mut created_files: Vec<&Path> = Vec::new();
    let result = (|| -> Result<(), LegacyExportError> {
        let mut log_file = fs::File::create(&log_path).map_err(|e| LegacyExportError {
            message: format!("failed to create log file {}", log_path.display()),
            source: Some(Box::new(e)),
        })?;
        created_files.push(&log_path);

        for entry in &log_entries {
            let line = serde_json::to_string(entry).map_err(|e| LegacyExportError {
                message: "failed to serialize log entry".into(),
                source: Some(Box::new(e)),
            })?;
            writeln!(log_file, "{line}").map_err(|e| LegacyExportError {
                message: "failed to write log entry".into(),
                source: Some(Box::new(e)),
            })?;
        }

        fs::write(&meta_path, &meta_content).map_err(|e| LegacyExportError {
            message: format!("failed to write session metadata {}", meta_path.display()),
            source: Some(Box::new(e)),
        })?;
        created_files.push(&meta_path);

        Ok(())
    })();

    if result.is_err() {
        for path in &created_files {
            let _ = fs::remove_file(path);
        }
    }

    result
}

/// Convert a V1 ConversationState into V2 log entries and an optional title.
fn convert_conversation(state: &ConversationState) -> (Vec<LogEntry>, Option<String>) {
    let history = state.history();
    let summary = state.latest_summary();
    let mut entries = Vec::new();
    let mut title = None;
    let mut is_first_prompt = true;

    for entry in history.iter() {
        // Convert user message, injecting compaction summary into the first prompt
        let user_entry = convert_user_message(&entry.user, if is_first_prompt { summary } else { None });
        is_first_prompt = false;

        if title.is_none()
            && let LogEntry::V1(agent::event_log::LogEntryV1::Prompt { ref content, .. }) = user_entry
        {
            title = create_session_title(content);
        }
        entries.push(user_entry);

        // Convert assistant message
        entries.push(convert_assistant_message(&entry.assistant));
    }

    (entries, title)
}

/// Convert a V1 UserMessage into a V2 log entry.
///
/// If `compaction_summary` is provided, it is injected as a context entry
/// in the first prompt's text, mirroring how V1 injects it via `context_messages()`.
fn convert_user_message(msg: &UserMessage, compaction_summary: Option<&str>) -> LogEntry {
    let msg_id = Uuid::new_v4().to_string();

    match &msg.content {
        UserMessageContent::Prompt { prompt } => {
            let mut content = Vec::new();

            if !prompt.is_empty() {
                content.push(ContentBlock::Text(prompt.clone()));
            }

            // Append images if present
            if let Some(images) = &msg.images {
                for img in images {
                    content.push(ContentBlock::Image(convert_image(img)));
                }
            }

            // Build structured metadata from V1 context fields
            let meta = build_prompt_meta(&msg.additional_context, msg.timestamp.as_ref(), compaction_summary);

            LogEntry::prompt(msg_id, content, meta)
        },
        UserMessageContent::ToolUseResults { tool_use_results } => {
            LogEntry::tool_results(msg_id, convert_tool_results(tool_use_results), HashMap::new())
        },
        UserMessageContent::CancelledToolUses {
            prompt,
            tool_use_results,
        } => {
            // V1 sends cancelled tool results + optional prompt as a single user message.
            // Combine them into one ToolResults entry to maintain user/assistant alternation.
            let mut content = convert_tool_results(tool_use_results);
            if let Some(prompt) = prompt
                && !prompt.is_empty()
            {
                content.push(ContentBlock::Text(prompt.clone()));
            }

            LogEntry::tool_results(msg_id, content, HashMap::new())
        },
    }
}

/// Convert a V1 AssistantMessage into a V2 log entry.
fn convert_assistant_message(msg: &AssistantMessage) -> LogEntry {
    let (msg_id, content) = match msg {
        AssistantMessage::Response { message_id, content, .. } => {
            let id = message_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
            (id, vec![ContentBlock::Text(content.clone())])
        },
        AssistantMessage::ToolUse {
            message_id,
            content,
            tool_uses,
            ..
        } => {
            let id = message_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
            let mut blocks = Vec::new();
            if !content.is_empty() {
                blocks.push(ContentBlock::Text(content.clone()));
            }
            for tu in tool_uses {
                blocks.push(ContentBlock::ToolUse(ToolUseBlock {
                    tool_use_id: tu.id.clone(),
                    name: tu.name.clone(),
                    input: tu.args.clone(),
                }));
            }
            (id, blocks)
        },
    };

    LogEntry::assistant_message(msg_id, content)
}

/// Convert V1 tool use results into V2 content blocks.
fn convert_tool_results(results: &[ToolUseResult]) -> Vec<ContentBlock> {
    results
        .iter()
        .map(|r| {
            ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: r.tool_use_id.clone(),
                content: r
                    .content
                    .iter()
                    .map(|b| match b {
                        ToolUseResultBlock::Json(v) => ToolResultContentBlock::Json(v.clone()),
                        ToolUseResultBlock::Text(s) => ToolResultContentBlock::Text(s.clone()),
                    })
                    .collect(),
                status: match r.status {
                    crate::api_client::model::ToolResultStatus::Error => AgentToolResultStatus::Error,
                    crate::api_client::model::ToolResultStatus::Success => AgentToolResultStatus::Success,
                },
            })
        })
        .collect()
}

/// Build the prompt text, preserving V1's `content_with_context()` formatting.
///
/// V1 bakes `additional_context` (hook output) and `timestamp` into each history
/// message sent to the model. We must preserve these so the exported conversation
/// matches what the model actually saw and responded to.
/// Build structured metadata from V1 context fields.
fn build_prompt_meta(
    additional_context: &str,
    timestamp: Option<&chrono::DateTime<chrono::FixedOffset>>,
    compaction_summary: Option<&str>,
) -> Option<MessageMetadata> {
    use super::conversation::{
        CONTEXT_ENTRY_END_HEADER,
        CONTEXT_ENTRY_START_HEADER,
    };

    let mut ctx = String::new();

    if let Some(summary) = compaction_summary {
        ctx.push_str(CONTEXT_ENTRY_START_HEADER);
        ctx.push_str("This summary contains ALL relevant information from our previous conversation including tool uses, results, code analysis, and file operations. YOU MUST reference this information when answering questions and explicitly acknowledge specific details from the summary when they're relevant to the current question.\n\nSUMMARY CONTENT:\n");
        ctx.push_str(summary);
        ctx.push('\n');
        ctx.push_str(CONTEXT_ENTRY_END_HEADER);
    }

    if !additional_context.is_empty() {
        ctx.push_str(additional_context);
        ctx.push('\n');
    }

    let ts = timestamp.map(|t| t.with_timezone(&chrono::Utc));

    if ts.is_none() && ctx.is_empty() {
        return None;
    }

    Some(MessageMetadata {
        timestamp: ts,
        additional_context: ctx.trim().to_string(),
    })
}

/// Convert a V1 ImageBlock to the agent crate's ImageBlock.
fn convert_image(img: &V1ImageBlock) -> AgentImageBlock {
    AgentImageBlock {
        format: match img.format {
            V1ImageFormat::Gif => AgentImageFormat::Gif,
            V1ImageFormat::Jpeg => AgentImageFormat::Jpeg,
            V1ImageFormat::Png => AgentImageFormat::Png,
            V1ImageFormat::Webp => AgentImageFormat::Webp,
        },
        source: match &img.source {
            V1ImageSource::Bytes(b) => AgentImageSource::Bytes(b.clone()),
            _ => AgentImageSource::Bytes(Vec::new()),
        },
    }
}

/// Derive a title from the first user prompt in a V1 conversation.
fn first_user_prompt_title(state: &ConversationState) -> Option<String> {
    let entry = state.history().front()?;
    if let UserMessageContent::Prompt { prompt } = &entry.user.content {
        create_session_title(&[ContentBlock::Text(prompt.clone())])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use agent::agent_loop::types::Role;
    use agent::detect_invariant_violations;
    use agent::event_log::EventLog;
    use chat_cli_v2::agent::session::legacy_compat::LegacySessionExporter;
    use chat_cli_v2::agent::session::{
        SessionData,
        SessionDb,
        session_exists,
    };
    use tempfile::TempDir;

    use super::*;

    const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/cli/chat/v1_export/fixtures");

    /// Insert a fixture into an in-memory database, run export_session, and return
    /// the exported SessionData and derived messages.
    async fn export_fixture(name: &str) -> (ConversationState, SessionData, Vec<agent::agent_loop::types::Message>) {
        let fixture_path = format!("{FIXTURES_DIR}/{name}");
        let raw = std::fs::read_to_string(&fixture_path).unwrap();
        let state: ConversationState = serde_json::from_str(&raw).unwrap();

        let mut db = crate::database::Database::new_default().await.unwrap();
        let cwd = "/test/v1_export";
        db.set_conversation_by_path(cwd, &state).unwrap();

        let conversation_id = state.conversation_id().to_string();
        let exporter = LegacySessionExporterImpl::new(Arc::new(db));

        let sessions_dir = TempDir::new().unwrap();
        exporter.export_session(&conversation_id, sessions_dir.path()).unwrap();

        assert!(
            session_exists(sessions_dir.path(), &conversation_id),
            "session_exists should be true after export"
        );

        let session_db = SessionDb::load_with_sessions_dir(sessions_dir.path(), &conversation_id, None).unwrap();
        let session_data = session_db.session();
        let entries = session_db.load_log_entries().unwrap();
        let messages = EventLog::new(entries).derive_messages();

        (state, session_data, messages)
    }

    /// 10 history entries, no compaction summary:
    ///   [0] Prompt("Create hello world...write them to files") -> ToolUse(3 tools)
    ///   [1] ToolUseResults(3) -> Response
    ///   [2] Prompt("use bash tool to echo hello world") -> ToolUse(1)
    ///   [3] ToolUseResults(1) -> Response
    ///   [4] Prompt("summarize our conversation") -> Response
    ///   [5] Prompt("now use rust") -> ToolUse(1)
    ///   [6] ToolUseResults(1) -> ToolUse(1)  (chained)
    ///   [7] ToolUseResults(1) -> Response
    ///   [8] Prompt("try it for me") -> ToolUse(1)
    ///   [9] ToolUseResults(1) -> Response
    #[tokio::test]
    async fn test_basic_tool_use() {
        let (state, session_data, messages) = export_fixture("basic_tool_use.json").await;

        assert_eq!(session_data.session_id, state.conversation_id());
        assert!(session_data.exported_from_v1);
        assert_eq!(messages.len(), 20); // 10 history pairs

        // First message should have structured metadata (timestamp from V1)
        let first_meta = messages[0].meta.as_ref().expect("first message should have meta");
        assert!(first_meta.timestamp.is_some(), "meta should have timestamp");

        // Content should be the raw user prompt, not context-wrapped
        let first_text = messages[0].content.iter().find_map(|b| b.text()).unwrap();
        assert!(
            !first_text.contains("--- CONTEXT ENTRY BEGIN ---"),
            "content should not contain context entries, got: {first_text}"
        );

        // Session title should be derived from the raw prompt
        let title = session_data.title.as_deref().unwrap();
        assert!(
            title.starts_with("Create hello world"),
            "title should start with user prompt, got: {title}"
        );

        let violations = detect_invariant_violations(&messages);
        assert!(violations.is_valid(), "invariant violations: {violations:?}");
    }

    /// 5 history entries, HAS compaction summary (4375 chars):
    ///   [0] Prompt(json blob) -> Response
    ///   [1] Prompt("what about external uses") -> ToolUse(2)
    ///   [2] ToolUseResults(2) -> ToolUse(2)  (chained)
    ///   [3] ToolUseResults(2) -> Response
    ///   [4] Prompt("what have we talked about so far?") -> Response
    #[tokio::test]
    async fn test_with_compaction_summary() {
        let (state, session_data, messages) = export_fixture("with_compaction_summary.json").await;

        assert!(state.latest_summary().is_some());
        assert!(session_data.exported_from_v1);
        assert_eq!(messages.len(), 10);

        // Summary must be in the first user message's meta
        let first_meta = messages[0].meta.as_ref().expect("first message should have meta");
        assert!(
            first_meta.additional_context.contains("SUMMARY CONTENT:"),
            "first user message meta should contain compaction summary"
        );

        // Second user message should NOT have summary
        let second_user = messages.iter().filter(|m| m.role == Role::User).nth(1).unwrap();
        let has_summary = second_user
            .meta
            .as_ref()
            .map_or(false, |m| m.additional_context.contains("SUMMARY CONTENT:"));
        assert!(!has_summary, "only the first user message should contain the summary");

        let violations = detect_invariant_violations(&messages);
        assert!(violations.is_valid(), "invariant violations: {violations:?}");
    }

    /// 6 history entries, no compaction summary, has CancelledToolUses:
    ///   [0] Prompt("hello") -> Response
    ///   [1] Prompt("show some cool bash tool calls") -> ToolUse(1)
    ///   [2] ToolUseResults(1) -> ToolUse(1)  (chained)
    ///   [3] ToolUseResults(1) -> ToolUse(1)  (chained)
    ///   [4] ToolUseResults(1) -> ToolUse(1)  (chained)
    ///   [5] CancelledToolUses(1 result, prompt="thats cool") -> Response
    #[tokio::test]
    async fn test_with_cancelled_tool_uses() {
        let (_state, session_data, messages) = export_fixture("with_cancelled_tool_uses.json").await;

        assert!(session_data.exported_from_v1);
        assert_eq!(messages.len(), 12);

        // The cancelled entry (last user message before final response) should have
        // both a ToolResult and Text block merged into a single user message
        let last_user = messages.iter().rev().find(|m| m.role == Role::User).unwrap();
        assert!(
            last_user
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult(_))),
            "cancelled entry should have tool result"
        );
        assert!(
            last_user.content.iter().any(|b| matches!(b, ContentBlock::Text(_))),
            "cancelled entry should have prompt text"
        );

        let violations = detect_invariant_violations(&messages);
        assert!(violations.is_valid(), "invariant violations: {violations:?}");
    }

    /// 6 history entries, no compaction summary, pure multi-tool chains:
    ///   [0] Prompt("research internal amazon uses for kiro cli ACP") -> ToolUse(2)
    ///   [1] ToolUseResults(2) -> ToolUse(2)  (chained)
    ///   [2] ToolUseResults(2) -> Response
    ///   [3] Prompt("what about external uses") -> ToolUse(2)
    ///   [4] ToolUseResults(2) -> ToolUse(2)  (chained)
    ///   [5] ToolUseResults(2) -> Response
    #[tokio::test]
    async fn test_tool_use_chains() {
        let (_state, session_data, messages) = export_fixture("tool_use_chains.json").await;

        assert!(session_data.exported_from_v1);
        assert_eq!(messages.len(), 12);

        let violations = detect_invariant_violations(&messages);
        assert!(violations.is_valid(), "invariant violations: {violations:?}");
    }

    #[tokio::test]
    async fn test_export_is_idempotent() {
        let fixture_path = format!("{FIXTURES_DIR}/basic_tool_use.json");
        let raw = std::fs::read_to_string(&fixture_path).unwrap();
        let state: ConversationState = serde_json::from_str(&raw).unwrap();

        let mut db = crate::database::Database::new_default().await.unwrap();
        db.set_conversation_by_path("/test/v1_export", &state).unwrap();

        let conversation_id = state.conversation_id().to_string();
        let exporter = LegacySessionExporterImpl::new(Arc::new(db));
        let sessions_dir = TempDir::new().unwrap();

        exporter.export_session(&conversation_id, sessions_dir.path()).unwrap();
        let first = SessionDb::load_with_sessions_dir(sessions_dir.path(), &conversation_id, None).unwrap();
        let first_data = first.session();
        let first_entries = first.load_log_entries().unwrap();
        drop(first);

        // Second export should be a no-op
        exporter.export_session(&conversation_id, sessions_dir.path()).unwrap();
        let second = SessionDb::load_with_sessions_dir(sessions_dir.path(), &conversation_id, None).unwrap();
        let second_data = second.session();
        let second_entries = second.load_log_entries().unwrap();

        assert_eq!(first_data.session_id, second_data.session_id);
        assert_eq!(
            first_entries.len(),
            second_entries.len(),
            "idempotent export should not add entries"
        );
    }

    #[tokio::test]
    async fn test_export_cleans_up_on_error() {
        let fixture_path = format!("{FIXTURES_DIR}/basic_tool_use.json");
        let raw = std::fs::read_to_string(&fixture_path).unwrap();
        let state: ConversationState = serde_json::from_str(&raw).unwrap();

        let mut db = crate::database::Database::new_default().await.unwrap();
        db.set_conversation_by_path("/test/v1_export", &state).unwrap();

        let conversation_id = state.conversation_id().to_string();
        let exporter = LegacySessionExporterImpl::new(Arc::new(db));
        let sessions_dir = TempDir::new().unwrap();

        // Create a directory at the .json path so fs::write fails after .jsonl is created
        let meta_path = sessions_dir.path().join(format!("{conversation_id}.json"));
        std::fs::create_dir_all(&meta_path).unwrap();

        let result = exporter.export_session(&conversation_id, sessions_dir.path());
        assert!(result.is_err(), "export should fail when .json path is a directory");

        let log_path = sessions_dir.path().join(format!("{conversation_id}.jsonl"));
        assert!(!log_path.exists(), "jsonl file should be cleaned up on error");
        assert!(!session_exists(sessions_dir.path(), &conversation_id));
    }

    #[tokio::test]
    async fn test_try_export_from_json_valid_v1() {
        let fixture_path = format!("{FIXTURES_DIR}/basic_tool_use.json");
        let raw = std::fs::read_to_string(&fixture_path).unwrap();

        let db = crate::database::Database::new_default().await.unwrap();
        let exporter = LegacySessionExporterImpl::new(Arc::new(db));
        let sessions_dir = TempDir::new().unwrap();
        let session_id = "test-session-id";
        let cwd = Path::new("/test/cwd");

        let import_path = Path::new("/tmp/my-export.json");
        exporter
            .try_export_from_json(&raw, session_id, cwd, sessions_dir.path(), Some(import_path))
            .unwrap();
        assert!(session_exists(sessions_dir.path(), session_id));

        let session_db = SessionDb::load_with_sessions_dir(sessions_dir.path(), session_id, None).unwrap();
        let session_data = session_db.session();
        assert!(session_data.exported_from_v1);
        assert_eq!(session_data.imported_from.as_deref(), Some("/tmp/my-export.json"));
    }

    #[tokio::test]
    async fn test_try_export_from_json_not_v1() {
        let db = crate::database::Database::new_default().await.unwrap();
        let exporter = LegacySessionExporterImpl::new(Arc::new(db));
        let sessions_dir = TempDir::new().unwrap();

        let result =
            exporter.try_export_from_json(r#"{"foo": "bar"}"#, "id", Path::new("/tmp"), sessions_dir.path(), None);
        assert!(result.is_err(), "should return Err for non-V1 JSON");
    }
}
