use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Map;
use uuid::Uuid;

use crate::api_client::error::{
    ApiClientError,
    ConverseStreamError,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamEvent {
    MessageStart(MessageStartEvent),
    MessageStop(MessageStopEvent),
    ContentBlockStart(ContentBlockStartEvent),
    ContentBlockDelta(ContentBlockDeltaEvent),
    ContentBlockStop(ContentBlockStopEvent),
    Metadata(MetadataEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamError {
    /// The request id returned by the model provider, if available
    pub original_request_id: Option<String>,
    /// The HTTP status code returned by model provider, if available
    pub original_status_code: Option<u16>,
    /// Exact error message returned by the model provider, if available
    pub original_message: Option<String>,
    pub kind: StreamErrorKind,
    #[serde(skip)]
    pub source: Option<Arc<dyn StreamErrorSource>>,
}

impl StreamError {
    pub fn new(kind: StreamErrorKind) -> Self {
        Self {
            kind,
            original_request_id: None,
            original_status_code: None,
            original_message: None,
            source: None,
        }
    }

    pub fn set_original_request_id(mut self, id: Option<String>) -> Self {
        self.original_request_id = id;
        self
    }

    pub fn set_original_status_code(mut self, id: Option<u16>) -> Self {
        self.original_status_code = id;
        self
    }

    pub fn set_original_message(mut self, id: Option<String>) -> Self {
        self.original_message = id;
        self
    }

    pub fn with_source(mut self, source: Arc<dyn StreamErrorSource>) -> Self {
        self.source = Some(source);
        self
    }

    /// Helper for downcasting the error source to [ConverseStreamError].
    ///
    /// Just defining this here for simplicity
    pub fn as_rts_error(&self) -> Option<&ConverseStreamError> {
        if let Some(source) = &self.source {
            (*source).as_any().downcast_ref::<ConverseStreamError>()
        } else {
            None
        }
    }
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Encountered an error in the response stream: ")?;
        if let Some(request_id) = self.original_request_id.as_ref() {
            write!(f, "request_id: {}, error: ", request_id)?;
        }
        if let Some(source) = self.source.as_ref() {
            write!(f, "{}", source)?;
        }
        Ok(())
    }
}

impl std::error::Error for StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|s| s.as_ref() as &(dyn std::error::Error + 'static))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamErrorKind {
    /// The request failed due to the context window overflowing.
    ///
    /// Q CLI by default will attempt to auto-summarize the conversation, and then retry the
    /// request.
    ContextWindowOverflow,
    /// The service failed for some reason.
    ///
    /// Should be returned for 5xx errors.
    ServiceFailure,
    /// The request failed due to the client being throttled.
    Throttling,
    /// The request was invalid.
    ///
    /// Not retryable - indicative of a bug with the client.
    Validation {
        /// Custom error message, if available
        message: Option<String>,
    },
    /// The stream timed out after some relatively long period of time.
    ///
    /// Q CLI currently retries these errors using some conversation fakery:
    /// 1. Add a new assistant message: `"Response timed out - message took too long to generate"`
    /// 2. Retry with a follow-up user message: `"You took too long to respond - try to split up the
    ///    work into smaller steps."`
    StreamTimeout { duration: Duration },
    /// The stream was closed to due being interrupted (for example, on ctrl+c).
    Interrupted,
    /// Catch-all for errors not modeled in [StreamErrorKind].
    Other(String),
}

impl std::fmt::Display for StreamErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg: Cow<'_, str> = match self {
            StreamErrorKind::ContextWindowOverflow => "The context window overflowed".into(),
            StreamErrorKind::ServiceFailure => "The service failed to process the request".into(),
            StreamErrorKind::Throttling => "The request was throttled by the service".into(),
            StreamErrorKind::Validation { .. } => "An invalid request was sent".into(),
            StreamErrorKind::StreamTimeout { duration } => format!(
                "The stream timed out receiving the response after {}ms",
                duration.as_millis()
            )
            .into(),
            StreamErrorKind::Interrupted => "The stream was interrupted".into(),
            StreamErrorKind::Other(msg) => msg.as_str().into(),
        };
        write!(f, "{}", msg)
    }
}

pub trait StreamErrorSource: std::any::Any + std::error::Error + Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
}

impl StreamErrorSource for ConverseStreamError {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl StreamErrorSource for ApiClientError {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: Option<String>,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub timestamp: Option<DateTime<Utc>>,
}

impl Message {
    /// Creates a new message with a new id
    pub fn new(role: Role, content: Vec<ContentBlock>, timestamp: Option<DateTime<Utc>>) -> Self {
        Self {
            id: Some(Uuid::new_v4().to_string()),
            role,
            content,
            timestamp,
        }
    }

