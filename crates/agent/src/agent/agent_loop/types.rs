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
use tracing::error;
use typeshare::typeshare;
use uuid::Uuid;

use crate::agent::util::truncate_safe_in_place;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum StreamEvent {
    MessageStart(MessageStartEvent),
    MessageStop(MessageStopEvent),
    ContentBlockStart(ContentBlockStartEvent),
    ContentBlockDelta(ContentBlockDeltaEvent),
    ContentBlockStop(ContentBlockStopEvent),
    Metadata(MetadataEvent),
}

#[typeshare]
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

    /// Helper for downcasting a [StreamErrorSource] to a concrete type.
    pub fn as_concrete_error<T: StreamErrorSource>(&self) -> Option<&T> {
        if let Some(source) = &self.source {
            (*source).as_any().downcast_ref::<T>()
        } else {
            None
        }
    }
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Encountered an error in the response stream: ")?;
        if let Some(request_id) = self.original_request_id.as_ref() {
            write!(f, "request_id: {request_id}, error: ")?;
        }
        // Always include the kind message for better error context
        write!(f, "{}", self.kind)?;
        // Include original message if available for more detail
        if let Some(original_message) = self.original_message.as_ref() {
            write!(f, " - {original_message}")?;
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

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
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
        write!(f, "{msg}")
    }
}

pub trait StreamErrorSource: std::any::Any + std::error::Error + Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    #[serde(default)]
    pub id: Option<String>,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    #[serde(default)]
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

    pub fn tool_uses_iter(&self) -> impl Iterator<Item = &ToolUseBlock> {
        self.content.iter().filter_map(|c| match c {
            ContentBlock::ToolUse(block) => Some(block),
            _ => None,
        })
    }

    /// Returns a [ToolUseBlock] for the given `tool_use_id` if it exists.
    pub fn get_tool_use(&self, tool_use_id: impl AsRef<str>) -> Option<&ToolUseBlock> {
        self.content.iter().find_map(|v| match v {
            ContentBlock::ToolUse(block) if block.tool_use_id == tool_use_id.as_ref() => Some(block),
            _ => None,
        })
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

    pub fn tool_results_iter(&self) -> impl Iterator<Item = &ToolResultBlock> {
        self.content.iter().filter_map(|c| match c {
            ContentBlock::ToolResult(block) => Some(block),
            _ => None,
        })
    }

    /// Returns a [ToolResultBlock] for the given `tool_use_id` if it exists.
    pub fn get_tool_result(&self, tool_use_id: impl AsRef<str>) -> Option<&ToolResultBlock> {
        self.content.iter().find_map(|v| match v {
            ContentBlock::ToolResult(block) if block.tool_use_id == tool_use_id.as_ref() => Some(block),
            _ => None,
        })
    }

    /// Replaces the [ContentBlock::ToolResult] with the given `tool_use_id` to instead be a
    /// [ContentBlock::Text] and [ContentBlock::Image].
    pub fn replace_tool_result_as_content(&mut self, tool_use_id: impl AsRef<str>) {
        let res = self
            .content
            .iter_mut()
            .enumerate()
            .find_map(|(i, content_block)| match content_block {
                ContentBlock::ToolResult(block) if block.tool_use_id == tool_use_id.as_ref() => {
                    let mut tool_imgs = Vec::new();
                    let mut tool_strs = Vec::new();
                    for v in &block.content {
                        match v {
                            ToolResultContentBlock::Text(s) => tool_strs.push(s.clone()),
                            ToolResultContentBlock::Json(value) => tool_strs.push(
                                serde_json::to_string(value)
                                    .map_err(|err| error!(?err, "failed to serialize tool result"))
                                    .unwrap_or_default(),
                            ),
                            ToolResultContentBlock::Image(img) => {
                                tool_imgs.push(ContentBlock::Image(img.clone()));
                            },
                        }
                    }
                    Some((
                        i,
                        if tool_strs.is_empty() {
                            None
                        } else {
                            Some(tool_strs.join(" "))
                        },
                        if tool_imgs.is_empty() { None } else { Some(tool_imgs) },
                    ))
                },
                _ => None,
            });
        if let Some((i, text, imgs)) = res {
            if let Some(text) = text {
                self.content.push(ContentBlock::Text(text));
            }
            if let Some(mut imgs) = imgs {
                self.content.append(&mut imgs);
            }
            self.content.swap_remove(i);
        }
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

    /// Returns the approximate byte length of this message's content.
    pub fn byte_len(&self) -> usize {
        self.content.iter().map(|c| c.byte_len()).sum()
    }

    /// Truncates content in this message so total size is under max_length.
    /// Budget is distributed equally among truncatable items (excludes images).
    pub fn truncate(&mut self, max_length: usize, suffix: Option<&str>) {
        let total_len: usize = self.content.iter().map(|c| c.byte_len()).sum();
        if total_len <= max_length {
            return;
        }

        let truncatable_count: usize = self.content.iter().map(|c| c.truncatable_count()).sum();
        if truncatable_count == 0 {
            return;
        }

        let per_item_budget = max_length / truncatable_count;
        for c in &mut self.content {
            c.truncate(per_item_budget, suffix);
        }
    }
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "data")]
pub enum ContentBlock {
    Text(String),
    ToolUse(ToolUseBlock),
    ToolResult(ToolResultBlock),
    Image(ImageBlock),
}

