//! Scenario → modeled KRS type.
//!
//! This module is the whole of the mock's translation layer: it turns an
//! [`EventScript`] into a [`ChatResponseStream`] value from the generated KRS
//! **server** SDK. Serialization, event-stream framing and header naming are
//! the SDK's job — it was generated from `KiroRuntimeServiceModel`, so those
//! are the service's own rules rather than this crate's guesses.
//!
//! What that buys: a member KRS renames or drops stops compiling here, and a
//! frame this mock emits is byte-shaped the way the model says, without a
//! hand-written marshaller to keep in sync.

use amzn_kiro_runtime_service_server_sdk::error::{
    ChatResponseStreamError,
    InternalServerException,
    ServiceUnavailableException,
    ThrottlingException,
    ValidationError,
};
use amzn_kiro_runtime_service_server_sdk::model::{
    AssistantResponseEvent,
    ChatResponseStream,
    ContextUsageEvent,
    MetadataEvent,
    MeteringEvent,
    ReasoningContentEvent,
    StopReason,
    TokenUsage,
    ToolUseEvent,
};
use anyhow::Context as _;
use aws_smithy_types::Blob;

use crate::scenario::{
    EventScript,
    StreamError,
    StreamErrorKind,
    TokenUsageScript,
};

/// Builds the modeled union member a scenario event describes. `None` for
/// [`EventScript::Delay`], which is pacing rather than an event.
pub fn to_modeled_event(script: &EventScript) -> anyhow::Result<Option<ChatResponseStream>> {
    let event = match script {
        EventScript::Delay { .. } => return Ok(None),

        EventScript::Text {
            content,
            model_id,
            model_tag,
        } => ChatResponseStream::AssistantResponseEvent(
            AssistantResponseEvent::builder()
                .content(Some(content.clone()))
                .model_id(model_id.clone())
                .model_tag(model_tag.clone())
                .build(),
        ),

        EventScript::Reasoning {
            text,
            signature,
            redacted_content,
        } => ChatResponseStream::ReasoningContentEvent(
            ReasoningContentEvent::builder()
                .text(text.clone())
                .signature(signature.clone())
                // Modeled as a blob. Scenarios carry plain text for legibility;
                // base64 is the SDK's business, not the scenario author's.
                .redacted_content(redacted_content.as_ref().map(|value| Blob::new(value.as_bytes())))
                .build(),
        ),

        EventScript::ToolUse {
            tool_use_id,
            name,
            input,
            stop,
        } => ChatResponseStream::ToolUseEvent(
            ToolUseEvent::builder()
                .tool_use_id(Some(tool_use_id.clone()))
                .name(Some(name.clone()))
                .input(input.clone())
                .stop(*stop)
                .build(),
        ),

        EventScript::Metering {
            usage,
            unit,
            unit_plural,
        } => ChatResponseStream::MeteringEvent(
            MeteringEvent::builder()
                .usage(*usage)
                .unit(unit.clone())
                .unit_plural(unit_plural.clone())
                .build(),
        ),

        EventScript::ContextUsage {
            context_usage_percentage,
        } => ChatResponseStream::ContextUsageEvent(
            ContextUsageEvent::builder()
                .context_usage_percentage(*context_usage_percentage)
                .build(),
        ),

        EventScript::Metadata {
            stop_reason,
            token_usage,
        } => ChatResponseStream::MetadataEvent(
            MetadataEvent::builder()
                .stop_reason(stop_reason.as_deref().map(parse_stop_reason).transpose()?)
                .token_usage(token_usage.as_ref().map(to_modeled_token_usage))
                .build(),
        ),
    };
    Ok(Some(event))
}

/// The model enumerates the stop reasons, so a scenario naming one that KRS
/// does not have is a scripting error rather than a string passed through to
/// KAS.
fn parse_stop_reason(value: &str) -> anyhow::Result<StopReason> {
    StopReason::try_from(value).with_context(|| {
        format!(
            "unknown stopReason {value:?}; the model allows {:?}",
            StopReason::values()
        )
    })
}

fn to_modeled_token_usage(script: &TokenUsageScript) -> TokenUsage {
    TokenUsage::builder()
        .uncached_input_tokens(Some(script.uncached_input_tokens))
        .output_tokens(Some(script.output_tokens))
        .total_tokens(Some(script.total_tokens))
        .cache_read_input_tokens(script.cache_read_input_tokens)
        .cache_write_input_tokens(script.cache_write_input_tokens)
        .context_usage_percentage(script.context_usage_percentage)
        .normalized_token_usage(script.normalized_token_usage)
        .build()
}

/// The terminal event a healthy KRS response always closes with.
pub fn terminal_metadata_event(stop_reason: StopReason) -> ChatResponseStream {
    ChatResponseStream::MetadataEvent(MetadataEvent::builder().stop_reason(Some(stop_reason)).build())
}

