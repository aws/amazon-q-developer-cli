//! Rust mirror of the KAS persistence schema in
//! `@kiro/acp-type-covenant`'s `session/schemas/index.ts`.
//!
//! The TypeScript Zod schema is the source of truth; these structs are
//! a hand-written subset covering the fields this crate emits. Adding
//! a new payload variant or field requires checking the `.d.ts`
//! definitions to confirm the wire shape.
//!
//! # Coverage
//!
//! These types are **safe for V2 -> KAS write only**. Only the four
//! `MessagePayload` variants needed by the V2 -> KAS converter
//! (`user`, `assistant`, `tool_call`, `tool_result`) are modelled, and
//! only the fields the converter emits are named. [`MessagePayload`]
//! is `#[non_exhaustive]` so adding more variants later is
//! non-breaking.
//!
//! For full-fidelity read of native KAS sessions (e.g. `kas_to_v2`),
//! extend the explicit fields and add round-trip support before
//! relying on these types.
//!
//! Struct fields use camelCase via `#[serde(rename_all = ...)]` to
//! match the JSON keys KAS reads.

use serde::{
    Deserialize,
    Serialize,
};

/// Schema version stamped onto every session this crate writes.
///
/// KAS's own `SessionPersistence.saveSession` writes `"1.0.0"` (see
/// `kiro-agent/packages/kiro-agent/src/session/session-persistence.ts`).
/// The constant `SCHEMA_VERSION` exported from `@kiro/acp-type-covenant`
/// is the package version (`"0.0.1"` at time of writing), NOT the
/// on-disk version - they share a name but mean different things.
/// We mirror what KAS writes so converted sessions are
/// indistinguishable from native KAS output.
pub const CURRENT_SCHEMA_VERSION: &str = "1.0.0";

/// Set of `schemaVersion` strings this crate accepts on read. Imports
/// or conversions that encounter any other value fail fast rather
/// than producing a session KAS later refuses to load.
pub const SUPPORTED_SCHEMA_VERSIONS: &[&str] = &["1.0.0"];

/// `session.json` contents. Mirrors `SessionMetadataSchema`.
///
/// Required fields (non-`Option`): KAS rejects sessions missing any of
/// these on load. Optional fields are skipped on serialize when absent
/// so a partially-populated converter output stays minimal on disk.
///
/// `extra` captures fields not named explicitly above. Serde
/// `flatten` re-emits captured entries inline at the parent level on
/// serialize, so a parse-mutate-serialize cycle on `id` and
/// `workspace_paths` preserves any other metadata the parent
/// `session.json` carried.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub schema_version: String,
    pub id: String,
    pub title: String,
    pub agent_mode: String,
    pub workspace_paths: Vec<String>,
    pub created_at: String,
    pub last_modified_at: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_model_version: Option<u64>,

    /// Active model id at the time of save. KAS reads this to seed
    /// the model selector on resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,

    /// What initiated session creation. Metadata-only on the KAS
    /// side; not read back during load or replay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_reason: Option<CreatedReason>,

    /// Any field not named explicitly above. Serde flattening
    /// captures unknown entries on deserialize and inlines them on
    /// serialize, so a parse-mutate-serialize cycle preserves
    /// arbitrary metadata the parent `session.json` carried.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Mirrors KAS's `createdReason` enum. The four variants are the
/// only values KAS's Zod schema accepts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CreatedReason {
    Human,
    Rewind,
    Subagent,
    Thread,
}

/// One line of `messages.jsonl`. Mirrors `PersistedMessageSchema`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    pub id: String,
    pub timestamp: String,
    pub payload: MessagePayload,
}

/// Discriminated-union payload of [`PersistedMessage`]. Mirrors
/// `MessagePayloadSchema`. Only the variants this crate emits are
/// listed; the enum is `#[non_exhaustive]` so adding new variants
/// later is not a breaking change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum MessagePayload {
    User(UserMessagePayload),
    Assistant(AssistantMessagePayload),
    ToolCall(ToolCallPayload),
    ToolResult(ToolResultPayload),
    Tombstone(TombstonePayload),
}

/// Image attached to a user message. Inline base64.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessageImage {
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessagePayload {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<UserMessageSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<UserMessageImage>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "_meta")]
    pub meta: Option<PersistedPayloadMeta>,
}

