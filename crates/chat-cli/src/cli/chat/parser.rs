use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
    SystemTime,
    UNIX_EPOCH,
};

use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use tokio::sync::{
    Mutex,
    mpsc,
};
use tokio_util::sync::CancellationToken;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use super::message::{
    AssistantMessage,
    AssistantToolUse,
};
use crate::api_client::ApiClient;
use crate::api_client::error::ConverseStreamError;
use crate::api_client::model::{
    ChatResponseStream,
    ConversationState,
    ReasoningContentForHistory,
};
use crate::api_client::send_message_output::SendMessageOutput;
use crate::telemetry::ReasonCode;
use crate::telemetry::core::{
    ChatConversationType,
    MessageMetaTag,
};

/// Error from sending a SendMessage request.
#[derive(Debug, Error)]
pub struct SendMessageError {
    #[source]
    pub source: ConverseStreamError,
    pub request_metadata: RequestMetadata,
}

impl SendMessageError {
    pub fn status_code(&self) -> Option<u16> {
        self.source.status_code
    }
}

impl ReasonCode for SendMessageError {
    fn reason_code(&self) -> String {
        self.source.reason_code()
    }
}

impl std::fmt::Display for SendMessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to send the request: ")?;
        if let Some(request_id) = self.request_metadata.request_id.as_ref() {
            write!(f, "request_id: {request_id}, error: ")?;
        }
        write!(f, "{}", self.source)?;
        Ok(())
    }
}

/// Errors associated with consuming the response stream.
#[derive(Debug, Error)]
pub struct RecvError {
    #[source]
    pub source: RecvErrorKind,
    pub request_metadata: RequestMetadata,
}

impl RecvError {
    pub fn status_code(&self) -> Option<u16> {
        match &self.source {
            RecvErrorKind::Client(e) => e.status_code(),
            RecvErrorKind::Json(_) => None,
            RecvErrorKind::StreamTimeout { .. } => None,
            RecvErrorKind::UnexpectedToolUseEos { .. } => None,
            RecvErrorKind::Cancelled => None,
            RecvErrorKind::ToolValidationError { .. } => None,
            RecvErrorKind::EmptyResponse => None,
        }
    }
}

impl ReasonCode for RecvError {
    fn reason_code(&self) -> String {
        match &self.source {
            RecvErrorKind::Client(_) => "RecvErrorApiClient".to_string(),
            RecvErrorKind::Json(_) => "RecvErrorJson".to_string(),
            RecvErrorKind::StreamTimeout { .. } => "RecvErrorStreamTimeout".to_string(),
            RecvErrorKind::UnexpectedToolUseEos { .. } => "RecvErrorUnexpectedToolUseEos".to_string(),
            RecvErrorKind::Cancelled => "Interrupted".to_string(),
            RecvErrorKind::ToolValidationError { .. } => "RecvErrorToolValidation".to_string(),
            RecvErrorKind::EmptyResponse => "RecvErrorEmptyResponse".to_string(),
        }
    }
}

impl std::fmt::Display for RecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to receive the next message: ")?;
        if let Some(request_id) = self.request_metadata.request_id.as_ref() {
            write!(f, "request_id: {request_id}, error: ")?;
        }
        write!(f, "{}", self.source)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum RecvErrorKind {
    #[error("{0}")]
    Client(#[from] crate::api_client::ApiClientError),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// An error was encountered while waiting for the next event in the stream after a noticeably
    /// long wait time.
    ///
    /// *Context*: the client can throw an error after ~100s of waiting with no response, likely due
    /// to an exceptionally complex tool use taking too long to generate.
    #[error("The stream ended after {}s: {source}", .duration.as_secs())]
    StreamTimeout {
        source: crate::api_client::ApiClientError,
        duration: std::time::Duration,
        /// Which mechanism abandoned the stream. Retry behavior treats both
        /// identically; the stall telemetry family must not: it is scoped to the
        /// locally enforced idle watchdog, and mixing in slow SDK failures would
        /// contaminate the watchdog's efficacy and recovery metrics (V2 applies
        /// the same gate).
        timeout_source: agent::agent_loop::types::StreamTimeoutSource,
    },
    /// Unexpected end of stream while receiving a tool use.
    ///
    /// *Context*: the stream can unexpectedly end with `Ok(None)` while waiting for an
    /// exceptionally complex tool use. This is due to some proxy server dropping idle
    /// connections after some timeout is reached.
    ///
    /// TODO: should this be removed?
    #[error("Unexpected end of stream for tool: {} with id: {}", .name, .tool_use_id)]
    UnexpectedToolUseEos {
        tool_use_id: String,
        name: String,
        message: Box<AssistantMessage>,
        time_elapsed: Duration,
    },
    /// The stream processing task was cancelled
    #[error("Stream handling was cancelled")]
    Cancelled,
    /// Tool validation failed due to invalid arguments
    #[error("Tool validation failed for tool: {} with id: {}", .name, .tool_use_id)]
    ToolValidationError {
        tool_use_id: String,
        name: String,
        message: Box<AssistantMessage>,
        error_message: String,
    },
    /// The response stream completed cleanly but produced no content events (no assistant text,
    /// tool use, or thinking).
    ///
    /// # Context
    ///
    /// This is the client-side fingerprint of Bedrock's `stopReason=content_filtered` outcome:
    /// HTTP 200, MeteringEvent + MetadataEvent only, no `AssistantResponseEvent` or
    /// `ToolUseEvent` or `ReasoningEvent`. Surfaced as an error so the chat loop can retry the
    /// request.
    #[error("Kiro failed to generate a response")]
    EmptyResponse,
}

/// Classifies a stream-read failure. A long wait before the error suggests a
/// stalled generation, classified as [RecvErrorKind::StreamTimeout] so the chat
/// loop retries with a continuation prompt — EXCEPT for errors the chat loop
/// handles by their own classification regardless of latency: context overflow
/// is terminal (retrying the same conversation can never succeed, it must reach
/// the compaction path), and a modeled mid-stream InternalServerError has its
/// own bounded clean re-send (rebranding it as a stall would blame the model
/// for a service 500 and bypass that budget).
fn classify_recv_failure(err: crate::api_client::ApiClientError, duration: Duration) -> RecvErrorKind {
    if duration.as_secs() >= 59 && !err.is_context_window_overflow() && !err.is_mid_stream_internal_server_error() {
        RecvErrorKind::StreamTimeout {
            source: err,
            duration,
            timeout_source: agent::agent_loop::types::StreamTimeoutSource::SdkRecv,
        }
    } else {
        RecvErrorKind::Client(err)
    }
}

/// The error shape for a request that received no response headers within the
/// idle deadline. Carries a `TimedOut` io source so `transient_class` reads it
/// as a network-class transient failure and the bounded send retry applies.
fn pre_headers_timeout_error(deadline: Duration) -> crate::api_client::error::ConverseStreamError {
    crate::api_client::error::ConverseStreamError {
        request_id: None,
        status_code: None,
        retry_after: None,
        kind: crate::api_client::error::ConverseStreamErrorKind::Unknown {
            reason_code: "RequestHeadersTimeout".to_string(),
        },
        source: Some(crate::api_client::error::ConverseStreamSdkError::SmithyBuild(
            aws_smithy_types::error::operation::BuildError::other(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("no response headers received within {}s", deadline.as_secs()),
            )),
        )),
    }
}

