//! The scripted-scenario model: the request/response pairs a test injects
//! through the control API.
//!
//! A scenario is a queue of [`Turn`]s. Each turn pairs an optional request
//! [`Matcher`] with the [`Respond`] the server plays back for it, so a test can
//! either script a strictly ordered conversation (no matchers — first queued
//! turn answers the next call) or bind responses to the request that should
//! receive them (matchers — order-independent, which is what multi-turn agent
//! flows need once tool calls make the call order hard to predict).
//!
//! Everything here is plain serde, wire-compatible with the JSON a test POSTs
//! to `/__control/scenarios`. The modeled KRS types are built from these
//! descriptions in [`crate::wire`], not here, so a scenario file stays readable
//! and free of event-stream detail.

use serde::{
    Deserialize,
    Serialize,
};

/// Parses the body of `POST /__control/scenarios`.
///
/// Accepts either `{"turns": [...]}` or a bare `[...]`, branching on the JSON
/// shape rather than deriving `#[serde(untagged)]`: untagged collapses every
/// failure into "data did not match any variant", which hides the one thing a
/// caller needs — the name of the field they got wrong.
pub fn parse_scenario_batch(body: &[u8]) -> Result<Vec<Turn>, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_slice(body)?;
    match value.get("turns") {
        Some(turns) => serde_json::from_value(turns.clone()),
        None => serde_json::from_value(value),
    }
}

/// One scripted request/response pair.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Turn {
    /// Free-form label echoed in `/__control/state` and in the "no scripted
    /// response" error, so a failing test names the turn it expected.
    #[serde(default)]
    pub name: Option<String>,

    /// Which requests this turn answers. Absent (or empty) matches anything.
    #[serde(default, rename = "match")]
    pub matcher: Matcher,

    /// How many requests this turn may answer before it is consumed. Defaults
    /// to 1. `0` means unlimited — the turn is sticky and never consumed, which
    /// is what a background call that fires an unpredictable number of times
    /// (title generation, compaction) needs.
    #[serde(default)]
    pub times: Option<u32>,

    pub respond: Respond,
}

impl Turn {
    /// Remaining uses, where `None` means unlimited.
    pub fn budget(&self) -> Option<u32> {
        match self.times {
            None => Some(1),
            Some(0) => None,
            Some(n) => Some(n),
        }
    }

    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| "<unnamed>".to_string())
    }
}

/// Request predicates. All present fields must match (AND).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Matcher {
    /// Substring of `conversationState.currentMessage.userInputMessage.content`.
    #[serde(default)]
    pub user_input_contains: Option<String>,

    /// Regex over the same user-input content.
    #[serde(default)]
    pub user_input_regex: Option<String>,

    /// Substring of the whole raw request body. The escape hatch for asserting
    /// on parts of the request this struct does not model yet (history shape,
    /// advertised tools, images) without growing a field per case.
    #[serde(default)]
    pub body_contains: Option<String>,

    /// `true` requires the request to carry at least one tool result (i.e. it
    /// is the continuation of a tool-using turn); `false` requires none.
    #[serde(default)]
    pub has_tool_results: Option<bool>,

    /// Exact match on `conversationState.currentMessage.userInputMessage.modelId`.
    #[serde(default)]
    pub model_id: Option<String>,

    /// Exact match on `agentMode`, which the model binds to the
    /// `x-amzn-kiro-agent-mode` request header rather than the body.
    #[serde(default)]
    pub agent_mode: Option<String>,

    /// Zero-based index of the GenerateAssistantResponse call, counted since
    /// the last reset.
    #[serde(default)]
    pub call_index: Option<usize>,
}

impl Matcher {
    pub fn is_empty(&self) -> bool {
        self.user_input_contains.is_none()
            && self.user_input_regex.is_none()
            && self.body_contains.is_none()
            && self.has_tool_results.is_none()
            && self.model_id.is_none()
            && self.agent_mode.is_none()
            && self.call_index.is_none()
    }
}

/// What the server plays back for a matched turn.
///
/// The healthy path is `events` alone: the server frames each event and then
/// guarantees a terminal `metadata` event carrying a `stopReason` if the script
/// did not include one. That default is deliberate — real KRS always closes a
/// complete response with a stop reason, and a stream that ends without one
/// trips KAS's stream-recovery retry, which silently doubles invocation counts
/// and makes a test look flaky rather than wrong. Tests that *want* that
/// behavior ask for it with `truncate`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Respond {
    /// The event sequence, in order.
    #[serde(default)]
    pub events: Vec<EventScript>,

    /// End the stream after `events` without appending a terminal stop reason.
    /// Models a truncated/mid-flight close.
    #[serde(default)]
    pub truncate: bool,

    /// Emit a modeled exception frame instead of finishing the stream. Applied
    /// after `events`, so a test can stream partial content and then fail.
    #[serde(default)]
    pub stream_error: Option<StreamError>,

    /// Fail the request with a modeled error instead of streaming. Takes
    /// precedence over `events`.
    #[serde(default)]
    pub http_error: Option<HttpError>,

    /// Delay before the first byte of the response.
    #[serde(default)]
    pub delay_ms: u64,

    /// Delay between event frames. Lets a test observe incremental rendering
    /// instead of a single flush.
    #[serde(default)]
    pub chunk_delay_ms: u64,
}