/// Where a user message came from. Mirrors the Zod `source` enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMessageSource {
    Chat,
    Hook,
    Api,
    Steer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessagePayload {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_type: Option<AssistantOperationType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "_meta")]
    pub meta: Option<PersistedPayloadMeta>,
}

/// Categorizes assistant messages. KAS uses this to decide rendering
/// (regular text vs reasoning vs printed program output).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AssistantOperationType {
    Say,
    Reasoning,
    Print,
    Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallPayload {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: serde_json::Map<String, serde_json::Value>,
    pub status: ToolCallStatus,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "_meta")]
    pub meta: Option<PersistedPayloadMeta>,
}

/// Lifecycle stage of a tool call. The converter always emits
/// [`Self::Completed`] because V2 only persists tool calls that have
/// returned a result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    AwaitingApproval,
    Approved,
    Denied,
    Executing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultPayload {
    pub tool_call_id: String,
    pub content: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "_meta")]
    pub meta: Option<PersistedPayloadMeta>,
}

/// Marks a range of prior messages as no longer effective. A
/// `summarization` tombstone pops the effective stack back through
/// `effective_from_message_id` (inclusive) while keeping the covered
/// messages on disk for UI drill-down. Mirrors `TombstonePayloadSchema`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TombstonePayload {
    pub kind: TombstoneKind,
    pub effective_from_message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Tombstone variants. Mirrors the Zod `kind` enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TombstoneKind {
    CheckpointRevert,
    Summarization,
}

/// `_meta` field carried on persisted payloads. Mirrors
/// `KiroPersistedMetaSchema` wrapped in the outer `_meta` envelope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedPayloadMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kiro: Option<KiroPersistedMeta>,
}

