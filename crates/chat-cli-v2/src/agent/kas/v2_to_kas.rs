//! V2 session -> KAS session conversion.
//!
//! In-memory transformation from V2's `SessionData` + `Vec<LogEntry>`
//! to KAS's session metadata + `Vec<PersistedMessage>`. Disk I/O lives
//! in the caller.

use agent::agent_loop::types::{
    ContentBlock,
    ToolResultContentBlock,
    ToolResultStatus,
};
use agent::event_log::{
    LogEntry,
    LogEntryV1,
};
use chrono::Utc;
use thiserror::Error;

use super::schema::{
    AssistantMessagePayload,
    AssistantOperationType,
    CURRENT_SCHEMA_VERSION,
    CreatedReason,
    KiroPersistedMeta,
    MessagePayload,
    PersistedMessage,
    PersistedPayloadMeta,
    SessionMetadata,
    TombstoneKind,
    TombstonePayload,
    ToolCallPayload,
    ToolCallStatus,
    ToolResultPayload,
    UserMessageImage,
    UserMessagePayload,
    UserMessageSource,
};
use super::session_id::generate_kas_session_id;
use crate::agent::session::SessionData;

/// Inputs to [`convert_v2_to_kas`]. Owned data - the converter
/// runs once per session and the input volume is bounded by a
/// single user's session log, so cloning at the call boundary is
/// cheaper than threading lifetimes through every intermediate
/// helper.
pub struct ConvertArgs {
    /// V2 session metadata read from `<id>.json`.
    pub session: SessionData,
    /// V2 log entries read from `<id>.jsonl`, in disk order.
    pub entries: Vec<LogEntry>,
    /// Workspace paths the resulting KAS session should be bucketed
    /// under. KAS's `compute_workspace_hash(workspacePaths)` decides
    /// the on-disk directory.
    pub workspace_paths: Vec<String>,
}

/// In-memory KAS shape. The caller serializes `metadata` to
/// `session.json` and writes `messages` as JSONL to `messages.jsonl`
/// inside the workspace-hash bucket.
#[derive(Debug)]
pub struct ConvertOutput {
    pub metadata: SessionMetadata,
    pub messages: Vec<PersistedMessage>,
}

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("{0}")]
    Message(String),
}

/// Convert a V2 session to KAS shape in memory.
///
/// `Clear` resets the accumulated output to empty (matching V2's
/// "fresh start within same session" semantic). `Compaction` emits a
/// `summarization` tombstone plus a `Summary` assistant payload and
/// re-emits the retained snapshot tail. `ResetTo` and
/// `CancelledPrompt` are silent no-ops - neither variant is produced
/// by any production V2 session writer.
pub fn convert_v2_to_kas(opts: ConvertArgs) -> Result<ConvertOutput, ConvertError> {
    let metadata = build_metadata(&opts);
    let mut messages: Vec<PersistedMessage> = Vec::new();
    // KAS's `_meta.kiro.userMessageId` binds an assistant or tool
    // payload to the user prompt that triggered it. The converter
    // tracks the most recently-seen V2 `Prompt` message id and
    // stamps that on subsequent assistant / tool_call payloads
    // until the next prompt. Tool results are never bound to a
    // user message id directly (tool results bind to a tool_call
    // via `toolCallId`), so the field stays absent on them.
    let mut current_user_message_id: Option<String> = None;
    for entry in &opts.entries {
        convert_entry(entry, &mut messages, &mut current_user_message_id)?;
    }
    Ok(ConvertOutput { metadata, messages })
}

fn build_metadata(opts: &ConvertArgs) -> SessionMetadata {
    let title = opts
        .session
        .title
        .clone()
        .unwrap_or_else(|| "Imported V2 session".to_string());
    SessionMetadata {
        schema_version: CURRENT_SCHEMA_VERSION.to_string(),
        // KAS surfaces tolerate arbitrary id strings; the `cli_`
        // prefix is observability sugar plus the lookup key the
        // resume-id idempotency probe uses to find prior conversions
        // of this same V2 session.
        id: generate_kas_session_id(Some(&opts.session.session_id)),
        title,
        agent_mode: map_agent_mode_from_agent_name(opts.session.session_state.agent_name()),
        workspace_paths: opts.workspace_paths.clone(),
        created_at: opts.session.created_at.to_rfc3339(),
        last_modified_at: Utc::now().to_rfc3339(),
        // KAS stamps `1` on save (see KAS's
        // `SessionPersistence.saveSession`). The field is advisory:
        // KAS does not branch on the value at load.
        data_model_version: Some(1),
        // V2 has no session-level model state to forward; KAS falls
        // back to the request-level model id when this is absent.
        model_id: None,
        // V2 sessions are always user-initiated chats. KAS's other
        // values (`rewind`, `subagent`, `thread`) describe modes V2
        // doesn't have.
        created_reason: Some(CreatedReason::Human),
        extra: serde_json::Map::new(),
    }
}

/// Map a V2 agent name (from `session_state.agent_name()`) to a KAS
/// `agent_mode` string. V2's two builtins (`kiro_default`,
/// `kiro_planner`) translate to KAS's closest builtin modes (`vibe`,
/// `quick-plan`). Custom V2 agent names pass through verbatim - KAS's
/// `AgentModeSchema` accepts any string and treats unknown values as
/// custom agent profile ids on its side. Missing or unrecognized
/// state falls back to KAS's default mode.
fn map_agent_mode_from_agent_name(v2_agent_name: Option<&str>) -> String {
    match v2_agent_name {
        Some("kiro_default") => "vibe".to_string(),
        Some("kiro_planner") => "quick-plan".to_string(),
        Some(custom) => custom.to_string(),
        None => "vibe".to_string(),
    }
}

