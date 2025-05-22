use std::collections::HashMap;

use aws_smithy_types::{
    Blob,
    Document as AwsDocument,
};
use serde::de::{
    self,
    MapAccess,
    SeqAccess,
    Visitor,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContext {
    pub left_file_content: String,
    pub right_file_content: String,
    pub filename: String,
    pub programming_language: ProgrammingLanguage,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgrammingLanguage {
    pub language_name: LanguageName,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, strum::AsRefStr)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum LanguageName {
    Python,
    Javascript,
    Java,
    Csharp,
    Typescript,
    C,
    Cpp,
    Go,
    Kotlin,
    Php,
    Ruby,
    Rust,
    Scala,
    Shell,
    Sql,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceTrackerConfiguration {
    pub recommendations_with_references: RecommendationsWithReferences,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RecommendationsWithReferences {
    Block,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationsInput {
    pub file_context: FileContext,
    pub max_results: i32,
    pub next_token: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationsOutput {
    pub recommendations: Vec<Recommendation>,
    pub next_token: Option<String>,
    pub session_id: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub content: String,
}

// =========
// Streaming
// =========

#[derive(Debug, Clone)]
pub struct ConversationState {
    pub conversation_id: Option<String>,
    pub user_input_message: UserInputMessage,
    pub history: Option<Vec<ChatMessage>>,
}

#[derive(Debug, Clone)]
pub enum ChatMessage {
    AssistantResponseMessage(AssistantResponseMessage),
    UserInputMessage(UserInputMessage),
}

impl TryFrom<ChatMessage> for amzn_codewhisperer_streaming_client::types::ChatMessage {
    type Error = aws_smithy_types::error::operation::BuildError;

    fn try_from(value: ChatMessage) -> Result<Self, Self::Error> {
        Ok(match value {
            ChatMessage::AssistantResponseMessage(message) => {
                amzn_codewhisperer_streaming_client::types::ChatMessage::AssistantResponseMessage(message.try_into()?)
            },
            ChatMessage::UserInputMessage(message) => {
                amzn_codewhisperer_streaming_client::types::ChatMessage::UserInputMessage(message.into())
            },
        })
    }
}

impl TryFrom<ChatMessage> for amzn_qdeveloper_streaming_client::types::ChatMessage {
    type Error = aws_smithy_types::error::operation::BuildError;

    fn try_from(value: ChatMessage) -> Result<Self, Self::Error> {
        Ok(match value {
            ChatMessage::AssistantResponseMessage(message) => {
                amzn_qdeveloper_streaming_client::types::ChatMessage::AssistantResponseMessage(message.try_into()?)
            },
            ChatMessage::UserInputMessage(message) => {
                amzn_qdeveloper_streaming_client::types::ChatMessage::UserInputMessage(message.into())
            },
        })
    }
}

/// Wrapper around [aws_smithy_types::Document].
///
/// Used primarily so we can implement [Serialize] and [Deserialize] for
/// [aws_smith_types::Document].
#[derive(Debug, Clone)]
pub struct FigDocument(AwsDocument);

impl From<AwsDocument> for FigDocument {
    fn from(value: AwsDocument) -> Self {
        Self(value)
    }
}

impl From<FigDocument> for AwsDocument {
    fn from(value: FigDocument) -> Self {
        value.0
    }
}

/// Internal type used only during serialization for `FigDocument` to avoid unnecessary cloning.
#[derive(Debug, Clone)]
struct FigDocumentRef<'a>(&'a AwsDocument);

impl Serialize for FigDocumentRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use aws_smithy_types::Number;
        match self.0 {
            AwsDocument::Null => serializer.serialize_unit(),
            AwsDocument::Bool(b) => serializer.serialize_bool(*b),
            AwsDocument::Number(n) => match n {
                Number::PosInt(u) => serializer.serialize_u64(*u),
                Number::NegInt(i) => serializer.serialize_i64(*i),
                Number::Float(f) => serializer.serialize_f64(*f),
            },
            AwsDocument::String(s) => serializer.serialize_str(s),
            AwsDocument::Array(arr) => {
                use serde::ser::SerializeSeq;
                let mut seq = serializer.serialize_seq(Some(arr.len()))?;
                for value in arr {
                    seq.serialize_element(&Self(value))?;
                }
                seq.end()
            },
            AwsDocument::Object(m) => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(m.len()))?;
                for (k, v) in m {
                    map.serialize_entry(k, &Self(v))?;
                }
                map.end()
            },
        }
    }
}

impl Serialize for FigDocument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        FigDocumentRef(&self.0).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FigDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use aws_smithy_types::Number;

        struct FigDocumentVisitor;

        impl<'de> Visitor<'de> for FigDocumentVisitor {
            type Value = FigDocument;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("any valid JSON value")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FigDocument(AwsDocument::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FigDocument(AwsDocument::Number(if value < 0 {
                    Number::NegInt(value)
                } else {
                    Number::PosInt(value as u64)
                })))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FigDocument(AwsDocument::Number(Number::PosInt(value))))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FigDocument(AwsDocument::Number(Number::Float(value))))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FigDocument(AwsDocument::String(value.to_owned())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FigDocument(AwsDocument::String(value)))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FigDocument(AwsDocument::Null))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(FigDocument(AwsDocument::Null))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut vec = Vec::new();

                while let Some(elem) = seq.next_element::<FigDocument>()? {
                    vec.push(elem.0);
                }

                Ok(FigDocument(AwsDocument::Array(vec)))
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut map = HashMap::new();

                while let Some((key, value)) = access.next_entry::<String, FigDocument>()? {
                    map.insert(key, value.0);
                }

                Ok(FigDocument(AwsDocument::Object(map)))
            }
        }

        deserializer.deserialize_any(FigDocumentVisitor)
    }
}

