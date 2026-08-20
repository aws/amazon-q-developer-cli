//! The `--output-format stream-json` wire contract.
//!
//! One JSON object per line on stdout. Every line is an adjacently-tagged envelope
//! `{ "type": <event>, "data": <payload> }`; [`StreamJsonEvent`] is the single source of
//! truth for the five event types so their shapes cannot drift independently.
//!
//! Two layers:
//! - A lifecycle envelope owned by this feature and stable across engines: `runStarted`,
//!   `runFinished`, `runError`, `metadata`.
//! - `sessionUpdate` records, which carry the engine's ACP [`acp::SessionUpdate`] verbatim.
//!   Consumers branch on `runStarted.engine` to parse the engine-specific payload.
//!
//! We deliberately do not version the stream: `sessionUpdate` forwards ACP events as-is, so we
//! can't own a schema for them (v3's payloads evolve independently). Instead `runStarted`
//! declares exactly what is being forwarded via `payloadSchema` + `acpProtocolVersion`, so a
//! consumer knows it is reading ACP events at that protocol version.

use agent_client_protocol as acp;
use serde::Serialize;

/// The payload schema `sessionUpdate` records carry. Only ACP today; a field rather than a
/// literal so the value is stated once and a consumer can branch on it.
pub(crate) const PAYLOAD_SCHEMA_ACP: &str = "acp";

/// The `stage` a stream-json `runError` reports: where in the run's lifecycle it failed.
/// Enumerated so a new stage is a compile-time decision, not a stray literal. A consumer must
/// treat an unknown stage as a generic failure rather than matching the set exhaustively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunErrorStage {
    /// Not authenticated (pre-launch).
    Auth,
    /// Engine resolution rejected the run, e.g. stream-json on v1 (pre-launch).
    Engine,
    /// `--cloud`/`--repo` is unsupported non-interactively (pre-launch).
    Cloud,
    /// The prompt input could not be resolved from args or stdin (pre-launch).
    Input,
    /// Spawning the ACP subprocess or wiring up its stdio failed, before the session
    /// protocol began and `runStarted` was written (pre-launch).
    Launch,
    /// ACP session initialization or creation failed (in-session).
    Init,
    /// The turn itself failed after the session started (in-session).
    Prompt,
}

/// Terminal status reported on `runFinished`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunStatus {
    Success,
    Error,
}

/// One line of the stream-json output. Adjacently tagged, so it serializes as
/// `{ "type": "<variant>", "data": { ... } }`.
#[derive(Debug, Serialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum StreamJsonEvent<'a> {
    /// First record of the stream. Declares what the `sessionUpdate` records forward
    /// (`payloadSchema` = "acp" at `acpProtocolVersion`) and the resolved engine, so a consumer
    /// knows it is reading ACP events at that protocol version rather than a schema we own.
    RunStarted {
        payload_schema: &'a str,
        acp_protocol_version: u16,
        engine: &'a str,
    },

    /// An ACP session update, carried verbatim under `update` alongside its session id so a
    /// consumer can correlate the record to its run.
    SessionUpdate {
        session_id: String,
        update: &'a acp::SessionUpdate,
    },

    /// Per-turn metering/usage and context data passed through from the agent's
    /// `_kiro.dev/metadata` ext-notification.
    Metadata(serde_json::Value),

    /// Terminal error record. `sessionId` is null before a session exists (pre-launch or init).
    RunError {
        session_id: Option<String>,
        stage: RunErrorStage,
        message: String,
    },

    /// Terminal success record: the run's final summary.
    RunFinished {
        session_id: String,
        status: RunStatus,
        stop_reason: serde_json::Value,
        final_text: String,
        final_text_truncated: bool,
    },
}

impl<'a> StreamJsonEvent<'a> {
    /// A pre-launch `runError` (no session yet): a null `sessionId` and the failure stage.
    pub(crate) fn pre_launch_error(stage: RunErrorStage, message: String) -> Self {
        StreamJsonEvent::RunError {
            session_id: None,
            stage,
            message,
        }
    }

    /// An in-session `runError`. `session_id` is `None` before session creation succeeds,
    /// `Some` afterward.
    pub(crate) fn in_session_error(session_id: Option<String>, stage: RunErrorStage, message: String) -> Self {
        StreamJsonEvent::RunError {
            session_id,
            stage,
            message,
        }
    }

    /// Serialize to a single JSON line (no trailing newline). Every field is infallible to
    /// serialize (strings, primitives, a passthrough `Value`), so the error arm is defensive:
    /// if it ever fires, degrade to a still-typed envelope with null `data` so a consumer can
    /// always read `type`, and log so the failure is not silent.
    pub(crate) fn to_line(&self) -> String {
        match serde_json::to_string(self) {
            Ok(line) => line,
            Err(err) => {
                let event_type = self.type_name();
                tracing::warn!(%err, event = event_type, "stream-json payload failed to serialize; emitting null data");
                serde_json::json!({ "type": event_type, "data": serde_json::Value::Null }).to_string()
            },
        }
    }