fn convert_entry(
    entry: &LogEntry,
    out: &mut Vec<PersistedMessage>,
    current_user_message_id: &mut Option<String>,
) -> Result<(), ConvertError> {
    match entry {
        LogEntry::V1(LogEntryV1::Prompt {
            message_id,
            content,
            meta,
        }) => {
            *current_user_message_id = Some(message_id.clone());
            out.push(prompt_to_user(message_id, content, meta.as_ref()));
            Ok(())
        },
        LogEntry::V1(LogEntryV1::AssistantMessage { message_id, content }) => {
            assistant_message_to_payloads(message_id, content, current_user_message_id.as_deref(), out)
        },
        LogEntry::V1(LogEntryV1::ToolResults { content, results, .. }) => {
            tool_results_to_payloads(content, results, out);
            Ok(())
        },
        LogEntry::V1(LogEntryV1::Compaction {
            summary,
            strategy,
            messages_snapshot,
        }) => {
            compaction_to_payloads(summary, strategy, messages_snapshot, out)?;
            *current_user_message_id = None;
            Ok(())
        },
        LogEntry::V1(LogEntryV1::ResetTo { .. } | LogEntryV1::CancelledPrompt) => {
            // Neither variant is produced by any production V2 session
            // writer. Treated as silent no-ops so a malformed-but-
            // well-typed log still converts cleanly.
            Ok(())
        },
        LogEntry::V1(LogEntryV1::Clear) => {
            // V2 semantic: fresh start within the same session. Drop
            // every payload emitted so far. KAS does not have a
            // first-class equivalent; the converted session simply
            // starts at the post-clear point.
            out.clear();
            *current_user_message_id = None;
            Ok(())
        },
    }
}

/// Build a KAS `user` message from a V2 `Prompt` entry.
///
/// Joins all `Text` blocks into the single `content` string, gathers
/// any `Image` blocks into `images`, and stamps the V2 message id
/// onto `_meta.kiro.userMessageId` for cross-referencing.
fn prompt_to_user(
    message_id: &str,
    content: &[ContentBlock],
    meta: Option<&agent::agent_loop::types::MessageMetadata>,
) -> PersistedMessage {
    let timestamp = meta.and_then(|m| m.timestamp).unwrap_or_else(Utc::now).to_rfc3339();
    let images = collect_images(content);
    let payload = UserMessagePayload {
        content: collect_text(content),
        source: Some(UserMessageSource::Chat),
        images: if images.is_empty() { None } else { Some(images) },
        meta: Some(meta_with_user_id(message_id)),
    };
    PersistedMessage {
        // V2 `Prompt.message_id` and KAS user `PersistedMessage.id`
        // are both UUID v4 minted at prompt time. The V2 id passes
        // through unchanged so a user payload looks the same as one
        // KAS mints natively.
        id: message_id.to_string(),
        timestamp,
        payload: MessagePayload::User(payload),
    }
}

/// Fan a V2 assistant message into KAS payloads. Each `Text` block
/// becomes one `assistant` payload; each `Thinking` block becomes one
/// `assistant { operationType: "Reasoning" }`; each `ToolUse` block
/// becomes one `tool_call`. Empty text blocks are skipped.
///
/// `current_user_message_id` is the most recent `Prompt` message id
/// seen by the converter and is stamped onto each emitted payload's
/// `_meta.kiro.userMessageId`, binding the assistant turn to its
/// triggering user prompt. `None` only at the very start of a session
/// where an assistant message somehow precedes any prompt - in that
/// case the field is omitted on output.
fn assistant_message_to_payloads(
    message_id: &str,
    content: &[ContentBlock],
    current_user_message_id: Option<&str>,
    out: &mut Vec<PersistedMessage>,
) -> Result<(), ConvertError> {
    let mut text_index: usize = 0;
    let mut tool_index: usize = 0;
    let mut think_index: usize = 0;
    let meta = current_user_message_id.map(meta_with_user_id);
    for block in content {
        match block {
            ContentBlock::Text(text) => {
                if text.is_empty() {
                    continue;
                }
                // V2's `AssistantMessage.message_id` has no direct KAS
                // equivalent: KAS persists each block from a turn as
                // its own `PersistedMessage` keyed off a per-action id,
                // and groups them via the payload-level `executionId`
                // field rather than a turn-level disk id. KAS uses ids
                // as opaque equality keys (the only structural parse
                // is on `sub_agent_complete` ids, which we don't
                // emit), so deriving `<v2_message_id>-text-<index>`
                // gives each fanned-out payload a unique, deterministic
                // id without colliding with KAS's own conventions.
                let id = format!("{message_id}-text-{text_index}");
                text_index += 1;
                out.push(PersistedMessage {
                    id,
                    timestamp: Utc::now().to_rfc3339(),
                    payload: MessagePayload::Assistant(AssistantMessagePayload {
                        content: text.clone(),
                        operation_type: Some(AssistantOperationType::Say),
                        reasoning_signature: None,
                        reasoning_model_id: None,
                        meta: meta.clone(),
                    }),
                });
            },
            ContentBlock::Thinking(t) => {
                let id = format!("{message_id}-thinking-{think_index}");
                think_index += 1;
                out.push(PersistedMessage {
                    id,
                    timestamp: Utc::now().to_rfc3339(),
                    payload: MessagePayload::Assistant(AssistantMessagePayload {
                        content: t.text.clone(),
                        operation_type: Some(AssistantOperationType::Reasoning),
                        reasoning_signature: t.signature.clone(),
                        reasoning_model_id: t.model_id.clone(),
                        meta: meta.clone(),
                    }),
                });
            },
            ContentBlock::ToolUse(tu) => {
                let id = format!("{message_id}-tool-{tool_index}");
                tool_index += 1;
                let args = tool_use_args(&tu.tool_use_id, &tu.input)?;
                out.push(PersistedMessage {
                    id,
                    timestamp: Utc::now().to_rfc3339(),
                    payload: MessagePayload::ToolCall(ToolCallPayload {
                        tool_call_id: tu.tool_use_id.clone(),
                        tool_name: tu.name.clone(),
                        args,
                        status: ToolCallStatus::Completed,
                        meta: meta.clone(),
                    }),
                });
            },
            ContentBlock::ToolResult(_) | ContentBlock::Image(_) => {
                // Tool result blocks belong to a `ToolResults` entry; they
                // never live on the assistant side. Image blocks aren't
                // emitted by V2 assistant messages today.
            },
        }
    }
    Ok(())
}