/// Information about a tool that can be used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tool {
    ToolSpecification(ToolSpecification),
}

impl From<Tool> for amzn_codewhisperer_streaming_client::types::Tool {
    fn from(value: Tool) -> Self {
        match value {
            Tool::ToolSpecification(v) => amzn_codewhisperer_streaming_client::types::Tool::ToolSpecification(v.into()),
        }
    }
}

impl From<Tool> for amzn_qdeveloper_streaming_client::types::Tool {
    fn from(value: Tool) -> Self {
        match value {
            Tool::ToolSpecification(v) => amzn_qdeveloper_streaming_client::types::Tool::ToolSpecification(v.into()),
        }
    }
}

/// The specification for the tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpecification {
    /// The name for the tool.
    pub name: String,
    /// The description for the tool.
    pub description: String,
    /// The input schema for the tool in JSON format.
    pub input_schema: ToolInputSchema,
}

impl From<ToolSpecification> for amzn_codewhisperer_streaming_client::types::ToolSpecification {
    fn from(value: ToolSpecification) -> Self {
        Self::builder()
            .name(value.name)
            .description(value.description)
            .input_schema(value.input_schema.into())
            .build()
            .expect("building ToolSpecification should not fail")
    }
}

impl From<ToolSpecification> for amzn_qdeveloper_streaming_client::types::ToolSpecification {
    fn from(value: ToolSpecification) -> Self {
        Self::builder()
            .name(value.name)
            .description(value.description)
            .input_schema(value.input_schema.into())
            .build()
            .expect("building ToolSpecification should not fail")
    }
}

/// The input schema for the tool in JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputSchema {
    pub json: Option<FigDocument>,
}

impl From<ToolInputSchema> for amzn_codewhisperer_streaming_client::types::ToolInputSchema {
    fn from(value: ToolInputSchema) -> Self {
        Self::builder().set_json(value.json.map(Into::into)).build()
    }
}

impl From<ToolInputSchema> for amzn_qdeveloper_streaming_client::types::ToolInputSchema {
    fn from(value: ToolInputSchema) -> Self {
        Self::builder().set_json(value.json.map(Into::into)).build()
    }
}

/// Contains information about a tool that the model is requesting be run. The model uses the result
/// from the tool to generate a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUse {
    /// The ID for the tool request.
    pub tool_use_id: String,
    /// The name for the tool.
    pub name: String,
    /// The input to pass to the tool.
    pub input: FigDocument,
}

impl From<ToolUse> for amzn_codewhisperer_streaming_client::types::ToolUse {
    fn from(value: ToolUse) -> Self {
        Self::builder()
            .tool_use_id(value.tool_use_id)
            .name(value.name)
            .input(value.input.into())
            .build()
            .expect("building ToolUse should not fail")
    }
}

impl From<ToolUse> for amzn_qdeveloper_streaming_client::types::ToolUse {
    fn from(value: ToolUse) -> Self {
        Self::builder()
            .tool_use_id(value.tool_use_id)
            .name(value.name)
            .input(value.input.into())
            .build()
            .expect("building ToolUse should not fail")
    }
}

