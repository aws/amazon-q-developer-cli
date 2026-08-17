use std::sync::Arc;

use agent::agent_loop::types::*;

use crate::api_client::error::{
    ConverseStreamError,
    ConverseStreamErrorKind,
};
use crate::api_client::model;
use crate::cli::chat::legacy::util::serde_value_to_document;
use crate::telemetry::ReasonCode;

impl From<ConverseStreamError> for StreamError {
    fn from(err: ConverseStreamError) -> Self {
        let kind = match &err.kind {
            ConverseStreamErrorKind::Throttling => StreamErrorKind::Throttling,
            ConverseStreamErrorKind::MonthlyLimitReached => StreamErrorKind::MonthlyLimitReached {
                message: err.to_string(),
            },
            ConverseStreamErrorKind::ContextWindowOverflow => StreamErrorKind::ContextWindowOverflow,
            ConverseStreamErrorKind::ModelOverloadedError => StreamErrorKind::ModelOverloaded {
                message: err.to_string(),
            },
            ConverseStreamErrorKind::AccessDenied { message } => StreamErrorKind::AccessDenied {
                message: message.clone(),
            },
            ConverseStreamErrorKind::InvalidModelId { model_id } => StreamErrorKind::InvalidModelId {
                model_id: model_id.clone(),
            },
            ConverseStreamErrorKind::Unknown { .. } => StreamErrorKind::Other {
                reason_code: Some(err.reason_code()),
                message: err.to_string(),
            },
        };

        let request_id = err.request_id.clone();
        StreamError::new(kind)
            .set_original_request_id(request_id)
            .set_original_status_code(err.status_code)
            .with_source(Arc::new(err))
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn stream_error(kind: ConverseStreamErrorKind) -> StreamError {
        ConverseStreamError::new(kind, None::<aws_smithy_types::error::operation::BuildError>).into()
    }

    #[test]
    fn model_overload_preserves_its_message_and_source() {
        let error = stream_error(ConverseStreamErrorKind::ModelOverloadedError);

        assert!(matches!(
            &error.kind,
            StreamErrorKind::ModelOverloaded { message }
                if message
                    == "The model you've selected is temporarily unavailable. Please use '/model' to select a different model and try again."
        ));
        assert!(
            error
                .as_concrete_error::<ConverseStreamError>()
                .is_some_and(|source| matches!(source.kind, ConverseStreamErrorKind::ModelOverloadedError))
        );
    }

    #[test]
    fn monthly_limit_preserves_its_message() {
        let error = stream_error(ConverseStreamErrorKind::MonthlyLimitReached);

        assert!(matches!(
            &error.kind,
            StreamErrorKind::MonthlyLimitReached { message }
                if message == "The monthly usage limit has been reached"
        ));
    }
}