/// Convert a V2 `ToolResults` entry into one KAS `tool_result`
/// payload per HashMap key. The `content` slice on the entry mirrors
/// the same data block-by-block; the HashMap is the source of truth
/// for our output - V2's tool result map and content blocks always
/// agree on shape, so we deduplicate by treating the map as canonical.
fn tool_results_to_payloads(
    _content: &[ContentBlock],
    results: &std::collections::HashMap<String, agent::event_log::ToolResult>,
    out: &mut Vec<PersistedMessage>,
) {
    // Sort by tool_use_id for stable output ordering across runs.
    let mut keys: Vec<&String> = results.keys().collect();
    keys.sort();
    for tool_call_id in keys {
        let tool_result = &results[tool_call_id];
        let success = matches!(tool_result.result, agent::protocol::ToolCallResult::Success(_));
        let content_text = render_tool_result(&tool_result.result);
        // Deterministic id derived from `tool_call_id`. There's exactly
        // one tool_result per tool_call so the id is unique. Format
        // matches KAS's own write convention at
        // `execution-message-adapter.ts` (`${actionId}-result`).
        let id = format!("{tool_call_id}-result");
        out.push(PersistedMessage {
            id,
            timestamp: Utc::now().to_rfc3339(),
            payload: MessagePayload::ToolResult(ToolResultPayload {
                tool_call_id: tool_call_id.clone(),
                content: content_text,
                success,
                duration_ms: None,
                // Native KAS likewise omits `_meta` on tool_result when
                // there's no prompt metadata to carry forward; tool
                // results bind to a tool_call via `toolCallId`, never
                // via `_meta.kiro.userMessageId`.
                meta: None,
            }),
        });
    }
}

/// Walk `out` mimicking KAS's `resolveEffectiveMessages` resolver
/// (a single-pass stack with restore-on-miss) and return the
/// `(first_id, count)` of the surviving effective stack. Used to pin
/// a `summarization` tombstone's `effective_from_message_id` and
/// `metadata.truncatedMessageCount` to KAS-native values when prior
/// tombstones already exist in `out` - `out.first().id` and
/// `out.len()` would both be wrong after the first compaction.
///
/// Mirrors `resolveEffectiveMessages` in
/// `kiro-agent/packages/kiro-agent/src/session/checkpoint-revert.ts`:
/// each non-tombstone is pushed; each tombstone pops the stack until
/// it finds and removes its target, restoring on miss. Returns
/// `None` when no messages are effective (caller skips the tombstone
/// in that case to match KAS-native behavior).
fn effective_head_and_count(out: &[PersistedMessage]) -> Option<(&str, usize)> {
    let mut stack: Vec<&PersistedMessage> = Vec::new();
    for msg in out {
        let target = match &msg.payload {
            MessagePayload::Tombstone(t) => t.effective_from_message_id.as_str(),
            _ => {
                stack.push(msg);
                continue;
            },
        };
        let mut popped: Vec<&PersistedMessage> = Vec::new();
        let mut found = false;
        while let Some(top) = stack.pop() {
            if top.id == target {
                found = true;
                break;
            }
            popped.push(top);
        }
        if !found {
            for item in popped.into_iter().rev() {
                stack.push(item);
            }
        }
    }
    stack.first().map(|m| (m.id.as_str(), stack.len()))
}

/// Convert a V2 `Compaction` entry into KAS shape: a
/// `summarization` tombstone, the `Summary` assistant payload, then
/// the retained `messages_snapshot` tail re-emitted as ordinary
/// payloads.
///
/// The tombstone's `effective_from_message_id` targets the first
/// EFFECTIVE message - the first message that survives KAS's
/// resolver after applying every tombstone already in `out`. For a
/// session with one Compaction this is `out.first()`; for sessions
/// with prior compactions the literal first id is already popped, so
/// the tombstone has to target the post-prior-summary head instead.
/// KAS replay pops the effective stack back through that id
/// (inclusive), so the pre-summary turns drop out of model context
/// while staying on disk for UI drill-down. `metadata` mirrors
/// KAS-native compaction (`truncatedMessageCount` is the effective
/// stack size at compaction time, `truncatedAt` is the timestamp).
/// When no message is currently effective there is nothing to
/// summarize, so only the snapshot tail is emitted.
///
/// The snapshot tail repeats the ids of the pre-compaction messages it
/// retains. Re-emitting them verbatim would collide with the
/// tombstoned originals, so the tail is re-emitted under a per-
/// compaction id namespace, keeping every id in the session unique
/// while the tombstone target remains a real pre-compaction id.
fn compaction_to_payloads(
    summary: &str,
    _strategy: &agent::compact::CompactStrategy,
    messages_snapshot: &[agent::agent_loop::types::Message],
    out: &mut Vec<PersistedMessage>,
) -> Result<(), ConvertError> {
    let namespace = uuid::Uuid::new_v4();
    if let Some((effective_from, effective_count)) = effective_head_and_count(out).map(|(id, n)| (id.to_string(), n)) {
        let now = Utc::now().to_rfc3339();
        let mut metadata = serde_json::Map::new();
        metadata.insert("truncatedMessageCount".to_string(), effective_count.into());
        metadata.insert("truncatedAt".to_string(), now.clone().into());
        out.push(PersistedMessage {
            id: format!("tombstone_summarize_{namespace}"),
            timestamp: now.clone(),
            payload: MessagePayload::Tombstone(TombstonePayload {
                kind: TombstoneKind::Summarization,
                effective_from_message_id: effective_from,
                metadata: Some(metadata),
            }),
        });
        out.push(PersistedMessage {
            id: format!("summary_{namespace}"),
            timestamp: now,
            payload: MessagePayload::Assistant(AssistantMessagePayload {
                content: summary.to_string(),
                operation_type: Some(AssistantOperationType::Summary),
                reasoning_signature: None,
                reasoning_model_id: None,
                meta: None,
            }),
        });
    }
    let prefix = format!("compact_{namespace}_");
    let mut snapshot_user_id: Option<String> = None;
    for message in messages_snapshot {
        snapshot_message_to_payloads(message, &prefix, &mut snapshot_user_id, out)?;
    }
    Ok(())
}