/// Represents a response stream from a call to the SendMessage API.
///
/// Send a request using [Self::send_message].
#[derive(Debug)]
pub struct SendMessageStream {
    request_id: Option<String>,
    ev_rx: mpsc::Receiver<Result<ResponseEvent, RecvError>>,
    /// Used for graceful cleanup of the stream handler task. Required for setting request metadata
    /// on drop (e.g. in the sigint case).
    cancel_token: CancellationToken,
}

impl Drop for SendMessageStream {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl SendMessageStream {
    /// Sends a SendMessage request to the backend, returning the response stream to consume.
    ///
    /// You should repeatedly call [Self::recv] to receive [ResponseEvent]'s until a
    /// [ResponseEvent::EndStream] value is returned.
    ///
    /// # Arguments
    ///
    /// * `client` - api client to make the request with
    /// * `conversation_state` - the [crate::api_client::model::ConversationState] to send
    /// * `request_metadata_lock` - a mutex that will be updated with metadata about the consumed
    ///   response stream on stream completion (ie, [ResponseEvent::EndStream] is returned) or on
    ///   drop.
    ///
    /// # Details
    ///
    /// Why `request_metadata_lock`? Because when a sigint occurs, we need to capture how much of
    /// the response stream was consumed for telemetry purposes. From the sigint handler, there's
    /// no easy way around this currently without a solution that requires global state - hence, a
    /// mutex.
    ///
    /// Internally, [Self::send_message] spawns a new task that will continually consume the
    /// response stream which will be cancelled when [Self] is dropped (e.g., when the surrounding
    /// future is aborted in the sigint case). The task will gracefully end with updating the mutex
    /// with [RequestMetadata].
    pub async fn send_message(
        client: &ApiClient,
        conversation_state: ConversationState,
        request_metadata_lock: Arc<Mutex<Option<RequestMetadata>>>,
        message_meta_tags: Option<Vec<MessageMetaTag>>,
        stream_idle_timeout: Duration,
    ) -> Result<Self, SendMessageError> {
        let message_id = uuid::Uuid::new_v4().to_string();
        info!(?message_id, "Generated new message id");
        let user_prompt_length = conversation_state.user_input_message.content.len();
        let model_id = conversation_state.user_input_message.model_id.clone();
        let message_meta_tags = message_meta_tags.unwrap_or_default();

        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();

        let start_time = Instant::now();
        let start_time_sys = SystemTime::now();
        debug!(?start_time, "sending send_message request");
        // The idle deadline below only covers the response body; without a bound
        // here, the pre-headers wait (the SDK send itself) is limited only by the
        // 1h streaming read timeout, so a server that accepts the connection but
        // never sends headers hangs the prompt for up to an hour. Apply the same
        // deadline, shaped as a transient timeout so the bounded send retry gets
        // a chance before the turn ends.
        let send_future = client.send_message(conversation_state);
        let response = if stream_idle_timeout.is_zero() {
            send_future.await
        } else {
            match tokio::time::timeout(stream_idle_timeout, send_future).await {
                Ok(result) => result,
                Err(_) => Err(pre_headers_timeout_error(stream_idle_timeout)),
            }
        };
        let response = response.map_err(|err| SendMessageError {
            source: err,
            request_metadata: RequestMetadata {
                message_id: message_id.clone(),
                request_start_timestamp_ms: system_time_to_unix_ms(start_time_sys),
                stream_end_timestamp_ms: system_time_to_unix_ms(SystemTime::now()),
                model_id: model_id.clone(),
                user_prompt_length,
                message_meta_tags: message_meta_tags.clone(),
                // Other fields are irrelevant if we can't get a successful response
                ..Default::default()
            },
        })?;
        let elapsed = start_time.elapsed();
        debug!(?elapsed, "send_message succeeded");

        let request_id = response.request_id().map(str::to_string);
        let (ev_tx, ev_rx) = mpsc::channel(16);
        tokio::spawn(async move {
            ResponseParser::new(
                response,
                message_id,
                model_id,
                user_prompt_length,
                message_meta_tags,
                ev_tx,
                start_time,
                start_time_sys,
                cancel_token_clone,
                request_metadata_lock,
                stream_idle_timeout,
            )
            .try_recv()
            .await;
        });

        Ok(Self {
            request_id,
            cancel_token,
            ev_rx,
        })
    }

    pub async fn recv(&mut self) -> Option<Result<ResponseEvent, RecvError>> {
        self.ev_rx.recv().await
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
}

/// State of a tool use being parsed from the response stream.
#[derive(Debug)]
struct PendingToolUse {
    id: String,
    name: String,
    /// Input payload from the first [ChatResponseStream::ToolUseEvent], if present.
    initial_input: Option<String>,
    /// Stop flag from the first [ChatResponseStream::ToolUseEvent], if present.
    initial_stop: Option<bool>,
}

/// State associated with parsing a [ChatResponseStream] into a [Message].
///
/// # Usage
///
/// You should repeatedly call [Self::recv] to receive [ResponseEvent]'s until a
/// [ResponseEvent::EndStream] value is returned.
#[derive(Debug)]
struct ResponseParser {
    /// The response to consume and parse into a sequence of [ResponseEvent].
    response: SendMessageOutput,
    event_tx: mpsc::Sender<Result<ResponseEvent, RecvError>>,