/// Whether an event carries a stop reason, i.e. terminates the turn.
pub fn carries_stop_reason(event: &ChatResponseStream) -> bool {
    matches!(event, ChatResponseStream::MetadataEvent(metadata) if metadata.stop_reason().is_some())
}

/// Builds the modeled error a scripted mid-stream failure describes. The SDK
/// frames it as an `exception` message with the member name the model gives it,
/// which is what KAS's client dispatches on.
pub fn to_stream_error(error: &StreamError) -> ChatResponseStreamError {
    let message = Some(
        error
            .message
            .clone()
            .unwrap_or_else(|| format!("mock KRS injected {}", error.kind.shape_name())),
    );
    match error.kind {
        StreamErrorKind::Internal => ChatResponseStreamError::InternalServerException(
            InternalServerException::builder().message(message).build(),
        ),
        StreamErrorKind::Throttling => {
            ChatResponseStreamError::ThrottlingException(ThrottlingException::builder().message(message).build())
        },
        StreamErrorKind::Validation => {
            ChatResponseStreamError::ValidationError(ValidationError::builder().message(message).build())
        },
        StreamErrorKind::ServiceUnavailable => ChatResponseStreamError::ServiceUnavailableException(
            ServiceUnavailableException::builder().message(message).build(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modeled(script: EventScript) -> ChatResponseStream {
        to_modeled_event(&script)
            .expect("script is valid")
            .expect("not a delay")
    }

    #[test]
    fn text_event_maps_to_the_modeled_member() {
        let event = modeled(EventScript::Text {
            content: "hello".into(),
            model_id: Some("claude".into()),
            model_tag: None,
        });
        let inner = event.as_assistant_response_event().expect("assistantResponseEvent");
        assert_eq!(inner.content(), Some("hello"));
        assert_eq!(inner.model_id(), Some("claude"));
        assert_eq!(inner.model_tag(), None);
    }

    #[test]
    fn tool_use_event_carries_the_stop_flag() {
        let event = modeled(EventScript::ToolUse {
            tool_use_id: "tu-1".into(),
            name: "fs_read".into(),
            input: Some(r#"{"path":"/tmp"}"#.into()),
            stop: Some(true),
        });
        let inner = event.as_tool_use_event().expect("toolUseEvent");
        assert_eq!(inner.tool_use_id(), Some("tu-1"));
        assert_eq!(inner.name(), Some("fs_read"));
        assert_eq!(inner.stop(), Some(true));
    }

    #[test]
    fn metadata_event_carries_stop_reason_and_token_usage() {
        let event = modeled(EventScript::Metadata {
            stop_reason: Some("END_TURN".into()),
            token_usage: Some(TokenUsageScript {
                uncached_input_tokens: 10,
                output_tokens: 4,
                total_tokens: 14,
                ..Default::default()
            }),
        });
        assert!(carries_stop_reason(&event));
        let inner = event.as_metadata_event().expect("metadataEvent");
        assert_eq!(inner.stop_reason().map(StopReason::as_str), Some("END_TURN"));
        assert_eq!(inner.token_usage().and_then(TokenUsage::total_tokens), Some(14));
    }

    #[test]
    fn an_unmodeled_stop_reason_is_a_scripting_error() {
        // Caught here rather than shipped to KAS as an unknown string: the model
        // enumerates the stop reasons, so this mock can only emit those.
        let error = to_modeled_event(&EventScript::Metadata {
            stop_reason: Some("NOT_A_REAL_REASON".into()),
            token_usage: None,
        })
        .expect_err("unknown stop reason is rejected");
        assert!(format!("{error:#}").contains("NOT_A_REAL_REASON"));
    }

    #[test]
    fn metadata_without_a_stop_reason_is_not_terminal() {
        let event = modeled(EventScript::Metadata {
            stop_reason: None,
            token_usage: None,
        });
        assert!(!carries_stop_reason(&event));
    }

    #[test]
    fn delay_is_pacing_not_an_event() {
        assert!(to_modeled_event(&EventScript::Delay { ms: 5 }).unwrap().is_none());
    }

    #[test]
    fn scripted_stream_errors_map_to_modeled_union_members() {
        let error = to_stream_error(&StreamError {
            kind: StreamErrorKind::Throttling,
            message: Some("slow down".into()),
        });
        assert!(error.is_throttling_exception());
        assert_eq!(error.name(), "ThrottlingException");
        assert!(format!("{error}").contains("slow down"));
    }

    #[test]
    fn stream_errors_without_a_message_get_a_traceable_default() {
        let error = to_stream_error(&StreamError {
            kind: StreamErrorKind::Internal,
            message: None,
        });
        assert!(error.is_internal_server_exception());
        assert!(format!("{error}").contains("mock KRS injected"));
    }
}