/// A tool result that contains the results for a tool request that was previously made.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// The ID for the tool request.
    pub tool_use_id: String,
    /// Content of the tool result.
    pub content: Vec<ToolResultContentBlock>,
    /// Status of the tools result.
    pub status: ToolResultStatus,
}

impl From<ToolResult> for amzn_codewhisperer_streaming_client::types::ToolResult {
    fn from(value: ToolResult) -> Self {
        Self::builder()
            .tool_use_id(value.tool_use_id)
            .set_content(Some(value.content.into_iter().map(Into::into).collect::<_>()))
            .status(value.status.into())
            .build()
            .expect("building ToolResult should not fail")
    }
}

impl From<ToolResult> for amzn_qdeveloper_streaming_client::types::ToolResult {
    fn from(value: ToolResult) -> Self {
        Self::builder()
            .tool_use_id(value.tool_use_id)
            .set_content(Some(value.content.into_iter().map(Into::into).collect::<_>()))
            .status(value.status.into())
            .build()
            .expect("building ToolResult should not fail")
    }
}

#[derive(Debug, Clone)]
pub enum ToolResultContentBlock {
    /// A tool result that is JSON format data.
    Json(AwsDocument),
    /// A tool result that is text.
    Text(String),
}

impl From<ToolResultContentBlock> for amzn_codewhisperer_streaming_client::types::ToolResultContentBlock {
    fn from(value: ToolResultContentBlock) -> Self {
        match value {
            ToolResultContentBlock::Json(document) => Self::Json(document),
            ToolResultContentBlock::Text(text) => Self::Text(text),
        }
    }
}