/// A modeled member of the `ChatResponseStream` union, in scenario form.
///
/// `rename_all_fields` is needed as well as `rename_all`: the latter renames the
/// variants (`toolUse`), the former their fields (`toolUseId`). Without it a
/// scenario would be camelCase everywhere except inside an event.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum EventScript {
    /// `assistantResponseEvent` — a chunk of assistant markdown.
    Text {
        content: String,
        #[serde(default)]
        model_id: Option<String>,
        #[serde(default)]
        model_tag: Option<String>,
    },

    /// `reasoningContentEvent` — a chunk of model reasoning.
    Reasoning {
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        signature: Option<String>,
        #[serde(default)]
        redacted_content: Option<String>,
    },

    /// `toolUseEvent` — a tool-call chunk. `input` is concatenated by the
    /// client across events until one arrives with `stop: true`, so a scenario
    /// can split a single call across frames to exercise that reassembly.
    ToolUse {
        tool_use_id: String,
        name: String,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        stop: Option<bool>,
    },

    /// `meteringEvent` — credit usage.
    Metering {
        #[serde(default)]
        usage: Option<f64>,
        #[serde(default)]
        unit: Option<String>,
        #[serde(default)]
        unit_plural: Option<String>,
    },

    /// `contextUsageEvent` — percentage of the context window used.
    ContextUsage {
        #[serde(default)]
        context_usage_percentage: Option<f32>,
    },

    /// `metadataEvent` — token usage and/or the terminal stop reason.
    Metadata {
        #[serde(default)]
        stop_reason: Option<String>,
        #[serde(default)]
        token_usage: Option<TokenUsageScript>,
    },

    /// Not an event: pause before the next one. Distinct from
    /// `Respond::chunk_delay_ms`, which paces every frame uniformly.
    Delay { ms: u64 },
}

/// `TokenUsage` in scenario form. The three required members carry zeroes by
/// default so a scenario that only cares about a stop reason stays short.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenUsageScript {
    #[serde(default)]
    pub uncached_input_tokens: i32,
    #[serde(default)]
    pub output_tokens: i32,
    #[serde(default)]
    pub total_tokens: i32,
    #[serde(default)]
    pub cache_read_input_tokens: Option<i32>,
    #[serde(default)]
    pub cache_write_input_tokens: Option<i32>,
    #[serde(default)]
    pub context_usage_percentage: Option<f32>,
    #[serde(default)]
    pub normalized_token_usage: Option<f32>,
}

/// A mid-stream exception frame. Each variant names a member of the modeled
/// `ChatResponseStream` error union; the generated SDK decides the
/// `:exception-type` header it is framed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamErrorKind {
    /// `InternalServerException` (renamed `InternalServerError` in the Rust client).
    Internal,
    Throttling,
    Validation,
    ServiceUnavailable,
}

impl StreamErrorKind {
    /// The modeled shape name, used for traceable default messages.
    pub fn shape_name(self) -> &'static str {
        match self {
            Self::Internal => "InternalServerException",
            Self::Throttling => "ThrottlingException",
            Self::Validation => "ValidationException",
            Self::ServiceUnavailable => "ServiceUnavailableException",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamError {
    pub kind: StreamErrorKind,
    #[serde(default)]
    pub message: Option<String>,
}

/// One of the errors the model says `GenerateAssistantResponse` can return.
///
/// A closed set on purpose: the mock can only fail the way KRS can fail, so a
/// scenario cannot script a response KAS would never see in production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HttpErrorKind {
    AccessDenied,
    InternalServer,
    ServiceQuotaExceeded,
    ServiceUnavailable,
    Throttling,
    Validation,
}

/// A modeled error response. The status code and body shape come from the
/// generated server SDK, not from this crate.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpError {
    pub kind: HttpErrorKind,
    #[serde(default)]
    pub message: Option<String>,
}

impl HttpError {
    /// Message to send, falling back to something traceable to this crate.
    pub fn message(&self) -> String {
        self.message
            .clone()
            .unwrap_or_else(|| format!("mock KRS injected {:?}", self.kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_wrapped_and_bare_batches() {
        let turn = r#"{"respond":{"events":[{"type":"text","content":"hi"}]}}"#;
        let wrapped = parse_scenario_batch(format!(r#"{{"turns":[{turn}]}}"#).as_bytes()).unwrap();
        let bare = parse_scenario_batch(format!("[{turn}]").as_bytes()).unwrap();
        assert_eq!(wrapped.len(), 1);
        assert_eq!(bare.len(), 1);
    }

    #[test]
    fn a_bad_field_is_named_in_the_error() {
        let error = parse_scenario_batch(br#"[{"respond":{"evnets":[]}}]"#).unwrap_err();
        assert!(error.to_string().contains("evnets"), "{error}");
    }

    #[test]
    fn times_maps_to_a_budget() {
        let mk = |json: &str| -> Turn { serde_json::from_str(json).unwrap() };
        assert_eq!(mk(r#"{"respond":{}}"#).budget(), Some(1));
        assert_eq!(mk(r#"{"times":3,"respond":{}}"#).budget(), Some(3));
        assert_eq!(mk(r#"{"times":0,"respond":{}}"#).budget(), None);
    }

    #[test]
    fn unknown_scenario_fields_are_rejected() {
        // A typo in a scenario must fail loudly at injection time; silently
        // ignoring it would produce a mystery timeout deep in a TUI test.
        let err = serde_json::from_str::<Turn>(r#"{"respnod":{}}"#).unwrap_err();
        assert!(err.to_string().contains("unknown field"), "{err}");
    }

    #[test]
    fn http_errors_are_restricted_to_the_modeled_set() {
        let throttle: HttpError = serde_json::from_str(r#"{"kind":"throttling"}"#).unwrap();
        assert_eq!(throttle.kind, HttpErrorKind::Throttling);
        assert!(throttle.message().contains("mock KRS injected"));

        // An error KRS cannot return is rejected when the scenario is loaded,
        // rather than producing a response KAS would never see in production.
        assert!(serde_json::from_str::<HttpError>(r#"{"kind":"imATeapot"}"#).is_err());
    }
}