impl ContentBlock {
    pub fn text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn tool_result(&self) -> Option<&ToolResultBlock> {
        match self {
            ContentBlock::ToolResult(block) => Some(block),
            _ => None,
        }
    }

    pub fn image(&self) -> Option<&ImageBlock> {
        match self {
            ContentBlock::Image(block) => Some(block),
            _ => None,
        }
    }

    pub fn byte_len(&self) -> usize {
        match self {
            ContentBlock::Text(t) => t.len(),
            ContentBlock::ToolUse(tu) => tu.input.to_string().len(),
            ContentBlock::ToolResult(tr) => tr
                .content
                .iter()
                .map(|c| match c {
                    ToolResultContentBlock::Text(t) => t.len(),
                    ToolResultContentBlock::Json(v) => v.to_string().len(),
                    ToolResultContentBlock::Image(img) => img.byte_len(),
                })
                .sum(),
            ContentBlock::Image(img) => img.byte_len(),
        }
    }

    pub fn truncatable_count(&self) -> usize {
        match self {
            ContentBlock::Text(_) => 1,
            ContentBlock::ToolUse(_) => 0,
            ContentBlock::ToolResult(tr) => tr
                .content
                .iter()
                .filter(|c| !matches!(c, ToolResultContentBlock::Image(_)))
                .count(),
            ContentBlock::Image(_) => 0,
        }
    }

    pub fn truncate(&mut self, max_length: usize, suffix: Option<&str>) {
        match self {
            ContentBlock::Text(t) => truncate_safe_in_place(t, max_length, suffix),
            ContentBlock::ToolUse(_) => (),
            ContentBlock::ToolResult(tr) => {
                for c in &mut tr.content {
                    match c {
                        ToolResultContentBlock::Text(t) => truncate_safe_in_place(t, max_length, suffix),
                        ToolResultContentBlock::Json(v) => {
                            let mut s = v.to_string();
                            truncate_safe_in_place(&mut s, max_length, suffix);
                            *c = ToolResultContentBlock::Text(s);
                        },
                        ToolResultContentBlock::Image(_) => (),
                    }
                }
            },
            ContentBlock::Image(_) => (),
        }
    }
}

impl From<String> for ContentBlock {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct ImageBlock {
    pub format: ImageFormat,
    pub source: ImageSource,
}

impl ImageBlock {
    pub fn byte_len(&self) -> usize {
        match &self.source {
            ImageSource::Bytes(bytes) => bytes.len(),
        }
    }
}

#[typeshare]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::EnumString, strum::Display, strum::EnumIter,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ImageFormat {
    Gif,
    #[serde(alias = "jpg")]
    #[strum(serialize = "jpeg", serialize = "jpg")]
    Jpeg,
    Png,
    Webp,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "data")]
pub enum ImageSource {
    Bytes(#[serde(with = "serde_bytes")] Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Map<String, serde_json::Value>,
}

#[typeshare]
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

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: Vec<ToolResultContentBlock>,
    pub status: ToolResultStatus,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "data")]
pub enum ToolResultContentBlock {
    Text(String),
    Json(serde_json::Value),
    Image(ImageBlock),
}

