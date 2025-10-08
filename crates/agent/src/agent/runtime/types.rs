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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct ImageBlock {
    pub format: ImageFormat,
    pub source: ImageSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ImageFormat {
    Gif,
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