    /// Message identifier for the assistant's response. Randomly generated on creation.
    message_id: String,
    /// Whether or not the stream has completed.
    ended: bool,
    /// Buffer to hold the next event in [SendMessageOutput].
    peek: Option<ChatResponseStream>,
    /// Buffer for holding the accumulated assistant response.
    assistant_text: String,
    /// Tool uses requested by the model.
    tool_uses: Vec<AssistantToolUse>,
    /// Whether or not we are currently receiving tool use delta events.
    parsing_tool_use: Option<PendingToolUse>,
    /// Visible text of the current, not-yet-sealed thinking block.
    thinking_text: String,
    /// Signature of the current thinking block, set when its sealing event arrives.
    thinking_signature: Option<String>,
    /// Redacted (encrypted) content of the current thinking block.
    thinking_redacted_content: Option<Vec<u8>>,
    /// The most recently sealed thinking block this turn. A turn may emit multiple
    /// thinking blocks, but history carries only one, so we keep the LAST sealed block —
    /// the reasoning that led to the turn's final output. Replaced each time a block
    /// seals; a trailing unsealed block never lands here and is dropped.
    last_sealed_thinking: Option<ReasoningContentForHistory>,
    /// Whether the stream ever carried any content event (assistant text, tool use, or
    /// thinking).
    received_content_event: bool,

    request_metadata: Arc<Mutex<Option<RequestMetadata>>>,
    cancel_token: CancellationToken,
    /// Inter-event stream silence after which the stream is abandoned with a
    /// [RecvErrorKind::StreamTimeout]. Zero disables the deadline.
    idle_timeout: Duration,

    // metadata fields
    /// Id of the model used with this request.
    model_id: Option<String>,
    /// Length of the user prompt for the initial request.
    user_prompt_length: usize,
    /// Meta tags for the initial request.
    message_meta_tags: Vec<MessageMetaTag>,
    /// Time immediately before sending the request.
    request_start_time: Instant,
    /// Time immediately before sending the request, as a [SystemTime].
    request_start_time_sys: SystemTime,
    /// Total size (in bytes) of the response received so far.
    received_response_size: usize,
    time_to_first_chunk: Option<Duration>,
    time_between_chunks: Vec<Duration>,
    context_usage_percentage: Option<f32>,
    total_tokens: Option<i32>,
    uncached_input_tokens: Option<i32>,
    output_tokens: Option<i32>,
    cache_read_input_tokens: Option<i32>,
    cache_write_input_tokens: Option<i32>,
}

impl ResponseParser {
    #[allow(clippy::too_many_arguments)]
    fn new(
        response: SendMessageOutput,
        message_id: String,
        model_id: Option<String>,
        user_prompt_length: usize,
        message_meta_tags: Vec<MessageMetaTag>,
        event_tx: mpsc::Sender<Result<ResponseEvent, RecvError>>,
        request_start_time: Instant,
        request_start_time_sys: SystemTime,
        cancel_token: CancellationToken,
        request_metadata: Arc<Mutex<Option<RequestMetadata>>>,
        idle_timeout: Duration,
    ) -> Self {
        Self {
            response,
            message_id,
            model_id,
            user_prompt_length,
            message_meta_tags,
            ended: false,
            event_tx,
            peek: None,
            assistant_text: String::new(),
            tool_uses: Vec::new(),
            parsing_tool_use: None,
            thinking_text: String::new(),
            thinking_signature: None,
            thinking_redacted_content: None,
            last_sealed_thinking: None,
            received_content_event: false,
            request_start_time,
            request_start_time_sys,
            received_response_size: 0,
            time_to_first_chunk: None,
            time_between_chunks: Vec::new(),
            request_metadata,
            context_usage_percentage: None,
            total_tokens: None,
            uncached_input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            cancel_token,
            idle_timeout,
        }
    }

    async fn try_recv(&mut self) {
        loop {
            if self.ended {
                trace!("response stream has ended");
                return;
            }

            let cancel_token = self.cancel_token.clone();
            tokio::select! {
                res = self.recv() => {
                    let _ = self.event_tx.send(res).await.map_err(|err| error!(?err, "failed to send event to channel"));
                },
                _ = cancel_token.cancelled() => {
                    debug!("response parser was cancelled");
                    let err = self.error(RecvErrorKind::Cancelled);
                    *self.request_metadata.lock().await = Some(err.request_metadata.clone());
                    let _ = self.event_tx.send(Err(err)).await.map_err(|err| error!(?err, "failed to send error to channel"));
                    return;
                },
            }
        }
    }

    /// Consumes the associated [ConverseStreamResponse] until a valid [ResponseEvent] is parsed.
    async fn recv(&mut self) -> Result<ResponseEvent, RecvError> {
        if let Some(pending) = self.parsing_tool_use.take() {
            let tool_use = self
                .parse_tool_use(pending.id, pending.name, pending.initial_input, pending.initial_stop)
                .await?;
            self.tool_uses.push(tool_use.clone());
            return Ok(ResponseEvent::ToolUse(tool_use));
        }

        // First, handle discarding AssistantResponseEvent's that immediately precede a
        // CodeReferenceEvent.
        let peek = self.peek().await?;
        if let Some(ChatResponseStream::AssistantResponseEvent { content }) = peek {
            // Cloning to bypass borrowchecker stuff.
            let content = content.clone();
            self.next().await?;
            self.received_content_event = true;
            match self.peek().await? {
                Some(ChatResponseStream::CodeReferenceEvent(_)) => (),
                _ => {
                    self.assistant_text.push_str(&content);
                    return Ok(ResponseEvent::AssistantText(content));
                },
            }
        }

        loop {
            match self.next().await {
                Ok(Some(output)) => match output {
                    ChatResponseStream::AssistantResponseEvent { content } => {
                        self.received_content_event = true;
                        self.assistant_text.push_str(&content);
                        return Ok(ResponseEvent::AssistantText(content));
                    },
                    ChatResponseStream::InvalidStateEvent { reason, message } => {
                        error!(%reason, %message, "invalid state event");
                    },
                    ChatResponseStream::ToolUseEvent {
                        tool_use_id,
                        name,
                        input,
                        stop,
                    } => {
                        self.received_content_event = true;
                        self.parsing_tool_use = Some(PendingToolUse {
                            id: tool_use_id.clone(),
                            name: name.clone(),
                            initial_input: input,
                            initial_stop: stop,
                        });
                        return Ok(ResponseEvent::ToolUseStart { name });
                    },
                    ChatResponseStream::ReasoningEvent {
                        text,
                        signature,
                        redacted_content,
                    } => {
                        self.received_content_event = true;
                        // Keep the LAST sealed thinking block. Text and signature/redacted
                        // content arrive in separate reasoning deltas: text accumulates into the
                        // current block, and a signature or redacted-content event seals it. Each
                        // newly sealed block replaces the previous one in `last_sealed_thinking`,
                        // so the block produced just before the turn's final output wins. A
                        // trailing unsealed block (e.g. a truncated final thinking block) is never
                        // sealed and is dropped — Bedrock rejects unsigned thinking in history.
                        // Text deltas are emitted as `ThinkingText` events (currently a no-op in
                        // the classic consumer).
                        if let Some(text) = text {
                            self.thinking_text.push_str(&text);
                            return Ok(ResponseEvent::ThinkingText);
                        }
                        if signature.is_some() {
                            self.thinking_signature = signature;
                        }
                        if redacted_content.is_some() {
                            self.thinking_redacted_content = redacted_content;
                        }
                        if self.thinking_signature.is_some() || self.thinking_redacted_content.is_some() {
                            self.last_sealed_thinking = Some(ReasoningContentForHistory {
                                text: std::mem::take(&mut self.thinking_text),
                                signature: self.thinking_signature.take(),
                                redacted_content: self.thinking_redacted_content.take().unwrap_or_default(),
                                model_id: self.model_id.clone(),
                            });
                        }
                    },
                    ref event if event.is_skippable_metadata() => {},
                    _ => {
                        warn!(?output, "received unexpected event type in main parsing loop");
                    },
                },
                Ok(None) => {
                    if !self.received_content_event {
                        let request_metadata = self.make_metadata(None);
                        *self.request_metadata.lock().await = Some(request_metadata.clone());
                        self.ended = true;
                        return Err(self.error(RecvErrorKind::EmptyResponse));
                    }
                    let message_id = Some(self.message_id.clone());
                    let content = std::mem::take(&mut self.assistant_text);
                    let thinking = self.take_thinking();
                    let (mut message, conv_type) = if self.tool_uses.is_empty() {
                        (
                            AssistantMessage::new_response(message_id, content),
                            ChatConversationType::NotToolUse,
                        )
                    } else {
                        (
                            AssistantMessage::new_tool_use(
                                message_id,
                                content,
                                self.tool_uses.clone().into_iter().collect(),
                            ),
                            ChatConversationType::ToolUse,
                        )
                    };
                    if let Some(t) = thinking {
                        message.set_thinking(t);
                    }
                    let request_metadata = self.make_metadata(Some(conv_type));
                    *self.request_metadata.lock().await = Some(request_metadata.clone());
                    self.ended = true;
                    return Ok(ResponseEvent::EndStream {
                        message,
                        request_metadata,
                    });
                },
                Err(err) => return Err(err),
            }
        }
    }