/// Convert one retained snapshot [`Message`] into KAS payloads. A
/// `user` message becomes a `user` payload for its text and a
/// `tool_result` payload per `ToolResult` block. An `assistant`
/// message becomes an `assistant` payload for its text and a
/// `tool_call` payload per `ToolUse` block.
///
/// `prefix` namespaces every emitted id and tool-call id so the
/// retained tail never collides with the tombstoned originals it
/// duplicates. `snapshot_user_id` tracks the most recent text-bearing
/// snapshot user so assistant payloads bind their `userMessageId` to a
/// real preceding user rather than to themselves. A toolResult-only
/// user does not update it; an assistant with no preceding user omits
/// the field.
fn snapshot_message_to_payloads(
    message: &agent::agent_loop::types::Message,
    prefix: &str,
    snapshot_user_id: &mut Option<String>,
    out: &mut Vec<PersistedMessage>,
) -> Result<(), ConvertError> {
    use agent::agent_loop::types::Role;
    let raw_id = message.id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let id = format!("{prefix}{raw_id}");
    let timestamp = message
        .meta
        .as_ref()
        .and_then(|m| m.timestamp)
        .unwrap_or_else(Utc::now)
        .to_rfc3339();
    match message.role {
        Role::User => {
            let text = collect_text(&message.content);
            if !text.is_empty() {
                *snapshot_user_id = Some(id.clone());
                out.push(PersistedMessage {
                    id: id.clone(),
                    timestamp: timestamp.clone(),
                    payload: MessagePayload::User(UserMessagePayload {
                        content: text,
                        source: Some(UserMessageSource::Chat),
                        images: None,
                        meta: Some(meta_with_user_id(&id)),
                    }),
                });
            }
            for block in &message.content {
                if let ContentBlock::ToolResult(tr) = block {
                    out.push(PersistedMessage {
                        id: format!("{prefix}{}-result", tr.tool_use_id),
                        timestamp: timestamp.clone(),
                        payload: MessagePayload::ToolResult(ToolResultPayload {
                            tool_call_id: tr.tool_use_id.clone(),
                            content: render_tool_result_blocks(&tr.content),
                            success: matches!(tr.status, ToolResultStatus::Success),
                            duration_ms: None,
                            meta: None,
                        }),
                    });
                }
            }
        },
        Role::Assistant => {
            assistant_message_to_payloads(&id, &message.content, snapshot_user_id.as_deref(), out)?;
        },
    }
    Ok(())
}