impl ToolResultContentBlock {
    pub fn text(&self) -> Option<&str> {
        match self {
            ToolResultContentBlock::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn json(&self) -> Option<&serde_json::Value> {
        match self {
            ToolResultContentBlock::Json(json) => Some(json),
            _ => None,
        }
    }
}

#[typeshare]
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolResultStatus {
    Error,
    Success,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageStartEvent {
    pub role: Role,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageStopEvent {
    pub stop_reason: StopReason,
}

#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum Role {
    User,
    Assistant,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum StopReason {
    ToolUse,
    EndTurn,
    MaxTokens,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockStartEvent {
    pub content_block_start: Option<ContentBlockStart>,
    /// Index of the content block within the message. This is optional to accommodate different
    /// model providers.
    pub content_block_index: Option<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum ContentBlockStart {
    ToolUse(ToolUseBlockStart),
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseBlockStart {
    /// Identifier for the tool use
    pub tool_use_id: String,
    /// Name of the tool
    pub name: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockDeltaEvent {
    pub delta: ContentBlockDelta,
    /// Index of the content block within the message. This is optional to accommodate different
    /// model providers.
    pub content_block_index: Option<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum ContentBlockDelta {
    Text(String),
    ToolUse(ToolUseBlockDelta),
    // todo?
    Reasoning,
    Document,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseBlockDelta {
    pub input: String,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockStopEvent {
    /// Index of the content block within the message. This is optional to accommodate different
    /// model providers.
    pub content_block_index: Option<i32>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataEvent {
    pub metrics: Option<MetadataMetrics>,
    pub usage: Option<MetadataUsage>,
    pub service: Option<MetadataService>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataMetrics {
    pub request_start_time: DateTime<Utc>,
    pub request_end_time: DateTime<Utc>,
    pub time_to_first_chunk: Option<Duration>,
    pub time_between_chunks: Option<Vec<Duration>>,
    pub response_stream_len: u32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
    pub cache_write_input_tokens: Option<u32>,
    pub context_usage_percentage: Option<f32>,
}

#[typeshare]
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
    fn test_image_format_ser_deser() {
        test_ser_deser!(ImageFormat, ImageFormat::Gif, "gif");
        test_ser_deser!(ImageFormat, ImageFormat::Png, "png");
        test_ser_deser!(ImageFormat, ImageFormat::Webp, "webp");
        test_ser_deser!(ImageFormat, ImageFormat::Jpeg, "jpeg");
        assert_eq!(
            ImageFormat::from_str("jpg").unwrap(),
            ImageFormat::Jpeg,
            "expected 'jpg' to parse to {}",
            ImageFormat::Jpeg
        );
    }

    #[test]
    fn test_message_byte_len() {
        let msg = Message::new(
            Role::User,
            vec![
                ContentBlock::Text("hello".to_string()),
                ContentBlock::Text("world".to_string()),
            ],
            None,
        );
        assert_eq!(msg.byte_len(), 10);
    }

    #[test]
    fn test_message_truncate_under_limit() {
        let mut msg = Message::new(Role::User, vec![ContentBlock::Text("hello".to_string())], None);
        msg.truncate(100, None);
        assert_eq!(msg.text(), "hello");
    }

    #[test]
    fn test_message_truncate_single_text() {
        let mut msg = Message::new(
            Role::User,
            vec![ContentBlock::Text("hello world this is a long message".to_string())],
            None,
        );
        msg.truncate(20, Some("..."));
        assert!(msg.byte_len() <= 20);
        assert!(msg.text().ends_with("..."));
    }

    #[test]
    fn test_message_truncate_multiple_text_blocks() {
        let mut msg = Message::new(
            Role::User,
            vec![
                ContentBlock::Text("aaaaaaaaaa".to_string()), // 10 bytes
                ContentBlock::Text("bbbbbbbbbb".to_string()), // 10 bytes
            ],
            None,
        );
        // Total 20 bytes, truncate to 16 -> 8 per item
        msg.truncate(16, Some(".."));
        assert!(msg.byte_len() <= 16);
    }

    #[test]
    fn test_message_truncate_with_tool_result() {
        let mut msg = Message::new(
            Role::User,
            vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: "test".to_string(),
                content: vec![
                    ToolResultContentBlock::Text("long text content here".to_string()),
                    ToolResultContentBlock::Json(serde_json::json!({"key": "value"})),
                ],
                status: ToolResultStatus::Success,
            })],
            None,
        );
        msg.truncate(20, Some(".."));
        assert!(msg.byte_len() <= 20);
    }

    #[test]
    fn test_message_truncate_skips_images() {
        let mut msg = Message::new(
            Role::User,
            vec![
                ContentBlock::Text("hello".to_string()),
                ContentBlock::Image(ImageBlock {
                    format: ImageFormat::Png,
                    source: ImageSource::Bytes(vec![0; 100]),
                }),
            ],
            None,
        );
        // Image has 100 bytes, text has 5. Only text is truncatable.
        // truncatable_count = 1, so budget = 10 / 1 = 10
        msg.truncate(10, None);
        // Text should remain unchanged since 5 <= 10
        assert_eq!(msg.content[0].text(), Some("hello"));
        // Image should be unchanged
        assert_eq!(msg.content[1].byte_len(), 100);
    }
}