    /// Consumes the response stream until a valid [ToolUse] is parsed.
    ///
    /// The arguments are the fields from the first [ChatResponseStream::ToolUseEvent] consumed.
    async fn parse_tool_use(
        &mut self,
        id: String,
        name: String,
        initial_input: Option<String>,
        initial_stop: Option<bool>,
    ) -> Result<AssistantToolUse, RecvError> {
        let mut tool_string = initial_input.unwrap_or_default();
        let start = Instant::now();
        if initial_stop != Some(true) {
            loop {
                match self.peek().await? {
                    Some(ChatResponseStream::ToolUseEvent { .. }) => {
                        if let Some(ChatResponseStream::ToolUseEvent { input, stop, .. }) = self.next().await? {
                            if let Some(i) = input {
                                tool_string.push_str(&i);
                            }
                            if let Some(true) = stop {
                                break;
                            }
                        }
                    },
                    Some(event) if event.is_skippable_metadata() => {
                        // Skip metadata events during tool use parsing
                        self.next().await?;
                    },
                    _other => {
                        break;
                    },
                }
            }
        }

        let args = match serde_json::from_str(&tool_string) {
            Ok(args) => {
                // Ensure we have a valid JSON object
                match args {
                    serde_json::Value::Object(_) => args,
                    _ => {
                        error!("Received non-object JSON for tool arguments: {:?}", args);
                        let warning_args = serde_json::Value::Object(
                            [(
                                "key".to_string(),
                                serde_json::Value::String(
                                    "WARNING: the actual tool use arguments were not a valid JSON object".to_string(),
                                ),
                            )]
                            .into_iter()
                            .collect(),
                        );
                        self.tool_uses.push(AssistantToolUse {
                            id: id.clone(),
                            name: name.clone(),
                            orig_name: name.clone(),
                            args: warning_args.clone(),
                            orig_args: warning_args.clone(),
                        });
                        let message = Box::new(AssistantMessage::new_tool_use(
                            Some(self.message_id.clone()),
                            std::mem::take(&mut self.assistant_text),
                            self.tool_uses.clone().into_iter().collect(),
                        ));
                        return Err(self.error(RecvErrorKind::ToolValidationError {
                            tool_use_id: id,
                            name,
                            message,
                            error_message: format!("Expected JSON object, got: {args:?}"),
                        }));
                    },
                }
            },
            Err(err) if !tool_string.is_empty() => {
                // If we failed deserializing after waiting for a long time, then this is most
                // likely bedrock responding with a stop event for some reason without actually
                // including the tool contents. Essentially, the tool was too large.
                let time_elapsed = start.elapsed();
                let args = serde_json::Value::Object(
                    [(
                        "key".to_string(),
                        serde_json::Value::String(
                            "WARNING: the actual tool use arguments were too complicated to be generated".to_string(),
                        ),
                    )]
                    .into_iter()
                    .collect(),
                );
                if self.peek().await?.is_none() {
                    error!(
                        "Received an unexpected end of stream after spending ~{}s receiving tool events",
                        time_elapsed.as_secs_f64()
                    );
                    self.tool_uses.push(AssistantToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        orig_name: name.clone(),
                        args: args.clone(),
                        orig_args: args.clone(),
                    });
                    let message = Box::new(AssistantMessage::new_tool_use(
                        Some(self.message_id.clone()),
                        std::mem::take(&mut self.assistant_text),
                        self.tool_uses.clone().into_iter().collect(),
                    ));
                    return Err(self.error(RecvErrorKind::UnexpectedToolUseEos {
                        tool_use_id: id,
                        name,
                        message,
                        time_elapsed,
                    }));
                } else {
                    return Err(self.error(err));
                }
            },
            // if the tool just does not need any input
            _ => serde_json::json!({}),
        };
        let orig_name = name.clone();
        let orig_args = args.clone();
        Ok(AssistantToolUse {
            id,
            name,
            orig_name,
            args,
            orig_args,
        })
    }

    /// Returns the next event in the [SendMessageOutput] without consuming it.
    async fn peek(&mut self) -> Result<Option<&ChatResponseStream>, RecvError> {
        if self.peek.is_some() {
            return Ok(self.peek.as_ref());
        }
        match self.next().await? {
            Some(v) => {
                self.peek = Some(v);
                Ok(self.peek.as_ref())
            },
            None => Ok(None),
        }
    }