    /// Returns only the text content, joined as a single string.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|v| match v {
                ContentBlock::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Returns a non-empty vector of [ToolUseBlock] if this message contains tool uses,
    /// otherwise [None].
    pub fn tool_uses(&self) -> Option<Vec<ToolUseBlock>> {
        let mut results = vec![];
        for c in &self.content {
            if let ContentBlock::ToolUse(v) = c {
                results.push(v.clone());
            }
        }
        if results.is_empty() { None } else { Some(results) }
    }

    /// Returns a non-empty vector of [ToolResultBlock] if this message contains tool results,
    /// otherwise [None].
    pub fn tool_results(&self) -> Option<Vec<ToolResultBlock>> {
        let mut results = vec![];
        for c in &self.content {
            if let ContentBlock::ToolResult(r) = c {
                results.push(r.clone());
            }
        }
        if results.is_empty() { None } else { Some(results) }
    }

    /// Returns a non-empty vector of [ImageBlock] if this message contains images,
    /// otherwise [None].
    pub fn images(&self) -> Option<Vec<ImageBlock>> {
        let mut results = vec![];
        for c in &self.content {
            if let ContentBlock::Image(img) = c {
                results.push(img.clone());
            }
        }
        if results.is_empty() { None } else { Some(results) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentBlock {
    Text(String),
    ToolUse(ToolUseBlock),
    ToolResult(ToolResultBlock),
    Image(ImageBlock),
}

impl From<String> for ContentBlock {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct ImageBlock {
    pub format: ImageFormat,
    pub source: ImageSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ImageFormat {
    Gif,
    #[serde(alias = "jpg")]
    Jpeg,
    Png,
    Webp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageSource {
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseBlock {
    /// Identifier for the tool use
    pub tool_use_id: String,
    /// Name of the tool
    pub name: String,
    /// The input to pass to the tool
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: Vec<ToolResultContentBlock>,
    pub status: ToolResultStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolResultContentBlock {
    Text(String),
    Json(serde_json::Value),
    Image(ImageBlock),
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolResultStatus {
    Error,
    Success,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageStartEvent {
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageStopEvent {
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum StopReason {
    ToolUse,
    EndTurn,
    MaxTokens,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockStartEvent {
    pub content_block_start: Option<ContentBlockStart>,
    /// Index of the content block within the message. This is optional to accommodate different
    /// model providers.
    pub content_block_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentBlockStart {
    ToolUse(ToolUseBlockStart),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseBlockStart {
    /// Identifier for the tool use
    pub tool_use_id: String,
    /// Name of the tool
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockDeltaEvent {
    pub delta: ContentBlockDelta,
    /// Index of the content block within the message. This is optional to accommodate different
    /// model providers.
    pub content_block_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentBlockDelta {
    Text(String),
    ToolUse(ToolUseBlockDelta),
    // todo?
    Reasoning,
    Document,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseBlockDelta {
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockStopEvent {
    /// Index of the content block within the message. This is optional to accommodate different
    /// model providers.
    pub content_block_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataEvent {
    pub metrics: Option<MetadataMetrics>,
    pub usage: Option<MetadataUsage>,
    pub service: Option<MetadataService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataMetrics {
    pub time_to_first_chunk: Option<Duration>,
    pub time_between_chunks: Option<Vec<Duration>>,
    pub response_stream_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_write_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataService {
    pub request_id: Option<String>,
    pub status_code: Option<u16>,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::api_client::error::ConverseStreamErrorKind;

    macro_rules! test_ser_deser {
        ($ty:ident, $variant:expr, $text:expr) => {
            let quoted = format!("\"{}\"", $text);
            assert_eq!(quoted, serde_json::to_string(&$variant).unwrap());
            assert_eq!($variant, serde_json::from_str(&quoted).unwrap());
            assert_eq!($variant, $ty::from_str($text).unwrap());
            assert_eq!($text, $variant.to_string());
        };
    }

    #[test]
    fn test_other_stream_err_downcasting() {
        let err = StreamError::new(StreamErrorKind::Interrupted).with_source(Arc::new(ConverseStreamError::new(
            ConverseStreamErrorKind::ModelOverloadedError,
            None::<aws_smithy_types::error::operation::BuildError>, /* annoying type inference
                                                                     * required */
        )));
        assert!(
            err.as_rts_error()
                .is_some_and(|r| matches!(r.kind, ConverseStreamErrorKind::ModelOverloadedError))
        );
    }

    #[test]
    fn test_image_format_ser_deser() {
        test_ser_deser!(ImageFormat, ImageFormat::Gif, "gif");
        test_ser_deser!(ImageFormat, ImageFormat::Png, "png");
        test_ser_deser!(ImageFormat, ImageFormat::Webp, "webp");
        test_ser_deser!(ImageFormat, ImageFormat::Jpeg, "jpeg");
        assert_eq!(ImageFormat::from_str("jpg").unwrap(), ImageFormat::Jpeg);
    }
}