impl From<ToolResultContentBlock> for amzn_qdeveloper_streaming_client::types::ToolResultContentBlock {
    fn from(value: ToolResultContentBlock) -> Self {
        match value {
            ToolResultContentBlock::Json(document) => Self::Json(document),
            ToolResultContentBlock::Text(text) => Self::Text(text),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolResultStatus {
    Error,
    Success,
}

impl From<ToolResultStatus> for amzn_codewhisperer_streaming_client::types::ToolResultStatus {
    fn from(value: ToolResultStatus) -> Self {
        match value {
            ToolResultStatus::Error => Self::Error,
            ToolResultStatus::Success => Self::Success,
        }
    }
}

impl From<ToolResultStatus> for amzn_qdeveloper_streaming_client::types::ToolResultStatus {
    fn from(value: ToolResultStatus) -> Self {
        match value {
            ToolResultStatus::Error => Self::Error,
            ToolResultStatus::Success => Self::Success,
        }
    }
}

/// Markdown text message.
#[derive(Debug, Clone)]
pub struct AssistantResponseMessage {
    /// Unique identifier for the chat message
    pub message_id: Option<String>,
    /// The content of the text message in markdown format.
    pub content: String,
    /// ToolUse Request
    pub tool_uses: Option<Vec<ToolUse>>,
}

impl TryFrom<AssistantResponseMessage> for amzn_codewhisperer_streaming_client::types::AssistantResponseMessage {
    type Error = aws_smithy_types::error::operation::BuildError;

    fn try_from(value: AssistantResponseMessage) -> Result<Self, Self::Error> {
        Self::builder()
            .content(value.content)
            .set_message_id(value.message_id)
            .set_tool_uses(value.tool_uses.map(|uses| uses.into_iter().map(Into::into).collect()))
            .build()
    }
}

impl TryFrom<AssistantResponseMessage> for amzn_qdeveloper_streaming_client::types::AssistantResponseMessage {
    type Error = aws_smithy_types::error::operation::BuildError;

    fn try_from(value: AssistantResponseMessage) -> Result<Self, Self::Error> {
        Self::builder()
            .content(value.content)
            .set_message_id(value.message_id)
            .set_tool_uses(value.tool_uses.map(|uses| uses.into_iter().map(Into::into).collect()))
            .build()
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatResponseStream {
    AssistantResponseEvent {
        content: String,
    },
    /// Streaming response event for generated code text.
    CodeEvent {
        content: String,
    },
    // TODO: finish events here
    CodeReferenceEvent(()),
    FollowupPromptEvent(()),
    IntentsEvent(()),
    InvalidStateEvent {
        reason: String,
        message: String,
    },
    MessageMetadataEvent {
        conversation_id: Option<String>,
        utterance_id: Option<String>,
    },
    SupplementaryWebLinksEvent(()),
    ToolUseEvent {
        tool_use_id: String,
        name: String,
        input: Option<String>,
        stop: Option<bool>,
    },

    #[non_exhaustive]
    Unknown,
}

impl From<amzn_codewhisperer_streaming_client::types::ChatResponseStream> for ChatResponseStream {
    fn from(value: amzn_codewhisperer_streaming_client::types::ChatResponseStream) -> Self {
        match value {
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::AssistantResponseEvent(
                amzn_codewhisperer_streaming_client::types::AssistantResponseEvent { content, .. },
            ) => ChatResponseStream::AssistantResponseEvent { content },
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::CodeEvent(
                amzn_codewhisperer_streaming_client::types::CodeEvent { content, .. },
            ) => ChatResponseStream::CodeEvent { content },
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::CodeReferenceEvent(_) => {
                ChatResponseStream::CodeReferenceEvent(())
            },
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::FollowupPromptEvent(_) => {
                ChatResponseStream::FollowupPromptEvent(())
            },
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::IntentsEvent(_) => {
                ChatResponseStream::IntentsEvent(())
            },
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::InvalidStateEvent(
                amzn_codewhisperer_streaming_client::types::InvalidStateEvent { reason, message, .. },
            ) => ChatResponseStream::InvalidStateEvent {
                reason: reason.to_string(),
                message,
            },
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::MessageMetadataEvent(
                amzn_codewhisperer_streaming_client::types::MessageMetadataEvent {
                    conversation_id,
                    utterance_id,
                    ..
                },
            ) => ChatResponseStream::MessageMetadataEvent {
                conversation_id,
                utterance_id,
            },
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::ToolUseEvent(
                amzn_codewhisperer_streaming_client::types::ToolUseEvent {
                    tool_use_id,
                    name,
                    input,
                    stop,
                    ..
                },
            ) => ChatResponseStream::ToolUseEvent {
                tool_use_id,
                name,
                input,
                stop,
            },
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::SupplementaryWebLinksEvent(_) => {
                ChatResponseStream::SupplementaryWebLinksEvent(())
            },
            _ => ChatResponseStream::Unknown,
        }
    }
}

impl From<amzn_qdeveloper_streaming_client::types::ChatResponseStream> for ChatResponseStream {
    fn from(value: amzn_qdeveloper_streaming_client::types::ChatResponseStream) -> Self {
        match value {
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::AssistantResponseEvent(
                amzn_qdeveloper_streaming_client::types::AssistantResponseEvent { content, .. },
            ) => ChatResponseStream::AssistantResponseEvent { content },
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::CodeEvent(
                amzn_qdeveloper_streaming_client::types::CodeEvent { content, .. },
            ) => ChatResponseStream::CodeEvent { content },
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::CodeReferenceEvent(_) => {
                ChatResponseStream::CodeReferenceEvent(())
            },
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::FollowupPromptEvent(_) => {
                ChatResponseStream::FollowupPromptEvent(())
            },
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::IntentsEvent(_) => {
                ChatResponseStream::IntentsEvent(())
            },
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::InvalidStateEvent(
                amzn_qdeveloper_streaming_client::types::InvalidStateEvent { reason, message, .. },
            ) => ChatResponseStream::InvalidStateEvent {
                reason: reason.to_string(),
                message,
            },
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::MessageMetadataEvent(
                amzn_qdeveloper_streaming_client::types::MessageMetadataEvent {
                    conversation_id,
                    utterance_id,
                    ..
                },
            ) => ChatResponseStream::MessageMetadataEvent {
                conversation_id,
                utterance_id,
            },
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::ToolUseEvent(
                amzn_qdeveloper_streaming_client::types::ToolUseEvent {
                    tool_use_id,
                    name,
                    input,
                    stop,
                    ..
                },
            ) => ChatResponseStream::ToolUseEvent {
                tool_use_id,
                name,
                input,
                stop,
            },
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::SupplementaryWebLinksEvent(_) => {
                ChatResponseStream::SupplementaryWebLinksEvent(())
            },
            _ => ChatResponseStream::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvState {
    pub operating_system: Option<String>,
    pub current_working_directory: Option<String>,
    pub environment_variables: Vec<EnvironmentVariable>,
}

impl From<EnvState> for amzn_codewhisperer_streaming_client::types::EnvState {
    fn from(value: EnvState) -> Self {
        let environment_variables: Vec<_> = value.environment_variables.into_iter().map(Into::into).collect();
        Self::builder()
            .set_operating_system(value.operating_system)
            .set_current_working_directory(value.current_working_directory)
            .set_environment_variables(if environment_variables.is_empty() {
                None
            } else {
                Some(environment_variables)
            })
            .build()
    }
}

impl From<EnvState> for amzn_qdeveloper_streaming_client::types::EnvState {
    fn from(value: EnvState) -> Self {
        let environment_variables: Vec<_> = value.environment_variables.into_iter().map(Into::into).collect();
        Self::builder()
            .set_operating_system(value.operating_system)
            .set_current_working_directory(value.current_working_directory)
            .set_environment_variables(if environment_variables.is_empty() {
                None
            } else {
                Some(environment_variables)
            })
            .build()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub key: String,
    pub value: String,
}

impl From<EnvironmentVariable> for amzn_codewhisperer_streaming_client::types::EnvironmentVariable {
    fn from(value: EnvironmentVariable) -> Self {
        Self::builder().key(value.key).value(value.value).build()
    }
}

impl From<EnvironmentVariable> for amzn_qdeveloper_streaming_client::types::EnvironmentVariable {
    fn from(value: EnvironmentVariable) -> Self {
        Self::builder().key(value.key).value(value.value).build()
    }
}

#[derive(Debug, Clone)]
pub struct GitState {
    pub status: String,
}

impl From<GitState> for amzn_codewhisperer_streaming_client::types::GitState {
    fn from(value: GitState) -> Self {
        Self::builder().status(value.status).build()
    }
}

impl From<GitState> for amzn_qdeveloper_streaming_client::types::GitState {
    fn from(value: GitState) -> Self {
        Self::builder().status(value.status).build()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBlock {
    pub format: ImageFormat,
    pub source: ImageSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageFormat {
    Gif,
    Jpeg,
    Png,
    Webp,
}

impl std::str::FromStr for ImageFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "gif" => Ok(ImageFormat::Gif),
            "jpeg" => Ok(ImageFormat::Jpeg),
            "jpg" => Ok(ImageFormat::Jpeg),
            "png" => Ok(ImageFormat::Png),
            "webp" => Ok(ImageFormat::Webp),
            _ => Err(format!("Failed to parse '{}' as ImageFormat", s)),
        }
    }
}

impl From<ImageFormat> for amzn_codewhisperer_streaming_client::types::ImageFormat {
    fn from(value: ImageFormat) -> Self {
        match value {
            ImageFormat::Gif => Self::Gif,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Png => Self::Png,
            ImageFormat::Webp => Self::Webp,
        }
    }
}
impl From<ImageFormat> for amzn_qdeveloper_streaming_client::types::ImageFormat {
    fn from(value: ImageFormat) -> Self {
        match value {
            ImageFormat::Gif => Self::Gif,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Png => Self::Png,
            ImageFormat::Webp => Self::Webp,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSource {
    Bytes(Vec<u8>),
    #[non_exhaustive]
    Unknown,
}

impl From<ImageSource> for amzn_codewhisperer_streaming_client::types::ImageSource {
    fn from(value: ImageSource) -> Self {
        match value {
            ImageSource::Bytes(bytes) => Self::Bytes(Blob::new(bytes)),
            ImageSource::Unknown => Self::Unknown,
        }
    }
}
impl From<ImageSource> for amzn_qdeveloper_streaming_client::types::ImageSource {
    fn from(value: ImageSource) -> Self {
        match value {
            ImageSource::Bytes(bytes) => Self::Bytes(Blob::new(bytes)),
            ImageSource::Unknown => Self::Unknown,
        }
    }
}

impl From<ImageBlock> for amzn_codewhisperer_streaming_client::types::ImageBlock {
    fn from(value: ImageBlock) -> Self {
        Self::builder()
            .format(value.format.into())
            .source(value.source.into())
            .build()
            .expect("Failed to build ImageBlock")
    }
}
impl From<ImageBlock> for amzn_qdeveloper_streaming_client::types::ImageBlock {
    fn from(value: ImageBlock) -> Self {
        Self::builder()
            .format(value.format.into())
            .source(value.source.into())
            .build()
            .expect("Failed to build ImageBlock")
    }
}

#[derive(Debug, Clone)]
pub struct UserInputMessage {
    pub content: String,
    pub user_input_message_context: Option<UserInputMessageContext>,
    pub user_intent: Option<UserIntent>,
    pub images: Option<Vec<ImageBlock>>,
}

impl From<UserInputMessage> for amzn_codewhisperer_streaming_client::types::UserInputMessage {
    fn from(value: UserInputMessage) -> Self {
        Self::builder()
            .content(value.content)
            .set_images(value.images.map(|images| images.into_iter().map(Into::into).collect()))
            .set_user_input_message_context(value.user_input_message_context.map(Into::into))
            .set_user_intent(value.user_intent.map(Into::into))
            .origin(amzn_codewhisperer_streaming_client::types::Origin::Cli)
            .build()
            .expect("Failed to build UserInputMessage")
    }
}

impl From<UserInputMessage> for amzn_qdeveloper_streaming_client::types::UserInputMessage {
    fn from(value: UserInputMessage) -> Self {
        Self::builder()
            .content(value.content)
            .set_images(value.images.map(|images| images.into_iter().map(Into::into).collect()))
            .set_user_input_message_context(value.user_input_message_context.map(Into::into))
            .set_user_intent(value.user_intent.map(Into::into))
            .origin(amzn_qdeveloper_streaming_client::types::Origin::Cli)
            .build()
            .expect("Failed to build UserInputMessage")
    }
}

#[derive(Debug, Clone, Default)]
pub struct UserInputMessageContext {
    pub env_state: Option<EnvState>,
    pub git_state: Option<GitState>,
    pub tool_results: Option<Vec<ToolResult>>,
    pub tools: Option<Vec<Tool>>,
}

impl From<UserInputMessageContext> for amzn_codewhisperer_streaming_client::types::UserInputMessageContext {
    fn from(value: UserInputMessageContext) -> Self {
        Self::builder()
            .set_env_state(value.env_state.map(Into::into))
            .set_git_state(value.git_state.map(Into::into))
            .set_tool_results(value.tool_results.map(|t| t.into_iter().map(Into::into).collect()))
            .set_tools(value.tools.map(|t| t.into_iter().map(Into::into).collect()))
            .build()
    }
}

impl From<UserInputMessageContext> for amzn_qdeveloper_streaming_client::types::UserInputMessageContext {
    fn from(value: UserInputMessageContext) -> Self {
        Self::builder()
            .set_env_state(value.env_state.map(Into::into))
            .set_git_state(value.git_state.map(Into::into))
            .set_tool_results(value.tool_results.map(|t| t.into_iter().map(Into::into).collect()))
            .set_tools(value.tools.map(|t| t.into_iter().map(Into::into).collect()))
            .build()
    }
}

#[derive(Debug, Clone)]
pub enum UserIntent {
    ApplyCommonBestPractices,
}

impl From<UserIntent> for amzn_codewhisperer_streaming_client::types::UserIntent {
    fn from(value: UserIntent) -> Self {
        match value {
            UserIntent::ApplyCommonBestPractices => Self::ApplyCommonBestPractices,
        }
    }
}

impl From<UserIntent> for amzn_qdeveloper_streaming_client::types::UserIntent {
    fn from(value: UserIntent) -> Self {
        match value {
            UserIntent::ApplyCommonBestPractices => Self::ApplyCommonBestPractices,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_user_input_message() {
        let user_input_message = UserInputMessage {
            images: Some(vec![ImageBlock {
                format: ImageFormat::Png,
                source: ImageSource::Bytes(vec![1, 2, 3]),
            }]),
            content: "test content".to_string(),
            user_input_message_context: Some(UserInputMessageContext {
                env_state: Some(EnvState {
                    operating_system: Some("test os".to_string()),
                    current_working_directory: Some("test cwd".to_string()),
                    environment_variables: vec![EnvironmentVariable {
                        key: "test key".to_string(),
                        value: "test value".to_string(),
                    }],
                }),
                git_state: Some(GitState {
                    status: "test status".to_string(),
                }),
                tool_results: Some(vec![ToolResult {
                    tool_use_id: "test id".to_string(),
                    content: vec![ToolResultContentBlock::Text("test text".to_string())],
                    status: ToolResultStatus::Success,
                }]),
                tools: Some(vec![Tool::ToolSpecification(ToolSpecification {
                    name: "test tool name".to_string(),
                    description: "test tool description".to_string(),
                    input_schema: ToolInputSchema {
                        json: Some(AwsDocument::Null.into()),
                    },
                })]),
            }),
            user_intent: Some(UserIntent::ApplyCommonBestPractices),
        };

        let codewhisper_input =
            amzn_codewhisperer_streaming_client::types::UserInputMessage::from(user_input_message.clone());
        let qdeveloper_input = amzn_qdeveloper_streaming_client::types::UserInputMessage::from(user_input_message);

        assert_eq!(format!("{codewhisper_input:?}"), format!("{qdeveloper_input:?}"));

        let minimal_message = UserInputMessage {
            images: None,
            content: "test content".to_string(),
            user_input_message_context: None,
            user_intent: None,
        };

        let codewhisper_minimal =
            amzn_codewhisperer_streaming_client::types::UserInputMessage::from(minimal_message.clone());
        let qdeveloper_minimal = amzn_qdeveloper_streaming_client::types::UserInputMessage::from(minimal_message);
        assert_eq!(format!("{codewhisper_minimal:?}"), format!("{qdeveloper_minimal:?}"));
    }

    #[test]
    fn build_assistant_response_message() {
        let message = AssistantResponseMessage {
            message_id: Some("testid".to_string()),
            content: "test content".to_string(),
            tool_uses: Some(vec![ToolUse {
                tool_use_id: "tooluseid_test".to_string(),
                name: "tool_name_test".to_string(),
                input: FigDocument(AwsDocument::Object(
                    [("key1".to_string(), AwsDocument::Null)].into_iter().collect(),
                )),
            }]),
        };
        let codewhisper_input =
            amzn_codewhisperer_streaming_client::types::AssistantResponseMessage::try_from(message.clone()).unwrap();
        let qdeveloper_input =
            amzn_qdeveloper_streaming_client::types::AssistantResponseMessage::try_from(message).unwrap();
        assert_eq!(format!("{codewhisper_input:?}"), format!("{qdeveloper_input:?}"));
    }

    #[test]
    fn build_chat_response() {
        let assistant_response_event =
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::AssistantResponseEvent(
                amzn_codewhisperer_streaming_client::types::AssistantResponseEvent::builder()
                    .content("context")
                    .build()
                    .unwrap(),
            );
        assert_eq!(
            ChatResponseStream::from(assistant_response_event),
            ChatResponseStream::AssistantResponseEvent {
                content: "context".into(),
            }
        );

        let assistant_response_event =
            amzn_qdeveloper_streaming_client::types::ChatResponseStream::AssistantResponseEvent(
                amzn_qdeveloper_streaming_client::types::AssistantResponseEvent::builder()
                    .content("context")
                    .build()
                    .unwrap(),
            );
        assert_eq!(
            ChatResponseStream::from(assistant_response_event),
            ChatResponseStream::AssistantResponseEvent {
                content: "context".into(),
            }
        );

        let code_event = amzn_codewhisperer_streaming_client::types::ChatResponseStream::CodeEvent(
            amzn_codewhisperer_streaming_client::types::CodeEvent::builder()
                .content("context")
                .build()
                .unwrap(),
        );
        assert_eq!(ChatResponseStream::from(code_event), ChatResponseStream::CodeEvent {
            content: "context".into()
        });

        let code_event = amzn_qdeveloper_streaming_client::types::ChatResponseStream::CodeEvent(
            amzn_qdeveloper_streaming_client::types::CodeEvent::builder()
                .content("context")
                .build()
                .unwrap(),
        );
        assert_eq!(ChatResponseStream::from(code_event), ChatResponseStream::CodeEvent {
            content: "context".into()
        });

        let code_reference_event = amzn_codewhisperer_streaming_client::types::ChatResponseStream::CodeReferenceEvent(
            amzn_codewhisperer_streaming_client::types::CodeReferenceEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(code_reference_event),
            ChatResponseStream::CodeReferenceEvent(())
        );

        let code_reference_event = amzn_qdeveloper_streaming_client::types::ChatResponseStream::CodeReferenceEvent(
            amzn_qdeveloper_streaming_client::types::CodeReferenceEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(code_reference_event),
            ChatResponseStream::CodeReferenceEvent(())
        );

        let followup_prompt_event = amzn_codewhisperer_streaming_client::types::ChatResponseStream::FollowupPromptEvent(
            amzn_codewhisperer_streaming_client::types::FollowupPromptEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(followup_prompt_event),
            ChatResponseStream::FollowupPromptEvent(())
        );

        let followup_prompt_event = amzn_qdeveloper_streaming_client::types::ChatResponseStream::FollowupPromptEvent(
            amzn_qdeveloper_streaming_client::types::FollowupPromptEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(followup_prompt_event),
            ChatResponseStream::FollowupPromptEvent(())
        );

        let intents_event = amzn_codewhisperer_streaming_client::types::ChatResponseStream::IntentsEvent(
            amzn_codewhisperer_streaming_client::types::IntentsEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(intents_event),
            ChatResponseStream::IntentsEvent(())
        );

        let intents_event = amzn_qdeveloper_streaming_client::types::ChatResponseStream::IntentsEvent(
            amzn_qdeveloper_streaming_client::types::IntentsEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(intents_event),
            ChatResponseStream::IntentsEvent(())
        );

        let user_input_event = amzn_codewhisperer_streaming_client::types::ChatResponseStream::InvalidStateEvent(
            amzn_codewhisperer_streaming_client::types::InvalidStateEvent::builder()
                .reason(amzn_codewhisperer_streaming_client::types::InvalidStateReason::InvalidTaskAssistPlan)
                .message("message")
                .build()
                .unwrap(),
        );
        assert_eq!(
            ChatResponseStream::from(user_input_event),
            ChatResponseStream::InvalidStateEvent {
                reason: amzn_codewhisperer_streaming_client::types::InvalidStateReason::InvalidTaskAssistPlan
                    .to_string(),
                message: "message".into()
            }
        );

        let user_input_event = amzn_qdeveloper_streaming_client::types::ChatResponseStream::InvalidStateEvent(
            amzn_qdeveloper_streaming_client::types::InvalidStateEvent::builder()
                .reason(amzn_qdeveloper_streaming_client::types::InvalidStateReason::InvalidTaskAssistPlan)
                .message("message")
                .build()
                .unwrap(),
        );
        assert_eq!(
            ChatResponseStream::from(user_input_event),
            ChatResponseStream::InvalidStateEvent {
                reason: amzn_qdeveloper_streaming_client::types::InvalidStateReason::InvalidTaskAssistPlan.to_string(),
                message: "message".into()
            }
        );

        let user_input_event = amzn_codewhisperer_streaming_client::types::ChatResponseStream::MessageMetadataEvent(
            amzn_codewhisperer_streaming_client::types::MessageMetadataEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(user_input_event),
            ChatResponseStream::MessageMetadataEvent {
                conversation_id: None,
                utterance_id: None
            }
        );

        let user_input_event = amzn_qdeveloper_streaming_client::types::ChatResponseStream::MessageMetadataEvent(
            amzn_qdeveloper_streaming_client::types::MessageMetadataEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(user_input_event),
            ChatResponseStream::MessageMetadataEvent {
                conversation_id: None,
                utterance_id: None
            }
        );

        let user_input_event =
            amzn_codewhisperer_streaming_client::types::ChatResponseStream::SupplementaryWebLinksEvent(
                amzn_codewhisperer_streaming_client::types::SupplementaryWebLinksEvent::builder().build(),
            );
        assert_eq!(
            ChatResponseStream::from(user_input_event),
            ChatResponseStream::SupplementaryWebLinksEvent(())
        );

        let user_input_event = amzn_qdeveloper_streaming_client::types::ChatResponseStream::SupplementaryWebLinksEvent(
            amzn_qdeveloper_streaming_client::types::SupplementaryWebLinksEvent::builder().build(),
        );
        assert_eq!(
            ChatResponseStream::from(user_input_event),
            ChatResponseStream::SupplementaryWebLinksEvent(())
        );

        let user_input_event = amzn_codewhisperer_streaming_client::types::ChatResponseStream::ToolUseEvent(
            amzn_codewhisperer_streaming_client::types::ToolUseEvent::builder()
                .tool_use_id("tool_use_id".to_string())
                .name("tool_name".to_string())
                .build()
                .unwrap(),
        );
        assert_eq!(
            ChatResponseStream::from(user_input_event),
            ChatResponseStream::ToolUseEvent {
                tool_use_id: "tool_use_id".to_string(),
                name: "tool_name".to_string(),
                input: None,
                stop: None,
            }
        );

        let user_input_event = amzn_qdeveloper_streaming_client::types::ChatResponseStream::ToolUseEvent(
            amzn_qdeveloper_streaming_client::types::ToolUseEvent::builder()
                .tool_use_id("tool_use_id".to_string())
                .name("tool_name".to_string())
                .build()
                .unwrap(),
        );
        assert_eq!(
            ChatResponseStream::from(user_input_event),
            ChatResponseStream::ToolUseEvent {
                tool_use_id: "tool_use_id".to_string(),
                name: "tool_name".to_string(),
                input: None,
                stop: None,
            }
        );
    }
}