/// Render the content blocks of a snapshot `ToolResultBlock` to a
/// single KAS-style string. Mirrors [`render_tool_result`] but
/// operates on the V2 `ToolResultContentBlock` shape carried in
/// retained snapshot messages.
fn render_tool_result_blocks(content: &[ToolResultContentBlock]) -> String {
    content
        .iter()
        .map(|c| match c {
            ToolResultContentBlock::Text(t) => t.clone(),
            ToolResultContentBlock::Json(v) => v.to_string(),
            ToolResultContentBlock::Image(_) => "[image]".to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Concatenate every `Text` block in `content`. Other block kinds
/// are ignored.
fn collect_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Convert any `Image` blocks into [`UserMessageImage`] entries.
fn collect_images(content: &[ContentBlock]) -> Vec<UserMessageImage> {
    use agent::agent_loop::types::{
        ImageFormat,
        ImageSource,
    };
    use base64::Engine;
    content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Image(img) => {
                let mime = match img.format {
                    ImageFormat::Gif => "image/gif",
                    ImageFormat::Jpeg => "image/jpeg",
                    ImageFormat::Png => "image/png",
                    ImageFormat::Webp => "image/webp",
                };
                let data = match &img.source {
                    ImageSource::Bytes(bytes) => base64::engine::general_purpose::STANDARD.encode(bytes),
                };
                Some(UserMessageImage {
                    data,
                    mime_type: mime.to_string(),
                })
            },
            _ => None,
        })
        .collect()
}

/// Render a V2 `ToolCallResult` to a KAS-style `content` string.
/// Concatenates text items; JSON items are stringified; image items
/// produce a `[image]` marker since KAS's `tool_result.content` is
/// a single string.
fn render_tool_result(result: &agent::protocol::ToolCallResult) -> String {
    use agent::protocol::ToolCallResult;
    use agent::tools::ToolExecutionOutputItem;
    let items: Vec<&ToolExecutionOutputItem> = match result {
        ToolCallResult::Success(output) => output.items.iter().collect(),
        ToolCallResult::Error(err) => return format!("error: {err:?}"),
        ToolCallResult::Cancelled => return "[cancelled]".to_string(),
    };
    let mut parts: Vec<String> = Vec::new();
    for item in items {
        match item {
            ToolExecutionOutputItem::Text(t) => parts.push(t.clone()),
            ToolExecutionOutputItem::Json(v) => parts.push(v.to_string()),
            ToolExecutionOutputItem::Image(_) => parts.push("[image]".to_string()),
        }
    }
    parts.join("\n")
}

/// Coerce a V2 tool input (`serde_json::Value`) to KAS's `args`
/// shape (`Map<String, Value>`). KAS rejects non-object args, so a
/// non-object input here signals a corrupt V2 session and is
/// surfaced as a [`ConvertError`] rather than silently wrapped.
fn tool_use_args(
    tool_use_id: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, ConvertError> {
    match input {
        serde_json::Value::Object(map) => Ok(map.clone()),
        other => Err(ConvertError::Message(format!(
            "tool {tool_use_id} input is not an object (got {}); KAS args must be a map",
            shape_name(other),
        ))),
    }
}

/// Return a short type name for a non-object [`serde_json::Value`].
/// Used to produce informative `tool_use_args` errors.
fn shape_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Build a `_meta.kiro.userMessageId` envelope from a V2 message id.
fn meta_with_user_id(message_id: &str) -> PersistedPayloadMeta {
    PersistedPayloadMeta {
        kiro: Some(KiroPersistedMeta {
            user_message_id: Some(message_id.to_string()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use agent::agent_loop::types::{
        ContentBlock,
        Message,
        ToolUseBlock,
    };
    use agent::event_log::ToolResult;
    use agent::protocol::ToolCallResult;
    use agent::tools::{
        ToolExecutionOutput,
        ToolExecutionOutputItem,
    };
    use chrono::TimeZone;

    use super::*;
    use crate::agent::session::{
        SessionCreatedReason,
        SessionState,
    };

    /// Build a minimal `SessionData` for tests. Title and timestamps are
    /// fixed; `session_state` is `Unknown` to skip constructing the
    /// conversation-state graph.
    fn fixture_session(session_id: &str) -> SessionData {
        SessionData {
            session_id: session_id.to_string(),
            cwd: PathBuf::from("/tmp/fixture"),
            created_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 5, 0).unwrap(),
            title: Some("test session".to_string()),
            exported_from_v1: false,
            imported_from: None,
            parent_session_id: None,
            session_created_reason: SessionCreatedReason::Subagent,
            session_state: SessionState::Unknown,
        }
    }

    fn prompt(id: &str, text: &str) -> LogEntry {
        LogEntry::V1(LogEntryV1::Prompt {
            message_id: id.to_string(),
            content: vec![ContentBlock::Text(text.to_string())],
            meta: None,
        })
    }

    fn assistant_text(id: &str, text: &str) -> LogEntry {
        LogEntry::V1(LogEntryV1::AssistantMessage {
            message_id: id.to_string(),
            content: vec![ContentBlock::Text(text.to_string())],
        })
    }

    fn assistant_tool_use(id: &str, tool_use_id: &str, name: &str) -> LogEntry {
        LogEntry::V1(LogEntryV1::AssistantMessage {
            message_id: id.to_string(),
            content: vec![ContentBlock::ToolUse(ToolUseBlock {
                tool_use_id: tool_use_id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({"path": "/foo"}),
            })],
        })
    }

    fn tool_results_for(message_id: &str, tool_use_id: &str, output: &str) -> LogEntry {
        let mut results = HashMap::new();
        results.insert(tool_use_id.to_string(), ToolResult {
            tool: None,
            result: ToolCallResult::Success(ToolExecutionOutput {
                items: vec![ToolExecutionOutputItem::Text(output.to_string())],
            }),
        });
        LogEntry::V1(LogEntryV1::ToolResults {
            message_id: message_id.to_string(),
            content: vec![],
            results,
        })
    }

    fn convert(entries: Vec<LogEntry>) -> ConvertOutput {
        let session = fixture_session("11111111-1111-1111-1111-111111111111");
        let workspace_paths = vec!["/tmp/ws".to_string()];
        convert_v2_to_kas(ConvertArgs {
            session,
            entries,
            workspace_paths,
        })
        .expect("conversion should succeed")
    }

    /// Build a `SessionStateV1` with the given agent name. Used to
    /// drive `map_agent_mode_from_agent_name` through the public
    /// converter entrypoint.
    fn session_state_with_agent(name: &str) -> SessionState {
        use agent::permissions::RuntimePermissions;
        use agent::types::ConversationMetadata;

        use crate::agent::rts::RtsStateSnapshot;

        SessionState::V1(crate::agent::session::SessionStateV1 {
            conversation_metadata: ConversationMetadata::default(),
            rts_model_state: RtsStateSnapshot {
                conversation_id: "test-conversation".to_string(),
                model_info: None,
                context_usage_percentage: None,
                additional_fields: None,
            },
            permissions: RuntimePermissions::default(),
            agent_name: Some(name.to_string()),
            goal: None,
        })
    }

    fn convert_with_agent(agent: &str) -> ConvertOutput {
        let mut session = fixture_session("11111111-1111-1111-1111-111111111111");
        session.session_state = session_state_with_agent(agent);
        convert_v2_to_kas(ConvertArgs {
            session,
            entries: vec![prompt("u1", "hi")],
            workspace_paths: vec!["/tmp/ws".to_string()],
        })
        .expect("conversion should succeed")
    }

    /// `schemaVersion` matches what KAS itself writes (`"1.0.0"`),
    /// not the `acp-type-covenant` package version. Sessions stamped
    /// with anything else are rejected by `import.rs`.
    #[test]
    fn metadata_stamps_kas_compatible_schema_version() {
        let out = convert(vec![prompt("u1", "hi")]);
        assert_eq!(out.metadata.schema_version, "1.0.0");
    }

    /// `dataModelVersion` is stamped to `1`, the value KAS writes on
    /// save. The field is advisory: KAS does not branch on the value
    /// at load time.
    #[test]
    fn metadata_stamps_data_model_version_one() {
        let out = convert(vec![prompt("u1", "hi")]);
        assert_eq!(out.metadata.data_model_version, Some(1));
    }

    /// V2's `agent_name` is a V2-side identifier (e.g. `kiro_default`)
    /// that has no meaning to KAS. The converter emits the documented
    /// KAS default mode rather than propagating the V2 string.
    #[test]
    fn metadata_uses_kas_default_agent_mode() {
        let out = convert(vec![prompt("u1", "hi")]);
        assert_eq!(out.metadata.agent_mode, "vibe");
    }

    /// V2 `kiro_default` maps to KAS `vibe`.
    #[test]
    fn agent_mode_maps_kiro_default_to_vibe() {
        let out = convert_with_agent("kiro_default");
        assert_eq!(out.metadata.agent_mode, "vibe");
    }

    /// V2 `kiro_planner` maps to KAS `quick-plan` - KAS's only
    /// plan-flavored builtin mode.
    #[test]
    fn agent_mode_maps_kiro_planner_to_quick_plan() {
        let out = convert_with_agent("kiro_planner");
        assert_eq!(out.metadata.agent_mode, "quick-plan");
    }

    /// Custom V2 agent names pass through verbatim. KAS treats
    /// unknown mode strings as custom agent profile ids.
    #[test]
    fn agent_mode_passes_custom_agent_name_through() {
        let out = convert_with_agent("my_custom_agent");
        assert_eq!(out.metadata.agent_mode, "my_custom_agent");
    }

    /// Generated session id is `cli_{v2_session_id}_{8 alphanumeric}`.
    /// The prefix encodes provenance; the suffix prevents collision
    /// when the same V2 session is converted twice.
    #[test]
    fn target_session_id_format() {
        let out = convert(vec![prompt("u1", "hi")]);
        let id = &out.metadata.id;
        let v2_id = "11111111-1111-1111-1111-111111111111";
        assert!(id.starts_with(&format!("cli_{v2_id}_")), "got id: {id}");
        let suffix = id.strip_prefix(&format!("cli_{v2_id}_")).expect("prefix matches");
        assert_eq!(suffix.len(), 8, "suffix should be 8 chars: {suffix}");
        assert!(
            suffix.chars().all(|c| c.is_ascii_alphanumeric()),
            "suffix should be alphanumeric: {suffix}"
        );
    }

    /// Each conversion mints a fresh suffix so re-converting the same
    /// V2 session produces a distinct KAS session id (avoiding
    /// collision in the workspace bucket).
    #[test]
    fn target_session_id_suffix_is_random_per_call() {
        let first = convert(vec![prompt("u1", "hi")]).metadata.id;
        let second = convert(vec![prompt("u1", "hi")]).metadata.id;
        assert_ne!(first, second);
    }

    /// `_meta.kiro.userMessageId` on assistant + tool_call payloads is
    /// the most-recent V2 `Prompt` message id, not the assistant's
    /// own id. Downstream KAS code uses this field to bind assistant
    /// turns to their triggering user prompt.
    #[test]
    fn assistant_payloads_carry_prompt_id_in_user_message_id() {
        let out = convert(vec![
            prompt("user-1", "do thing"),
            assistant_text("assistant-1", "ok"),
            assistant_tool_use("assistant-2", "tu-1", "fs_read"),
        ]);

        let user_msg_ids: Vec<Option<String>> = out
            .messages
            .iter()
            .map(|m| match &m.payload {
                MessagePayload::Assistant(a) => a
                    .meta
                    .as_ref()
                    .and_then(|x| x.kiro.as_ref())
                    .and_then(|k| k.user_message_id.clone()),
                MessagePayload::ToolCall(t) => t
                    .meta
                    .as_ref()
                    .and_then(|x| x.kiro.as_ref())
                    .and_then(|k| k.user_message_id.clone()),
                _ => None,
            })
            .collect();

        // First payload is the user prompt itself (not asserted here);
        // the assistant text and the tool_call should both bind back
        // to "user-1".
        assert_eq!(user_msg_ids[1], Some("user-1".to_string()));
        assert_eq!(user_msg_ids[2], Some("user-1".to_string()));
    }

    /// A second user prompt switches the binding for subsequent
    /// assistant payloads.
    #[test]
    fn user_message_id_resets_on_each_prompt() {
        let out = convert(vec![
            prompt("user-1", "first"),
            assistant_text("a-1", "first reply"),
            prompt("user-2", "second"),
            assistant_text("a-2", "second reply"),
        ]);

        // Messages: [user-1, a-1-text-0, user-2, a-2-text-0]
        let bindings: Vec<Option<String>> = out
            .messages
            .iter()
            .filter_map(|m| match &m.payload {
                MessagePayload::Assistant(a) => Some(
                    a.meta
                        .as_ref()
                        .and_then(|x| x.kiro.as_ref())
                        .and_then(|k| k.user_message_id.clone()),
                ),
                _ => None,
            })
            .collect();

        assert_eq!(bindings, vec![Some("user-1".to_string()), Some("user-2".to_string())]);
    }

    /// Tool result ids are deterministic across runs - the suffix is
    /// derived from `tool_call_id`, not a random UUID.
    #[test]
    fn tool_result_ids_are_deterministic() {
        let entries = vec![
            prompt("user-1", "use a tool"),
            assistant_tool_use("a-1", "tu-1", "fs_read"),
            tool_results_for("tr-1", "tu-1", "file contents"),
        ];

        let first = convert(entries.clone());
        let second = convert(entries);

        let extract_tool_result_ids = |out: &ConvertOutput| -> Vec<String> {
            out.messages
                .iter()
                .filter_map(|m| match &m.payload {
                    MessagePayload::ToolResult(_) => Some(m.id.clone()),
                    _ => None,
                })
                .collect()
        };

        assert_eq!(extract_tool_result_ids(&first), extract_tool_result_ids(&second));
        assert_eq!(extract_tool_result_ids(&first), vec!["tu-1-result".to_string()]);
    }

    /// `Clear` drops the accumulated converted output, mirroring V2's
    /// "fresh start within the same session" semantic. Subsequent
    /// payloads start over.
    #[test]
    fn clear_drops_accumulated_output() {
        let out = convert(vec![
            prompt("u1", "first"),
            assistant_text("a1", "first reply"),
            LogEntry::V1(LogEntryV1::Clear),
            prompt("u2", "second"),
            assistant_text("a2", "second reply"),
        ]);

        // Pre-clear messages were dropped; only the post-clear pair
        // (user u2 + assistant a2-text-0) survives.
        assert_eq!(out.messages.len(), 2);
        assert_eq!(out.messages[0].id, "u2");
        assert_eq!(out.messages[1].id, "a2-text-0");
    }

    /// `CancelledPrompt` never appears in production V2 sessions.
    /// The converter treats it as a silent no-op - the surrounding
    /// prompts and replies pass through unchanged.
    #[test]
    fn cancelled_prompt_is_silent_noop() {
        let out = convert(vec![
            prompt("u1", "first"),
            assistant_text("a1", "first reply"),
            prompt("u2", "second"),
            LogEntry::V1(LogEntryV1::CancelledPrompt),
            prompt("u3", "third"),
            assistant_text("a3", "third reply"),
        ]);

        // Every prompt is preserved. Subsequent assistants bind to
        // u3, the most-recent prompt.
        assert_eq!(out.messages.len(), 5);
        assert_eq!(out.messages[0].id, "u1");
        assert_eq!(out.messages[2].id, "u2");
        assert_eq!(out.messages[3].id, "u3");
        if let MessagePayload::Assistant(a) = &out.messages[4].payload {
            let bound = a.meta.as_ref().unwrap().kiro.as_ref().unwrap().user_message_id.clone();
            assert_eq!(bound, Some("u3".to_string()));
        } else {
            panic!("expected assistant payload");
        }
    }

    /// Same no-op behavior whether or not the preceding entry is a
    /// user message.
    #[test]
    fn cancelled_prompt_with_non_user_last_is_noop() {
        let out = convert(vec![
            prompt("u1", "hello"),
            assistant_text("a1", "reply"),
            LogEntry::V1(LogEntryV1::CancelledPrompt),
        ]);

        assert_eq!(out.messages.len(), 2);
    }

    /// `ResetTo` never appears in production V2 sessions. The
    /// converter treats it as a silent no-op rather than failing the
    /// whole conversion.
    #[test]
    fn reset_to_is_silent_noop() {
        let out = convert(vec![
            prompt("u1", "first"),
            assistant_text("a1", "first reply"),
            LogEntry::V1(LogEntryV1::ResetTo { target_index: 0 }),
            prompt("u2", "second"),
        ]);

        // Output is identical to running without the ResetTo entry:
        // [u1, a1-text-0, u2].
        assert_eq!(out.messages.len(), 3);
        assert_eq!(out.messages[0].id, "u1");
        assert_eq!(out.messages[1].id, "a1-text-0");
        assert_eq!(out.messages[2].id, "u2");
    }

    fn user_tool_result(id: &str, tool_use_id: &str, output: &str) -> Message {
        use agent::agent_loop::types::{
            Role,
            ToolResultBlock,
            ToolResultContentBlock,
            ToolResultStatus,
        };
        Message {
            id: Some(id.to_string()),
            role: Role::User,
            content: vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: tool_use_id.to_string(),
                content: vec![ToolResultContentBlock::Text(output.to_string())],
                status: ToolResultStatus::Success,
            })],
            meta: None,
        }
    }

    fn assistant_snapshot(id: &str, text: &str, tool_use_id: &str, name: &str) -> Message {
        use agent::agent_loop::types::Role;
        Message {
            id: Some(id.to_string()),
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text(text.to_string()),
                ContentBlock::ToolUse(ToolUseBlock {
                    tool_use_id: tool_use_id.to_string(),
                    name: name.to_string(),
                    input: serde_json::json!({"path": "/foo"}),
                }),
            ],
            meta: None,
        }
    }

    fn compaction(summary: &str, snapshot: Vec<Message>) -> LogEntry {
        LogEntry::V1(LogEntryV1::Compaction {
            summary: summary.to_string(),
            strategy: agent::compact::CompactStrategy::default_strategy(),
            messages_snapshot: snapshot,
        })
    }

    /// `Compaction` emits a `summarization` tombstone whose
    /// `effectiveFromMessageId` targets the first effective message,
    /// immediately followed by the `Summary` assistant payload.
    #[test]
    fn compaction_emits_tombstone_then_summary() {
        let out = convert(vec![
            prompt("u1", "do work"),
            assistant_text("a1", "working"),
            compaction("## OBJECTIVE\nrecorded", vec![]),
        ]);

        // [u1, a1-text-0, tombstone, summary]
        let tombstone = &out.messages[2];
        match &tombstone.payload {
            MessagePayload::Tombstone(t) => {
                assert!(matches!(t.kind, TombstoneKind::Summarization));
                assert_eq!(t.effective_from_message_id, "u1");
                let meta = t.metadata.as_ref().expect("metadata present");
                assert_eq!(meta.get("truncatedMessageCount").and_then(|v| v.as_u64()), Some(2));
            },
            other => panic!("expected tombstone, got {other:?}"),
        }
        match &out.messages[3].payload {
            MessagePayload::Assistant(a) => {
                assert!(matches!(a.operation_type, Some(AssistantOperationType::Summary)));
                assert_eq!(a.content, "## OBJECTIVE\nrecorded");
            },
            other => panic!("expected summary assistant, got {other:?}"),
        }
    }

    /// The retained `messages_snapshot` tail is re-emitted after the
    /// summary: user toolResults become `tool_result` payloads and
    /// assistant text+toolUse become `assistant` + `tool_call`.
    #[test]
    fn compaction_reemits_snapshot_tail() {
        let snapshot = vec![
            user_tool_result("snap-u", "tu-prev", "prior output"),
            assistant_snapshot("snap-a", "next step", "tu-next", "write"),
        ];
        let out = convert(vec![
            prompt("u1", "do work"),
            assistant_text("a1", "working"),
            compaction("summary", snapshot),
        ]);

        // tail after [u1, a1-text-0, tombstone, summary]
        let tail = &out.messages[4..];
        assert!(matches!(tail[0].payload, MessagePayload::ToolResult(_)));
        assert!(tail[0].id.ends_with("tu-prev-result"));
        assert!(matches!(tail[1].payload, MessagePayload::Assistant(_)));
        assert!(matches!(tail[2].payload, MessagePayload::ToolCall(_)));
    }

    /// A `Compaction` with no preceding effective messages emits no
    /// tombstone or summary - only the retained snapshot tail.
    #[test]
    fn compaction_with_empty_history_skips_tombstone() {
        let out = convert(vec![compaction("summary", vec![user_tool_result("s", "tu", "out")])]);
        assert!(
            out.messages
                .iter()
                .all(|m| !matches!(m.payload, MessagePayload::Tombstone(_)))
        );
        assert_eq!(out.messages.len(), 1);
        assert!(matches!(out.messages[0].payload, MessagePayload::ToolResult(_)));
    }

    /// Compaction drops the pre-compaction segment after the tombstone
    /// + summary, so the re-emitted snapshot tail never collides with
    /// the ids it summarized. Every id in the converted session is
    /// unique.
    #[test]
    fn compaction_produces_unique_ids() {
        let snapshot = vec![
            user_tool_result("snap-u", "tu-prev", "prior output"),
            assistant_snapshot("snap-a", "next step", "tu-next", "write"),
        ];
        let out = convert(vec![
            prompt("u1", "do work"),
            assistant_tool_use("a1", "tu-prev", "write"),
            tool_results_for("tr1", "tu-prev", "prior output"),
            compaction("summary", snapshot),
        ]);

        let mut ids: Vec<&str> = out.messages.iter().map(|m| m.id.as_str()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "converted session has duplicate message ids");
    }

    /// Two consecutive `Compaction` entries each emit a tombstone whose
    /// `effective_from_message_id` targets a currently-effective id at
    /// the time of that compaction. Tombstone-2 must NOT reuse
    /// tombstone-1's target: KAS's `resolveEffectiveMessages` pops the
    /// target id off the effective stack when applying tombstone-1, so
    /// reusing it would be a reference to an already-removed id and
    /// the resolver skips the tombstone, leaving the inter-compaction
    /// turns in model context.
    #[test]
    fn double_compaction_tombstones_target_distinct_effective_ids() {
        let out = convert(vec![
            prompt("u1", "first"),
            assistant_text("a1", "answer"),
            compaction("summary-1", vec![]),
            prompt("u2", "second"),
            assistant_text("a2", "answer"),
            compaction("summary-2", vec![]),
        ]);

        let tombstones: Vec<&PersistedMessage> = out
            .messages
            .iter()
            .filter(|m| matches!(m.payload, MessagePayload::Tombstone(_)))
            .collect();
        assert_eq!(tombstones.len(), 2, "expected two tombstones");

        let target1 = match &tombstones[0].payload {
            MessagePayload::Tombstone(t) => t.effective_from_message_id.clone(),
            _ => unreachable!(),
        };
        let target2 = match &tombstones[1].payload {
            MessagePayload::Tombstone(t) => t.effective_from_message_id.clone(),
            _ => unreachable!(),
        };

        assert_ne!(
            target1, target2,
            "second tombstone targets the first tombstone's already-popped id; \
             KAS resolver will skip it and leave inter-compaction turns visible"
        );
        // Tombstone-1 targets the literal first message - it was the
        // first effective message at compaction time.
        assert_eq!(target1, "u1");
        // Tombstone-2 targets the first effective message AFTER
        // tombstone-1 has been applied: that is `summary_<...>`, the
        // summary assistant emitted by tombstone-1.
        assert!(
            target2.starts_with("summary_"),
            "tombstone-2 should target the summary emitted by tombstone-1, got `{target2}`"
        );
    }

    /// A snapshot assistant binds its `userMessageId` to the snapshot
    /// user that preceded it, never to its own id. Self-binding would
    /// group the turn under a non-existent user.
    #[test]
    fn snapshot_assistant_binds_to_snapshot_user_not_itself() {
        let snapshot = vec![
            Message {
                id: Some("snap-user".to_string()),
                role: agent::agent_loop::types::Role::User,
                content: vec![ContentBlock::Text("retained question".to_string())],
                meta: None,
            },
            assistant_snapshot("snap-asst", "retained answer", "tu-x", "write"),
        ];
        let out = convert(vec![prompt("u1", "work"), compaction("summary", snapshot)]);

        let snapshot_user_id = out
            .messages
            .iter()
            .find_map(|m| match &m.payload {
                MessagePayload::User(_) if m.id.ends_with("snap-user") => Some(m.id.clone()),
                _ => None,
            })
            .expect("snapshot user payload present");
        let binding = out
            .messages
            .iter()
            .find_map(|m| match &m.payload {
                MessagePayload::Assistant(a) if m.id.ends_with("snap-asst-text-0") => Some(
                    a.meta
                        .as_ref()
                        .and_then(|x| x.kiro.as_ref())
                        .and_then(|k| k.user_message_id.clone()),
                ),
                _ => None,
            })
            .expect("snapshot assistant text payload present");
        assert_eq!(binding, Some(snapshot_user_id));
    }

    /// Tool inputs are required to be JSON objects on the KAS side.
    /// A primitive or array input signals a corrupt V2 session and
    /// surfaces as an error rather than silently being wrapped.
    #[test]
    fn non_object_tool_input_errors() {
        let entries = vec![
            prompt("u1", "use a tool"),
            LogEntry::V1(LogEntryV1::AssistantMessage {
                message_id: "a-1".to_string(),
                content: vec![ContentBlock::ToolUse(ToolUseBlock {
                    tool_use_id: "tu-1".to_string(),
                    name: "weird_tool".to_string(),
                    input: serde_json::json!("not-an-object"),
                })],
            }),
        ];
        let result = convert_v2_to_kas(ConvertArgs {
            session: fixture_session("11111111-1111-1111-1111-111111111111"),
            entries,
            workspace_paths: vec!["/tmp/ws".to_string()],
        });
        let err = result.expect_err("non-object tool input should error");
        let msg = err.to_string();
        assert!(msg.contains("tu-1"), "error should reference tool use id: {msg}");
        assert!(msg.contains("string"), "error should name the input shape: {msg}");
    }
}