    /// Consumes the next [SendMessageOutput] event.
    async fn next(&mut self) -> Result<Option<ChatResponseStream>, RecvError> {
        if let Some(ev) = self.peek.take() {
            return Ok(Some(ev));
        }
        trace!("Attempting to recv next event");
        let start = std::time::Instant::now();
        // Client-side inter-event deadline: without it, this await can hang forever when
        // the connection dies silently (no error, no events, no server-side kill).
        let result = if self.idle_timeout.is_zero() {
            self.response.recv().await
        } else {
            match tokio::time::timeout(self.idle_timeout, self.response.recv()).await {
                Ok(result) => result,
                Err(_) => {
                    let duration = start.elapsed();
                    error!(?duration, "the response stream exceeded the idle deadline");
                    return Err(self.error(RecvErrorKind::StreamTimeout {
                        source: crate::api_client::ApiClientError::Other(format!(
                            "no stream event received within {}s",
                            self.idle_timeout.as_secs()
                        )),
                        duration,
                        timeout_source: agent::agent_loop::types::StreamTimeoutSource::IdleWatchdog,
                    }));
                },
            }
        };
        let duration = std::time::Instant::now().duration_since(start);
        match result {
            Ok(ev) => {
                trace!(?ev, "Received new event");

                // Track metadata about the chunk.
                self.time_to_first_chunk
                    .get_or_insert_with(|| self.request_start_time.elapsed());
                self.time_between_chunks.push(duration);
                if let Some(r) = ev.as_ref() {
                    match r {
                        ChatResponseStream::AssistantResponseEvent { content } => {
                            self.received_response_size += content.len();
                        },
                        ChatResponseStream::ToolUseEvent { input, .. } => {
                            self.received_response_size += input.as_ref().map(String::len).unwrap_or_default();
                        },
                        ChatResponseStream::MeteringEvent {
                            usage,
                            unit,
                            unit_plural,
                        } => {
                            info!("GenerateAssistanceResponse - MeteringEvent");
                            if let (Some(value), Some(unit), Some(unit_plural)) = (usage, unit, unit_plural) {
                                let _ = self
                                    .event_tx
                                    .send(Ok(ResponseEvent::MeteringUsage {
                                        request_id: self.response.request_id().map(String::from),
                                        model: self.model_id.clone(),
                                        value: *value,
                                        unit: unit.clone(),
                                        unit_plural: unit_plural.clone(),
                                    }))
                                    .await;
                            }
                        },
                        ChatResponseStream::ContextUsageEvent {
                            context_usage_percentage,
                        } => {
                            self.context_usage_percentage = Some(*context_usage_percentage);
                        },
                        ChatResponseStream::MetadataEvent {
                            total_tokens,
                            uncached_input_tokens,
                            output_tokens,
                            cache_read_input_tokens,
                            cache_write_input_tokens,
                        } => {
                            set_if_some(&mut self.total_tokens, *total_tokens);
                            set_if_some(&mut self.uncached_input_tokens, *uncached_input_tokens);
                            set_if_some(&mut self.output_tokens, *output_tokens);
                            set_if_some(&mut self.cache_read_input_tokens, *cache_read_input_tokens);
                            set_if_some(&mut self.cache_write_input_tokens, *cache_write_input_tokens);
                        },
                        _ => {
                            warn!(?r, "received unexpected event from the response stream");
                        },
                    }
                }

                Ok(ev)
            },
            Err(err) => {
                error!(?err, "failed to receive the next event");
                Err(self.error(classify_recv_failure(err, duration)))
            },
        }
    }

    /// Helper to create a new [RecvError] populated with the associated request id for the stream.
    fn error(&self, source: impl Into<RecvErrorKind>) -> RecvError {
        RecvError {
            source: source.into(),
            request_metadata: self.make_metadata(None),
        }
    }

    /// Returns the last sealed thinking block of the turn, or `None` if no block sealed.
    /// See the `ReasoningEvent` handler for the keep-last-sealed accumulation; a trailing
    /// unsealed block is intentionally dropped (Bedrock rejects unsigned thinking).
    fn take_thinking(&mut self) -> Option<ReasoningContentForHistory> {
        self.last_sealed_thinking.take()
    }

    fn make_metadata(&self, chat_conversation_type: Option<ChatConversationType>) -> RequestMetadata {
        RequestMetadata {
            request_id: self.response.request_id().map(String::from),
            context_usage_percentage: self.context_usage_percentage,
            message_id: self.message_id.clone(),
            time_to_first_chunk: self.time_to_first_chunk,
            time_between_chunks: self.time_between_chunks.clone(),
            response_size: self.received_response_size,
            chat_conversation_type,
            request_start_timestamp_ms: system_time_to_unix_ms(self.request_start_time_sys),
            // We always end the stream when this method is called, so just set the end timestamp
            // here.
            stream_end_timestamp_ms: system_time_to_unix_ms(SystemTime::now()),
            user_prompt_length: self.user_prompt_length,
            message_meta_tags: self.message_meta_tags.clone(),
            tool_use_ids_and_names: self
                .tool_uses
                .iter()
                .map(|t| (t.id.clone(), t.name.clone()))
                .collect::<_>(),
            model_id: self.model_id.clone(),
            total_tokens: self.total_tokens,
            uncached_input_tokens: self.uncached_input_tokens,
            output_tokens: self.output_tokens,
            cache_read_input_tokens: self.cache_read_input_tokens,
            cache_write_input_tokens: self.cache_write_input_tokens,
        }
    }
}

fn set_if_some<T: Copy>(target: &mut Option<T>, value: Option<T>) {
    if value.is_some() {
        *target = value;
    }
}

#[derive(Debug)]
pub enum ResponseEvent {
    /// Text returned by the assistant. This should be displayed to the user as it is received.
    AssistantText(String),
    /// Notification that a tool use is being received.
    ToolUseStart { name: String },
    /// A tool use requested by the assistant. This should be displayed to the user as it is
    /// received.
    ToolUse(AssistantToolUse),
    /// Represents the end of the response. No more events will be returned.
    EndStream {
        /// The completed message containing all of the assistant text and tool use events
        /// previously emitted. This should be stored in the conversation history and sent in
        /// subsequent requests.
        message: AssistantMessage,
        /// Metadata for the request stream.
        request_metadata: RequestMetadata,
    },
    /// Metering usage consumed for this request
    MeteringUsage {
        request_id: Option<String>,
        model: Option<String>,
        value: f64,
        unit: String,
        unit_plural: String,
    },
    /// Thinking/reasoning text from extended thinking models.
    ThinkingText,
}

/// Metadata about the sent request and associated response stream.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestMetadata {
    /// The request id associated with the [SendMessageOutput] stream.
    pub request_id: Option<String>,
    /// Context usage percentage from backend ContextUsageEvent.
    pub context_usage_percentage: Option<f32>,
    /// The randomly-generated id associated with the request. Equivalent to utterance id.
    pub message_id: String,
    /// Unix timestamp (milliseconds) immediately before sending the request.
    pub request_start_timestamp_ms: u64,
    /// Unix timestamp (milliseconds) once the stream has either completed or ended in an error.
    pub stream_end_timestamp_ms: u64,
    /// Time until the first chunk was received.
    pub time_to_first_chunk: Option<Duration>,
    /// Time between each received chunk in the stream.
    pub time_between_chunks: Vec<Duration>,
    /// Total size (in bytes) of the user prompt associated with the request.
    pub user_prompt_length: usize,
    /// Total size (in bytes) of the response.
    pub response_size: usize,
    /// [ChatConversationType] for the returned assistant message.
    pub chat_conversation_type: Option<ChatConversationType>,
    /// Tool uses returned by the assistant for this request.
    pub tool_use_ids_and_names: Vec<(String, String)>,
    /// Model id.
    pub model_id: Option<String>,
    /// Meta tags for the request.
    pub message_meta_tags: Vec<MessageMetaTag>,
    /// Total tokens reported by the backend for this request.
    #[serde(default)]
    pub total_tokens: Option<i32>,
    /// Uncached input tokens reported by the backend for this request.
    #[serde(default)]
    pub uncached_input_tokens: Option<i32>,
    /// Output tokens reported by the backend for this request.
    #[serde(default)]
    pub output_tokens: Option<i32>,
    /// Cache-read input tokens reported by the backend for this request.
    #[serde(default)]
    pub cache_read_input_tokens: Option<i32>,
    /// Cache-write input tokens reported by the backend for this request.
    #[serde(default)]
    pub cache_write_input_tokens: Option<i32>,
}

