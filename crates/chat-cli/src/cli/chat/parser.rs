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
use crate::api_client::model::{
    ChatResponseStream,
    ConversationState,
};
use crate::api_client::send_message_output::SendMessageOutput;
use crate::api_client::{
    ApiClient,
    ApiClientError,
};
use crate::telemetry::ReasonCode;
use crate::telemetry::core::{
    ChatConversationType,
    MessageMetaTag,
};

/// Error from sending a SendMessage request.
#[derive(Debug, Error)]
pub struct SendMessageError {
    #[source]
    pub source: ApiClientError,
    pub request_metadata: RequestMetadata,
}

impl SendMessageError {
    pub fn status_code(&self) -> Option<u16> {
        self.source.status_code()
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
            write!(f, "request_id: {}, error: ", request_id)?;
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
        }
    }
}

impl std::fmt::Display for RecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to receive the next message: ")?;
        if let Some(request_id) = self.request_metadata.request_id.as_ref() {
            write!(f, "request_id: {}, error: ", request_id)?;
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
        let response = client
            .send_message(conversation_state)
            .await
            .map_err(|err| SendMessageError {
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
    /// Whether or not we are currently receiving tool use delta events. Tuple of
    /// `Some((tool_use_id, name))` if true, [None] otherwise.
    parsing_tool_use: Option<(String, String)>,

    request_metadata: Arc<Mutex<Option<RequestMetadata>>>,
    cancel_token: CancellationToken,

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
            request_start_time,
            request_start_time_sys,
            received_response_size: 0,
            time_to_first_chunk: None,
            time_between_chunks: Vec::new(),
            request_metadata,
            cancel_token,
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
        if let Some((id, name)) = self.parsing_tool_use.take() {
            let tool_use = self.parse_tool_use(id, name).await?;
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
                        debug_assert!(input.is_none(), "Unexpected initial content in first tool use event");
                        debug_assert!(
                            stop.is_none_or(|v| !v),
                            "Unexpected immediate stop in first tool use event"
                        );
                        self.parsing_tool_use = Some((tool_use_id.clone(), name.clone()));
                        return Ok(ResponseEvent::ToolUseStart { name });
                    },
                    _ => {},
                },
                Ok(None) => {
                    let message_id = Some(self.message_id.clone());
                    let content = std::mem::take(&mut self.assistant_text);
                    let (message, conv_type) = if self.tool_uses.is_empty() {
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
    async fn parse_tool_use(&mut self, id: String, name: String) -> Result<AssistantToolUse, RecvError> {
        let mut tool_string = String::new();
        let start = Instant::now();
        while let Some(ChatResponseStream::ToolUseEvent { .. }) = self.peek().await? {
            if let Some(ChatResponseStream::ToolUseEvent { input, stop, .. }) = self.next().await? {
                if let Some(i) = input {
                    tool_string.push_str(&i);
                }
                if let Some(true) = stop {
                    break;
                }
            }
        }

        let args = match serde_json::from_str(&tool_string) {
            Ok(args) => args,
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
        let result = self.response.recv().await;
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
                        _ => {
                            warn!(?r, "received unexpected event from the response stream");
                        },
                    }
                }

                Ok(ev)
            },
            Err(err) => {
                error!(?err, "failed to receive the next event");
                if duration.as_secs() >= 59 {
                    Err(self.error(RecvErrorKind::StreamTimeout { source: err, duration }))
                } else {
                    Err(self.error(err))
                }
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

    fn make_metadata(&self, chat_conversation_type: Option<ChatConversationType>) -> RequestMetadata {
        RequestMetadata {
            request_id: self.response.request_id().map(String::from),
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
        }
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
}

/// Metadata about the sent request and associated response stream.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestMetadata {
    /// The request id associated with the [SendMessageOutput] stream.
    pub request_id: Option<String>,
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

    #[tokio::test]
    async fn test_parse() {
        // let _ = tracing_subscriber::fmt::try_init();
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
                content: "IGNORE ME PLEASE".to_string(),
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
        );

        for _ in 0..5 {
            println!("{:?}", parser.recv().await.unwrap());
        }
    }
}
