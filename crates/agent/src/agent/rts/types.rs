use super::util::serde_value_to_document;
use crate::agent::agent_loop::types::*;
use crate::api_client::model;

impl From<ImageBlock> for model::ImageBlock {
    fn from(v: ImageBlock) -> Self {
        Self {
            format: v.format.into(),
            source: v.source.into(),
        }
    }
}

impl From<ImageFormat> for model::ImageFormat {
    fn from(value: ImageFormat) -> Self {
        match value {
            ImageFormat::Gif => Self::Gif,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Png => Self::Png,
            ImageFormat::Webp => Self::Webp,
        }
    }
}

impl From<ImageSource> for model::ImageSource {
    fn from(value: ImageSource) -> Self {
        match value {
            ImageSource::Bytes(items) => Self::Bytes(items),
        }
    }
}

impl From<ToolUseBlock> for model::ToolUse {
    fn from(v: ToolUseBlock) -> Self {
        Self {
            tool_use_id: v.tool_use_id,
            name: v.name,
            input: serde_value_to_document(v.input).into(),
        }
    }
}

// impl From<ToolResultBlock> for model::ToolResult {
//     fn from(v: ToolResultBlock) -> Self {
//         Self {
//             tool_use_id: v.tool_use_id,
//             content: v.content.into_iter().map(Into::into).collect(),
//             status: v.status.into(),
//         }
//     }
// }

// impl From<ToolResultContentBlock> for model::ToolResultContentBlock {
//     fn from(v: ToolResultContentBlock) -> Self {
//         match v {
//             ToolResultContentBlock::Text(t) => Self::Text(t),
//             ToolResultContentBlock::Json(v) => Self::Json(serde_value_to_document(v)),
//         }
//     }
// }

impl From<ToolResultStatus> for model::ToolResultStatus {
    fn from(value: ToolResultStatus) -> Self {
        match value {
            ToolResultStatus::Error => Self::Error,
            ToolResultStatus::Success => Self::Success,
        }
    }
}

impl From<ToolSpec> for model::ToolSpecification {
    fn from(v: ToolSpec) -> Self {
        Self {
            name: v.name,
            description: v.description,
            input_schema: v.input_schema.into(),
        }
    }
}

impl From<serde_json::Map<String, serde_json::Value>> for model::ToolInputSchema {
    fn from(v: serde_json::Map<String, serde_json::Value>) -> Self {
        Self {
            json: Some(serde_value_to_document(v.into()).into()),
        }
    }
}