fn system_time_to_unix_ms(time: SystemTime) -> u64 {
    (time
        .duration_since(UNIX_EPOCH)
        .expect("time should never be before unix epoch")
        .as_secs_f64()
        * 1000.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overflow_error() -> crate::api_client::ApiClientError {
        crate::api_client::ApiClientError::ConverseStream(ConverseStreamError {
            request_id: None,
            status_code: Some(400),
            retry_after: None,
            kind: crate::api_client::error::ConverseStreamErrorKind::ContextWindowOverflow,
            source: None,
        })
    }

    /// A slow overflow failure must keep its overflow classification: rerouting it
    /// through StreamTimeout would feed the stall-continuation retry loop with a
    /// conversation that can never fit (the 40x-retry incident shape).
    #[test]
    fn slow_overflow_failure_is_not_reclassified_as_timeout() {
        let kind = classify_recv_failure(overflow_error(), Duration::from_secs(75));
        assert!(
            matches!(&kind, RecvErrorKind::Client(e) if e.is_context_window_overflow()),
            "expected overflow to keep its Client classification, got: {kind:?}"
        );
    }

    /// A modeled mid-stream InternalServerError observed after a long wait must
    /// keep its Client classification so the chat loop's bounded clean re-send
    /// applies, instead of being rebranded as a stall — which would synthesize a
    /// timeout continuation and bypass the 5xx retry budget.
    #[test]
    fn slow_mid_stream_5xx_keeps_client_classification() {
        use amzn_codewhisperer_streaming_client::types::error::ChatResponseStreamError;
        use aws_smithy_types::event_stream::{
            Message,
            RawMessage,
        };

        let ise = amzn_codewhisperer_streaming_client::types::error::InternalServerError::builder()
            .message("boom")
            .build()
            .unwrap();
        let err = crate::api_client::ApiClientError::CodewhispererChatResponseStream(
            crate::api_client::error::SdkError::service_error(
                ChatResponseStreamError::InternalServerError(ise),
                RawMessage::Decoded(Message::new(b"<payload>".to_vec())),
            ),
        );
        let kind = classify_recv_failure(err, Duration::from_secs(75));
        assert!(
            matches!(&kind, RecvErrorKind::Client(e) if e.is_mid_stream_internal_server_error()),
            "expected the mid-stream 5xx to keep its Client classification, got: {kind:?}"
        );
    }

    /// Non-overflow failures after a long wait still classify as StreamTimeout.
    #[test]
    fn slow_generic_failure_still_classifies_as_timeout() {
        let err = crate::api_client::ApiClientError::Other("connection reset".to_string());
        let kind = classify_recv_failure(err, Duration::from_secs(75));
        assert!(matches!(kind, RecvErrorKind::StreamTimeout { .. }));
    }

    /// A stream that goes silent forever must fail with StreamTimeout once the
    /// inter-event idle deadline elapses, instead of hanging the turn.
    #[tokio::test(start_paused = true)]
    async fn test_idle_deadline_times_out_silent_stream() {
        let mock = SendMessageOutput::MockSilent(vec![]);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::from_secs(120),
        );

        // The error's duration is wall-clock and stays near zero under paused tokio
        // time, so only the error kind is asserted.
        match parser.recv().await {
            Err(RecvError {
                source: RecvErrorKind::StreamTimeout { .. },
                ..
            }) => {},
            other => panic!("expected StreamTimeout after the idle deadline, got: {other:?}"),
        }
    }

    /// A zero idle deadline disables the client-side timeout entirely.
    #[tokio::test(start_paused = true)]
    async fn test_idle_deadline_zero_disables_timeout() {
        let mock = SendMessageOutput::MockSilent(vec![]);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        // With the deadline disabled the recv never resolves; a bounded wait must time
        // out at the caller instead of the parser returning an error.
        let res = tokio::time::timeout(Duration::from_secs(3600), parser.recv()).await;
        assert!(res.is_err(), "recv should still be pending with a zero idle deadline");
    }

    #[tokio::test]
    async fn test_response_parser_ignores_licensed_code() {
        // let _ = tracing_subscriber::fmt::try_init();

        let content_to_ignore = "IGNORE ME PLEASE";
        let tool_use_id = "TEST_ID".to_string();
        let tool_name = "execute_bash".to_string();
        let tool_args = serde_json::json!({
            "command": "echo hello"
        })
        .to_string();
        let tool_use_split_at = 5;
        let mut events = vec![
            ChatResponseStream::AssistantResponseEvent {
                content: "hi".to_string(),
            },
            ChatResponseStream::AssistantResponseEvent {
                content: " there".to_string(),
            },
            ChatResponseStream::AssistantResponseEvent {
                content: content_to_ignore.to_string(),
            },
            ChatResponseStream::CodeReferenceEvent(()),
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: None,
                stop: None,
            },
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: Some(tool_args.as_str().split_at(tool_use_split_at).0.to_string()),
                stop: None,
            },
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: Some(tool_args.as_str().split_at(tool_use_split_at).1.to_string()),
                stop: None,
            },
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: None,
                stop: Some(true),
            },
        ];
        events.reverse();
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        let mut output = String::new();
        for _ in 0..5 {
            output.push_str(&format!("{:?}", parser.recv().await.unwrap()));
        }

        assert!(
            !output.contains(content_to_ignore),
            "assistant text preceding a code reference should be ignored as this indicates licensed code is being returned"
        );
    }

    #[tokio::test]
    async fn test_response_parser_avoid_invalid_json() {
        let content_to_ignore = "IGNORE ME PLEASE";
        let tool_use_id = "TEST_ID".to_string();
        let tool_name = "execute_bash".to_string();
        let tool_args = serde_json::json!("invalid json").to_string();
        let mut events = vec![
            ChatResponseStream::AssistantResponseEvent {
                content: "hi".to_string(),
            },
            ChatResponseStream::AssistantResponseEvent {
                content: " there".to_string(),
            },
            ChatResponseStream::AssistantResponseEvent {
                content: content_to_ignore.to_string(),
            },
            ChatResponseStream::CodeReferenceEvent(()),
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: None,
                stop: None,
            },
            ChatResponseStream::ToolUseEvent {
                tool_use_id: tool_use_id.clone(),
                name: tool_name.clone(),
                input: Some(tool_args),
                stop: None,
            },
        ];
        events.reverse();
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        let mut output = String::new();
        let mut found_validation_error = false;
        for _ in 0..5 {
            match parser.recv().await {
                Ok(event) => {
                    output.push_str(&format!("{event:?}"));
                },
                Err(recv_error) => {
                    if matches!(recv_error.source, RecvErrorKind::ToolValidationError { .. }) {
                        found_validation_error = true;
                    }
                    break;
                },
            }
        }

        assert!(
            !output.contains(content_to_ignore),
            "assistant text preceding a code reference should be ignored as this indicates licensed code is being returned"
        );
        assert!(
            found_validation_error,
            "Expected to find tool validation error for non-object JSON"
        );
    }

    #[tokio::test]
    async fn test_response_parser_single_event_tool_use() {
        let tool_use_id = "TEST_ID".to_string();
        let tool_name = "thinking".to_string();
        let tool_args = serde_json::json!({
            "thought": "Let me think about this."
        })
        .to_string();
        let events = vec![ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: tool_name.clone(),
            input: Some(tool_args.clone()),
            stop: Some(true),
        }];
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        // First event should be ToolUseStart
        let event = parser.recv().await.unwrap();
        assert!(
            matches!(event, ResponseEvent::ToolUseStart { ref name } if name == "thinking"),
            "expected ToolUseStart, got {event:?}"
        );

        // Second event should be ToolUse with the correct args
        let event = parser.recv().await.unwrap();
        match event {
            ResponseEvent::ToolUse(tool_use) => {
                assert_eq!(tool_use.name, "thinking");
                assert_eq!(tool_use.args["thought"], "Let me think about this.");
            },
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    /// A clean stream end with no content events on the wire. The parser must surface this
    /// as `RecvErrorKind::EmptyResponse` so the chat loop can retry the request.
    #[tokio::test]
    async fn test_response_parser_empty_stream_produces_empty_response_error() {
        let mock = SendMessageOutput::Mock(vec![]);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        let result = parser.recv().await;
        assert!(
            matches!(
                result,
                Err(RecvError {
                    source: RecvErrorKind::EmptyResponse,
                    ..
                })
            ),
            "expected RecvErrorKind::EmptyResponse, got {result:?}"
        );
    }

    /// A stream that delivered any assistant text must complete normally, not as
    /// `EmptyResponse`. Retrying after the user already saw deltas would double-render content.
    #[tokio::test]
    async fn test_response_parser_text_response_is_not_empty() {
        let events = vec![ChatResponseStream::AssistantResponseEvent {
            content: "hello".to_string(),
        }];
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        let first = parser.recv().await;
        assert!(
            matches!(first, Ok(ResponseEvent::AssistantText(ref s)) if s == "hello"),
            "expected AssistantText(\"hello\"), got {first:?}"
        );
        let end = parser.recv().await;
        assert!(
            matches!(end, Ok(ResponseEvent::EndStream { .. })),
            "expected EndStream after content, got {end:?}"
        );
    }

    #[tokio::test]
    async fn test_response_parser_preserves_metadata_event_token_usage() {
        let mut events = vec![
            ChatResponseStream::AssistantResponseEvent {
                content: "hello".to_string(),
            },
            ChatResponseStream::MetadataEvent {
                total_tokens: Some(20),
                uncached_input_tokens: Some(10),
                output_tokens: Some(5),
                cache_read_input_tokens: Some(3),
                cache_write_input_tokens: Some(2),
            },
        ];
        events.reverse();
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "message".to_string(),
            Some("claude-4-sonnet".to_string()),
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        assert!(matches!(parser.recv().await, Ok(ResponseEvent::AssistantText(_))));
        let end = parser.recv().await;

        match end {
            Ok(ResponseEvent::EndStream { request_metadata, .. }) => {
                assert_eq!(request_metadata.total_tokens, Some(20));
                assert_eq!(request_metadata.uncached_input_tokens, Some(10));
                assert_eq!(request_metadata.output_tokens, Some(5));
                assert_eq!(request_metadata.cache_read_input_tokens, Some(3));
                assert_eq!(request_metadata.cache_write_input_tokens, Some(2));
            },
            other => panic!("expected EndStream with token metadata, got {other:?}"),
        }
    }

    /// A stream that delivered only thinking content must complete normally, not as
    /// `EmptyResponse`. Even though orphan-thinking blocks are dropped at finalization, the
    /// model did emit content on the wire and a retry would compound the wasted call.
    #[tokio::test]
    async fn test_response_parser_thinking_only_response_is_not_empty() {
        let events = vec![ChatResponseStream::ReasoningEvent {
            text: Some("let me think...".to_string()),
            signature: None,
            redacted_content: None,
        }];
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        // Drain until EndStream; assert we never see EmptyResponse.
        loop {
            match parser.recv().await {
                Ok(ResponseEvent::EndStream { .. }) => break,
                Ok(_) => continue,
                Err(err) => panic!("unexpected error from thinking-only stream: {err:?}"),
            }
        }
    }

    /// `AssistantResponseEvent` immediately followed by `CodeReferenceEvent` is the
    /// code-attribution-suppressed shape: the parser drops the assistant text but the model
    /// did emit content on the wire. Must complete normally, not as `EmptyResponse`.
    #[tokio::test]
    async fn test_response_parser_code_reference_suppression_is_not_empty() {
        // SendMessageOutput::Mock pops from the end, so the wire order is the reverse of
        // this vec: AssistantResponseEvent first, then CodeReferenceEvent. That sequence
        // hits the peek-discard branch in `recv` (assistant event followed by code-ref)
        // which is the path the regression fix targets.
        let mut events = vec![
            ChatResponseStream::AssistantResponseEvent {
                content: "this would be code".to_string(),
            },
            ChatResponseStream::CodeReferenceEvent(()),
        ];
        events.reverse();
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        loop {
            match parser.recv().await {
                Ok(ResponseEvent::EndStream { .. }) => break,
                Ok(_) => continue,
                Err(err) => panic!("unexpected error from code-attribution-suppressed stream: {err:?}"),
            }
        }
    }

    /// A turn with multiple sealed thinking blocks forwards the LAST sealed block into
    /// history (keep-last-sealed), not the first.
    #[tokio::test]
    async fn test_response_parser_keeps_last_sealed_thinking_block() {
        // Wire order (Mock pops from the end, so reverse below): block one's text, its
        // signature (seals it), block two's text, its signature (seals it), then the
        // assistant answer.
        let mut events = vec![
            ChatResponseStream::ReasoningEvent {
                text: Some("first block".to_string()),
                signature: None,
                redacted_content: None,
            },
            ChatResponseStream::ReasoningEvent {
                text: None,
                signature: Some("sig-1".to_string()),
                redacted_content: None,
            },
            ChatResponseStream::ReasoningEvent {
                text: Some("second block".to_string()),
                signature: None,
                redacted_content: None,
            },
            ChatResponseStream::ReasoningEvent {
                text: None,
                signature: Some("sig-2".to_string()),
                redacted_content: None,
            },
            ChatResponseStream::AssistantResponseEvent {
                content: "answer".to_string(),
            },
        ];
        events.reverse();
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        loop {
            match parser.recv().await {
                Ok(ResponseEvent::EndStream { message, .. }) => {
                    let thinking = message
                        .thinking()
                        .expect("the last sealed thinking block should be forwarded");
                    assert_eq!(
                        thinking.text, "second block",
                        "should keep the last sealed block, not the first"
                    );
                    assert_eq!(thinking.signature.as_deref(), Some("sig-2"));
                    break;
                },
                Ok(_) => continue,
                Err(err) => panic!("unexpected error from multi-block thinking stream: {err:?}"),
            }
        }
    }

    /// When the final thinking block never seals (e.g. a truncated stream), the previously
    /// sealed block is forwarded and the dangling unsealed block is dropped.
    #[tokio::test]
    async fn test_response_parser_drops_trailing_unsealed_thinking_block() {
        let mut events = vec![
            ChatResponseStream::ReasoningEvent {
                text: Some("sealed block".to_string()),
                signature: None,
                redacted_content: None,
            },
            ChatResponseStream::ReasoningEvent {
                text: None,
                signature: Some("sig-1".to_string()),
                redacted_content: None,
            },
            // A second block streams text but never receives a signature.
            ChatResponseStream::ReasoningEvent {
                text: Some("dangling unsealed".to_string()),
                signature: None,
                redacted_content: None,
            },
            ChatResponseStream::AssistantResponseEvent {
                content: "answer".to_string(),
            },
        ];
        events.reverse();
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        loop {
            match parser.recv().await {
                Ok(ResponseEvent::EndStream { message, .. }) => {
                    let thinking = message
                        .thinking()
                        .expect("the earlier sealed block should be forwarded, not the dangling one");
                    assert_eq!(thinking.text, "sealed block");
                    assert_eq!(thinking.signature.as_deref(), Some("sig-1"));
                    break;
                },
                Ok(_) => continue,
                Err(err) => panic!("unexpected error from trailing-orphan thinking stream: {err:?}"),
            }
        }
    }

    /// A redacted-only thinking block (encrypted reasoning: no visible text, no signature,
    /// but non-empty redacted content) is sealed and forwarded into history.
    #[tokio::test]
    async fn test_response_parser_forwards_redacted_only_thinking_block() {
        let mut events = vec![
            ChatResponseStream::ReasoningEvent {
                text: None,
                signature: None,
                redacted_content: Some(vec![0xde, 0xad, 0xbe, 0xef]),
            },
            ChatResponseStream::AssistantResponseEvent {
                content: "answer".to_string(),
            },
        ];
        events.reverse();
        let mock = SendMessageOutput::Mock(events);
        let mut parser = ResponseParser::new(
            mock,
            "".to_string(),
            None,
            1,
            vec![],
            mpsc::channel(32).0,
            Instant::now(),
            SystemTime::now(),
            CancellationToken::new(),
            Arc::new(Mutex::new(None)),
            Duration::ZERO,
        );

        loop {
            match parser.recv().await {
                Ok(ResponseEvent::EndStream { message, .. }) => {
                    let thinking = message
                        .thinking()
                        .expect("a redacted-only thinking block should be forwarded");
                    assert_eq!(thinking.redacted_content, vec![0xde, 0xad, 0xbe, 0xef]);
                    assert!(thinking.text.is_empty());
                    break;
                },
                Ok(_) => continue,
                Err(err) => panic!("unexpected error from redacted-only thinking stream: {err:?}"),
            }
        }
    }

    /// A slow SDK receive failure is a `SdkRecv`-sourced timeout, so the chat
    /// loop's watchdog-gated stall telemetry family must not fire for it.
    #[test]
    fn slow_sdk_recv_failure_is_not_watchdog_sourced() {
        let err = crate::api_client::ApiClientError::Other("connection reset by peer".to_string());
        match classify_recv_failure(err, Duration::from_secs(75)) {
            RecvErrorKind::StreamTimeout { timeout_source, .. } => {
                assert_eq!(timeout_source, agent::agent_loop::types::StreamTimeoutSource::SdkRecv);
            },
            other => panic!("expected a StreamTimeout classification, got {other:?}"),
        }
    }

    /// The pre-headers deadline error must read as a transient network failure
    /// so the bounded send retry applies instead of ending the turn.
    #[test]
    fn pre_headers_timeout_error_is_network_transient() {
        let err = pre_headers_timeout_error(Duration::from_secs(300));
        assert_eq!(
            err.transient_class(),
            Some(agent::error_recovery::TransientErrorClass::Network)
        );
    }
}