/// Subset of the `_meta.kiro` shape. Only fields this crate writes
/// are listed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroPersistedMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_message_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Required fields round-trip through serialize/deserialize
    /// without loss. Optional fields absent from input stay absent
    /// from output.
    #[test]
    fn metadata_required_fields_round_trip() {
        let raw = json!({
            "schemaVersion": "1.0.0",
            "id": "cli_abc_12345678",
            "title": "test session",
            "agentMode": "vibe",
            "workspacePaths": ["/tmp/ws"],
            "createdAt": "2024-01-01T00:00:00Z",
            "lastModifiedAt": "2024-01-01T00:00:00Z",
        });
        let parsed: SessionMetadata = serde_json::from_value(raw.clone()).expect("parse");
        let reserialized = serde_json::to_value(&parsed).expect("serialize");
        assert_eq!(reserialized, raw);
    }

    /// `modelId` deserializes when present and serializes back out.
    #[test]
    fn metadata_round_trips_model_id() {
        let raw = json!({
            "schemaVersion": "1.0.0",
            "id": "cli_abc_12345678",
            "title": "test",
            "agentMode": "vibe",
            "workspacePaths": ["/tmp/ws"],
            "createdAt": "2024-01-01T00:00:00Z",
            "lastModifiedAt": "2024-01-01T00:00:00Z",
            "modelId": "claude-sonnet-4.5",
        });
        let parsed: SessionMetadata = serde_json::from_value(raw).expect("parse");
        assert_eq!(parsed.model_id.as_deref(), Some("claude-sonnet-4.5"));
        let out = serde_json::to_value(&parsed).expect("serialize");
        assert_eq!(out["modelId"], "claude-sonnet-4.5");
    }

    /// `createdReason` accepts every KAS-defined enum value.
    #[test]
    fn metadata_created_reason_accepts_all_kas_values() {
        for reason in ["human", "rewind", "subagent", "thread"] {
            let raw = json!({
                "schemaVersion": "1.0.0",
                "id": "cli_abc_12345678",
                "title": "test",
                "agentMode": "vibe",
                "workspacePaths": ["/tmp/ws"],
                "createdAt": "2024-01-01T00:00:00Z",
                "lastModifiedAt": "2024-01-01T00:00:00Z",
                "createdReason": reason,
            });
            let parsed: SessionMetadata = serde_json::from_value(raw).unwrap_or_else(|e| panic!("parse {reason}: {e}"));
            let out = serde_json::to_value(&parsed).expect("serialize");
            assert_eq!(out["createdReason"], reason);
        }
    }

    /// Fields not named in [`SessionMetadata`] survive a
    /// parse/reserialize cycle so a `/chat load` rewrite of `id`
    /// and `workspacePaths` doesn't drop arbitrary metadata.
    #[test]
    fn metadata_preserves_unknown_fields() {
        let raw = json!({
            "schemaVersion": "1.0.0",
            "id": "cli_abc_12345678",
            "title": "test",
            "agentMode": "vibe",
            "workspacePaths": ["/tmp/ws"],
            "createdAt": "2024-01-01T00:00:00Z",
            "lastModifiedAt": "2024-01-01T00:00:00Z",
            "futureFieldA": "hello",
            "futureFieldB": { "nested": [1, 2, 3] },
        });
        let parsed: SessionMetadata = serde_json::from_value(raw).expect("parse");
        let out = serde_json::to_value(&parsed).expect("serialize");
        assert_eq!(out["futureFieldA"], "hello");
        assert_eq!(out["futureFieldB"]["nested"][1], 2);
    }

    /// Mutating `id` and `workspacePaths` doesn't drop other fields -
    /// the canonical `/chat load` rewrite use case.
    #[test]
    fn metadata_id_rewrite_preserves_other_fields() {
        let raw = json!({
            "schemaVersion": "1.0.0",
            "id": "cli_abc_12345678",
            "title": "preserved title",
            "agentMode": "quick-plan",
            "workspacePaths": ["/old/ws"],
            "createdAt": "2024-01-01T00:00:00Z",
            "lastModifiedAt": "2024-01-01T00:00:00Z",
            "modelId": "claude-opus-4.7",
            "createdReason": "subagent",
            "futureField": "still-here",
        });
        let mut parsed: SessionMetadata = serde_json::from_value(raw).expect("parse");
        parsed.id = "cli_xyz_87654321".to_string();
        parsed.workspace_paths = vec!["/new/ws".to_string()];
        let out = serde_json::to_value(&parsed).expect("serialize");
        assert_eq!(out["id"], "cli_xyz_87654321");
        assert_eq!(out["workspacePaths"][0], "/new/ws");
        assert_eq!(out["title"], "preserved title");
        assert_eq!(out["agentMode"], "quick-plan");
        assert_eq!(out["modelId"], "claude-opus-4.7");
        assert_eq!(out["createdReason"], "subagent");
        assert_eq!(out["futureField"], "still-here");
    }

    /// KAS metadata fields not modelled in [`SessionMetadata`] are
    /// not emitted on serialize, even if some other KAS-side writer
    /// might still produce them.
    #[test]
    fn metadata_does_not_emit_dropped_fields() {
        let metadata = SessionMetadata {
            schema_version: "1.0.0".to_string(),
            id: "cli_abc_12345678".to_string(),
            title: "test".to_string(),
            agent_mode: "vibe".to_string(),
            workspace_paths: vec!["/tmp/ws".to_string()],
            created_at: "2024-01-01T00:00:00Z".to_string(),
            last_modified_at: "2024-01-01T00:00:00Z".to_string(),
            data_model_version: Some(1),
            model_id: None,
            created_reason: None,
            extra: serde_json::Map::new(),
        };
        let out = serde_json::to_value(&metadata).expect("serialize");
        let obj = out.as_object().expect("object");
        for dropped in [
            "lastCheckpointId",
            "parentSessionId",
            "parentExecutionId",
            "repositories",
            "effortLevel",
            "semanticReviewEnabled",
        ] {
            assert!(!obj.contains_key(dropped), "{dropped} should not be emitted");
        }
    }

    /// Each [`CreatedReason`] variant serializes to its lowercase
    /// JSON form, matching KAS's `createdReason` enum on the wire.
    #[test]
    fn created_reason_serializes_to_lowercase_strings() {
        assert_eq!(serde_json::to_value(CreatedReason::Human).unwrap(), json!("human"));
        assert_eq!(serde_json::to_value(CreatedReason::Rewind).unwrap(), json!("rewind"));
        assert_eq!(
            serde_json::to_value(CreatedReason::Subagent).unwrap(),
            json!("subagent")
        );
        assert_eq!(serde_json::to_value(CreatedReason::Thread).unwrap(), json!("thread"));
    }
}