    /// The `type` discriminator, for the serialize-failure fallback line.
    fn type_name(&self) -> &'static str {
        match self {
            StreamJsonEvent::RunStarted { .. } => "runStarted",
            StreamJsonEvent::SessionUpdate { .. } => "sessionUpdate",
            StreamJsonEvent::Metadata(_) => "metadata",
            StreamJsonEvent::RunError { .. } => "runError",
            StreamJsonEvent::RunFinished { .. } => "runFinished",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(line: &str) -> serde_json::Value {
        let parsed: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
        assert!(!line.contains('\n'), "line must be a single JSONL record: {line}");
        parsed
    }

    #[test]
    fn run_started_declares_acp_payload_and_engine() {
        let line = StreamJsonEvent::RunStarted {
            payload_schema: PAYLOAD_SCHEMA_ACP,
            acp_protocol_version: 1,
            engine: "v2",
        }
        .to_line();
        let v = data(&line);
        assert_eq!(v["type"], "runStarted");
        assert_eq!(v["data"]["payloadSchema"], "acp");
        assert_eq!(v["data"]["acpProtocolVersion"], 1);
        assert_eq!(v["data"]["engine"], "v2");
        // We do not claim to own a schema for forwarded ACP events.
        assert!(v["data"].get("schemaVersion").is_none());
    }

    #[test]
    fn session_update_wraps_acp_update_with_session_id() {
        let update = acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(acp::ContentBlock::Text(
            acp::TextContent::new("hello".to_string()),
        )));
        let line = StreamJsonEvent::SessionUpdate {
            session_id: "sess-1".to_string(),
            update: &update,
        }
        .to_line();
        let v = data(&line);
        assert_eq!(v["type"], "sessionUpdate");
        assert_eq!(v["data"]["sessionId"], "sess-1");
        // The ACP update is nested under `update`, tagged by its `sessionUpdate` discriminator.
        assert_eq!(v["data"]["update"]["sessionUpdate"], "agent_message_chunk");
        assert_eq!(v["data"]["update"]["content"]["text"], "hello");
    }

    #[test]
    fn pre_launch_error_has_stage_and_null_session() {
        let line = StreamJsonEvent::pre_launch_error(RunErrorStage::Auth, "Not logged in.".to_string()).to_line();
        let v = data(&line);
        assert_eq!(v["type"], "runError");
        assert!(v["data"]["sessionId"].is_null());
        assert_eq!(v["data"]["stage"], "auth");
        assert_eq!(v["data"]["message"], "Not logged in.");
    }

    #[test]
    fn in_session_error_carries_session_id() {
        let line =
            StreamJsonEvent::in_session_error(Some("sess-1".to_string()), RunErrorStage::Prompt, "boom".to_string())
                .to_line();
        let v = data(&line);
        assert_eq!(v["type"], "runError");
        assert_eq!(v["data"]["sessionId"], "sess-1");
        assert_eq!(v["data"]["stage"], "prompt");
        assert_eq!(v["data"]["message"], "boom");
    }

    #[test]
    fn init_error_before_session_has_null_session_id() {
        let line = StreamJsonEvent::in_session_error(None, RunErrorStage::Init, "no cwd".to_string()).to_line();
        let v = data(&line);
        assert!(v["data"]["sessionId"].is_null());
        assert_eq!(v["data"]["stage"], "init");
    }

    #[test]
    fn run_finished_summary_shape() {
        let line = StreamJsonEvent::RunFinished {
            session_id: "sess-1".to_string(),
            status: RunStatus::Success,
            stop_reason: serde_json::to_value(acp::StopReason::EndTurn).unwrap(),
            final_text: "done".to_string(),
            final_text_truncated: false,
        }
        .to_line();
        let v = data(&line);
        assert_eq!(v["type"], "runFinished");
        assert_eq!(v["data"]["sessionId"], "sess-1");
        assert_eq!(v["data"]["status"], "success");
        // stopReason uses ACP-canonical snake_case, matching every other event.
        assert_eq!(v["data"]["stopReason"], "end_turn");
        assert_eq!(v["data"]["finalText"], "done");
        assert_eq!(v["data"]["finalTextTruncated"], false);
    }

    #[test]
    fn metadata_passes_payload_through() {
        let line = StreamJsonEvent::Metadata(serde_json::json!({ "meteringUsage": [] })).to_line();
        let v = data(&line);
        assert_eq!(v["type"], "metadata");
        assert!(v["data"]["meteringUsage"].is_array());
    }

    #[test]
    fn every_stage_maps_to_its_wire_string() {
        for (stage, expected) in [
            (RunErrorStage::Auth, "auth"),
            (RunErrorStage::Engine, "engine"),
            (RunErrorStage::Cloud, "cloud"),
            (RunErrorStage::Input, "input"),
            (RunErrorStage::Launch, "launch"),
            (RunErrorStage::Init, "init"),
            (RunErrorStage::Prompt, "prompt"),
        ] {
            assert_eq!(
                serde_json::to_value(stage).unwrap(),
                serde_json::Value::String(expected.to_string())
            );
        }
    }
}
