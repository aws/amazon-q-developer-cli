#![allow(dead_code)]

pub mod types;

use std::pin::Pin;
use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use agent::agent_loop::model::Model;
use agent::agent_loop::protocol::StreamResult;
use agent::agent_loop::types::{
    ContentBlock,
    ContentBlockDelta,
    ContentBlockDeltaEvent,
    ContentBlockStart,
    ContentBlockStartEvent,
    ContentBlockStopEvent,
    Message,
    MessageStartEvent,
    MessageStopEvent,
    MetadataEvent,
    MetadataMetrics,
    MetadataService,
    Role,
    StopReason,
    StreamError,
    StreamErrorKind,
    StreamErrorSource,
    StreamEvent,
    ToolResultContentBlock,
    ToolSpec,
    ToolUseBlockDelta,
    ToolUseBlockStart,
};
use chrono::{
    DateTime,
    Utc,
};
use eyre::Result;
use futures::Stream;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};
use uuid::Uuid;

use crate::api_client::error::{
    ApiClientError,
    ConverseStreamError,
    ConverseStreamErrorKind,
};
use crate::api_client::model::{
    ChatResponseStream,
    ConversationState,
    ToolSpecification,
    UserInputMessage,
    UserInputMessageContext,
};
use crate::api_client::send_message_output::SendMessageOutput;
use crate::api_client::{
    ApiClient,
    model as rts,
};
use crate::cli::chat::util::serde_value_to_document;

/// A [Model] implementation using the RTS backend.
#[derive(Debug, Clone)]
pub struct RtsModel {
    client: ApiClient,
    conversation_id: Uuid,
    model_id: Option<String>,
}

impl RtsModel {
    pub fn new(client: ApiClient, conversation_id: Uuid, model_id: Option<String>) -> Self {
        Self {
            client,
            conversation_id,
            model_id,
        }
    }

    pub fn conversation_id(&self) -> &Uuid {
        &self.conversation_id
    }

    pub fn model_id(&self) -> Option<&str> {
        self.model_id.as_deref()
    }

    async fn converse_stream_rts(
        self,
        tx: mpsc::Sender<StreamResult>,
        cancel_token: CancellationToken,
        messages: Vec<Message>,
        tool_specs: Option<Vec<ToolSpec>>,
        system_prompt: Option<String>,
    ) {
        let state = match self.make_conversation_state(messages, tool_specs, system_prompt) {
            Ok(s) => s,
            Err(msg) => {
                error!(?msg, "failed to create conversation state");
                tx.send(StreamResult::Err(StreamError::new(StreamErrorKind::Validation {
                    message: Some(msg),
                })))
                .await
                .map_err(|err| error!(?err, "failed to send model event"))
                .ok();
                return;
            },
        };

        let request_start_time = Instant::now();
        let request_start_time_sys = Utc::now();
        let token_clone = cancel_token.clone();
        let result = tokio::select! {
            _ = token_clone.cancelled() => {
                warn!("rts request cancelled during send");
                tx.send(StreamResult::Err(StreamError::new(StreamErrorKind::Interrupted)))
                    .await
                    .map_err(|err| (error!(?err, "failed to send event")))
                    .ok();
                return;
            },
            result = self.client.send_message(state) => {
                result
            }
        };
        self.handle_send_message_output(
            result,
            request_start_time.elapsed(),
            tx,
            cancel_token,
            request_start_time,
            request_start_time_sys,
        )
        .await;
    }

    async fn handle_send_message_output(
        &self,
        res: Result<SendMessageOutput, ConverseStreamError>,
        request_duration: Duration,
        tx: mpsc::Sender<StreamResult>,
        token: CancellationToken,
        request_start_time: Instant,
        request_start_time_sys: DateTime<Utc>,
    ) {
        match res {
            Ok(output) => {
                info!(?request_duration, "rts request sent successfully");
                let request_id = output.request_id().map(String::from);
                ResponseParser::new(
                    output,
                    tx,
                    token,
                    request_id,
                    request_start_time,
                    request_start_time_sys,
                )
                .consume_stream()
                .await;
            },
            Err(err) => {
                error!(?err, ?request_duration, "failed to send rts request");
                let kind = match err.kind {
                    ConverseStreamErrorKind::Throttling => StreamErrorKind::Throttling,
                    ConverseStreamErrorKind::MonthlyLimitReached => StreamErrorKind::Other(err.to_string()),
                    ConverseStreamErrorKind::ContextWindowOverflow => StreamErrorKind::ContextWindowOverflow,
                    ConverseStreamErrorKind::ModelOverloadedError => StreamErrorKind::Throttling,
                    ConverseStreamErrorKind::Unknown { .. } => StreamErrorKind::Other(err.to_string()),
                };
                let request_id = err.request_id.clone();
                tx.send(StreamResult::Err(
                    StreamError::new(kind)
                        .set_original_request_id(request_id)
                        .set_original_status_code(err.status_code)
                        .with_source(Arc::new(err)),
                ))
                .await
                .map_err(|err| error!(?err, "failed to send stream event"))
                .ok();
            },
        }
    }

    fn make_conversation_state(
        &self,
        mut messages: Vec<Message>,
        tool_specs: Option<Vec<ToolSpec>>,
        _system_prompt: Option<String>,
    ) -> Result<ConversationState, String> {
        debug!(?messages, ?tool_specs, "creating conversation state");
        let tools = tool_specs.map(|v| {
            v.into_iter()
                .map(Into::<ToolSpecification>::into)
                .map(Into::into)
                .collect()
        });

        // Creates the next user message to send.
        let user_input_message = match messages.pop() {
            Some(m) if m.role == Role::User => {
                let content = m.text();
                let (tool_results, images) = extract_tool_results_and_images(&m);
                let user_input_message_context = Some(UserInputMessageContext {
                    env_state: None,
                    git_state: None,
                    tool_results,
                    tools,
                });

                UserInputMessage {
                    content,
                    user_input_message_context,
                    user_intent: None,
                    images,
                    model_id: self.model_id.clone(),
                }
            },
            Some(m) => return Err(format!("Next message must be from the user, instead found: {}", m.role)),
            None => return Err("Empty conversation".to_string()),
        };

        let history = messages
            .into_iter()
            .map(|m| match m.role {
                Role::User => {
                    let content = m.text();
                    let (tool_results, _) = extract_tool_results_and_images(&m);
                    let ctx = if tool_results.is_some() {
                        Some(UserInputMessageContext {
                            env_state: None,
                            git_state: None,
                            tool_results,
                            tools: None,
                        })
                    } else {
                        None
                    };
                    let msg = UserInputMessage {
                        content,
                        user_input_message_context: ctx,
                        user_intent: None,
                        images: None,
                        model_id: None,
                    };
                    rts::ChatMessage::UserInputMessage(msg)
                },
                Role::Assistant => {
                    let msg = rts::AssistantResponseMessage {
                        message_id: m.id.clone(),
                        content: m.text(),
                        tool_uses: m.tool_uses().map(|v| v.into_iter().map(Into::into).collect()),
                    };
                    rts::ChatMessage::AssistantResponseMessage(msg)
                },
            })
            .collect();

        Ok(ConversationState {
            conversation_id: Some(self.conversation_id.to_string()),
            user_input_message,
            history: Some(history),
            agent_continuation_id: None,
        })
    }
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

/// Annoyingly, the RTS API doesn't allow images as tool use results, so we have to extract tool
/// results and image content separately.
fn extract_tool_results_and_images(message: &Message) -> (Option<Vec<rts::ToolResult>>, Option<Vec<rts::ImageBlock>>) {
    let mut images = Vec::new();
    let mut tool_results = Vec::new();
    for item in &message.content {
        match item {
            ContentBlock::ToolResult(block) => {
                let tool_use_id = block.tool_use_id.clone();
                let status = block.status.into();
                let mut content = Vec::new();
                for c in &block.content {
                    match c {
                        ToolResultContentBlock::Text(t) => content.push(rts::ToolResultContentBlock::Text(t.clone())),
                        ToolResultContentBlock::Json(v) => {
                            content.push(rts::ToolResultContentBlock::Json(serde_value_to_document(v.clone())));
                        },
                        ToolResultContentBlock::Image(img) => images.push(rts::ImageBlock {
                            format: img.format.into(),
                            source: img.source.clone().into(),
                        }),
                    }
                }
                tool_results.push(rts::ToolResult {
                    tool_use_id,
                    content,
                    status,
                });
            },
            ContentBlock::Image(img) => images.push(rts::ImageBlock {
                format: img.format.into(),
                source: img.source.clone().into(),
            }),
            _ => (),
        }
    }

    (
        if tool_results.is_empty() {
            None
        } else {
            Some(tool_results)
        },
        if images.is_empty() { None } else { Some(images) },
    )
}

impl Model for RtsModel {
    fn stream(
        &self,
        messages: Vec<Message>,
        tool_specs: Option<Vec<ToolSpec>>,
        system_prompt: Option<String>,
        cancel_token: CancellationToken,
    ) -> Pin<Box<dyn Stream<Item = StreamResult> + Send + 'static>> {
        let (tx, rx) = mpsc::channel(16);

        let self_clone = self.clone();
        let cancel_token_clone = cancel_token.clone();

        tokio::spawn(async move {
            self_clone
                .converse_stream_rts(tx, cancel_token_clone, messages, tool_specs, system_prompt)
                .await;
        });

        Box::pin(ReceiverStream::new(rx))
    }
}

/// Contains only the serializable data associated with [RtsModel].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtsModelState {
    pub conversation_id: Uuid,
    pub model_id: Option<String>,
}

impl RtsModelState {
    pub fn new() -> Self {
        Self {
            conversation_id: Uuid::new_v4(),
            model_id: None,
        }
    }
}

impl Default for RtsModelState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct ResponseParser {
    /// The response to consume and parse into a sequence of [StreamEvent].
    response: SendMessageOutput,
    event_tx: mpsc::Sender<StreamResult>,
    cancel_token: CancellationToken,

    /// Buffer that is continually written to during stream parsing.
    buf: Vec<StreamResult>,

    // parse state
    /// Whether or not the stream has completed.
    ended: bool,
    /// Buffer to hold the next event in [SendMessageOutput].
    ///
    /// Required since the RTS stream needs 1 look-ahead token to ensure we don't emit assistant
    /// response events that are immediately followed by a code reference event.
    peek: Option<ChatResponseStream>,
    /// Whether or not we have sent a [MessageStartEvent].
    message_start_pushed: bool,
    /// Whether or not we are currently receiving tool use delta events. Tuple of
    /// `Some((tool_use_id, name))` if true, [None] otherwise.
    parsing_tool_use: Option<(String, String)>,
    /// Whether or not the response stream contained at least one tool use.
    tool_use_seen: bool,

    // metadata fields
    request_id: Option<String>,
    /// Time immediately before sending the request.
    request_start_time: Instant,
    /// Time immediately before sending the request, as a [SystemTime].
    request_start_time_sys: DateTime<Utc>,
    time_to_first_chunk: Option<Duration>,
    time_between_chunks: Vec<Duration>,
    /// Total size (in bytes) of the response received so far.
    received_response_size: usize,
}

impl ResponseParser {
    fn new(
        response: SendMessageOutput,
        event_tx: mpsc::Sender<StreamResult>,
        cancel_token: CancellationToken,
        request_id: Option<String>,
        request_start_time: Instant,
        request_start_time_sys: DateTime<Utc>,
    ) -> Self {
        Self {
            response,
            event_tx,
            cancel_token,
            ended: false,
            peek: None,
            message_start_pushed: false,
            parsing_tool_use: None,
            tool_use_seen: false,
            buf: vec![],
            time_to_first_chunk: None,
            time_between_chunks: vec![],
            request_id,
            request_start_time,
            request_start_time_sys,
            received_response_size: 0,
        }
    }

    /// Consumes the entire response stream, emitting [StreamEvent] and [StreamError], or exiting
    /// early if [Self::cancel_token] is cancelled.
    ///
    /// In either case, metadata regarding the stream is emitted with a [StreamEvent::Metadata].
    async fn consume_stream(mut self) {
        loop {
            if self.ended {
                debug!("rts response stream has ended");
                return;
            }

            let token = self.cancel_token.clone();
            tokio::select! {
                _ = token.cancelled() => {
                    debug!("rts response parser was cancelled");
                    self.buf.push(StreamResult::Ok(self.make_metadata()));
                    self.buf.push(StreamResult::Err(StreamError::new(StreamErrorKind::Interrupted)));
                    self.drain_buf_events().await;
                    return;
                },
                res = self.fill_streamevent_buf() => {
                    match res {
                        Ok(_) => {
                            self.drain_buf_events().await;
                        },
                        Err(err) => {
                            self.buf.push(StreamResult::Ok(self.make_metadata()));
                            self.buf.push(StreamResult::Err(self.recv_error_to_stream_error(err)));
                            self.drain_buf_events().await;
                            return;
                        },
                    }
                }
            }
        }
    }

    async fn drain_buf_events(&mut self) {
        for ev in self.buf.drain(..) {
            self.event_tx
                .send(ev)
                .await
                .map_err(|err| error!(?err, "failed to send event to channel"))
                .ok();
        }
    }

    /// Consumes the next token(s) in the response stream, filling [Self::buf] with the stream
    /// events to be emitted, sequentially.
    ///
    /// We only consume the stream in parts in order to ensure we exit in a timely manner if
    /// [Self::cancel_token] is cancelled.
    async fn fill_streamevent_buf(&mut self) -> Result<(), RecvError> {
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
                    self.buf.push(StreamResult::Ok(StreamEvent::ContentBlockDelta(
                        ContentBlockDeltaEvent {
                            delta: ContentBlockDelta::Text(content),
                            content_block_index: None,
                        },
                    )));
                },
            }
        }

        loop {
            match self.next().await? {
                Some(ev) => match ev {
                    ChatResponseStream::AssistantResponseEvent { content } => {
                        self.buf.push(StreamResult::Ok(StreamEvent::ContentBlockDelta(
                            ContentBlockDeltaEvent {
                                delta: ContentBlockDelta::Text(content),
                                content_block_index: None,
                            },
                        )));
                        return Ok(());
                    },
                    ChatResponseStream::ToolUseEvent {
                        tool_use_id,
                        name,
                        input,
                        stop,
                    } => {
                        self.tool_use_seen = true;
                        if self.parsing_tool_use.is_none() {
                            self.parsing_tool_use = Some((tool_use_id.clone(), name.clone()));
                            self.buf.push(StreamResult::Ok(StreamEvent::ContentBlockStart(
                                ContentBlockStartEvent {
                                    content_block_start: Some(ContentBlockStart::ToolUse(ToolUseBlockStart {
                                        tool_use_id,
                                        name,
                                    })),
                                    content_block_index: None,
                                },
                            )));
                        }
                        if let Some(input) = input {
                            self.buf.push(StreamResult::Ok(StreamEvent::ContentBlockDelta(
                                ContentBlockDeltaEvent {
                                    delta: ContentBlockDelta::ToolUse(ToolUseBlockDelta { input }),
                                    content_block_index: None,
                                },
                            )));
                        }
                        if let Some(true) = stop {
                            self.buf
                                .push(StreamResult::Ok(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                                    content_block_index: None,
                                })));
                            self.parsing_tool_use = None;
                        }
                        return Ok(());
                    },
                    other => {
                        warn!(?other, "received unexpected rts event");
                    },
                },
                None => {
                    self.ended = true;
                    self.buf
                        .push(StreamResult::Ok(StreamEvent::MessageStop(MessageStopEvent {
                            stop_reason: if self.tool_use_seen {
                                StopReason::ToolUse
                            } else {
                                StopReason::EndTurn
                            },
                        })));
                    self.buf.push(StreamResult::Ok(self.make_metadata()));
                    return Ok(());
                },
            }
        }
    }

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

    async fn next(&mut self) -> Result<Option<ChatResponseStream>, RecvError> {
        if let Some(ev) = self.peek.take() {
            return Ok(Some(ev));
        }

        trace!("Attempting to recv next event");
        let start = Instant::now();
        let result = self.response.recv().await;
        let duration = Instant::now().duration_since(start);
        match result {
            Ok(ev) => {
                trace!(?ev, "Received new event");

                if !self.message_start_pushed {
                    self.buf
                        .push(StreamResult::Ok(StreamEvent::MessageStart(MessageStartEvent {
                            role: Role::Assistant,
                        })));
                    self.message_start_pushed = true;
                }

                // Track metadata about the chunk.
                self.time_to_first_chunk
                    .get_or_insert_with(|| self.request_start_time.elapsed());
                self.time_between_chunks.push(duration);
                self.received_response_size += ev.as_ref().map(|e| e.len()).unwrap_or_default();

                Ok(ev)
            },
            Err(err) => {
                error!(?err, "failed to receive the next event");
                if duration.as_secs() >= 59 {
                    Err(RecvError::Timeout { source: err, duration })
                } else {
                    Err(RecvError::Other { source: err })
                }
            },
        }
    }

    fn recv_error_to_stream_error(&self, err: RecvError) -> StreamError {
        match err {
            RecvError::Timeout { source, duration } => StreamError::new(StreamErrorKind::StreamTimeout { duration })
                .set_original_request_id(self.request_id.clone())
                .with_source(Arc::new(source)),
            RecvError::Other { source } => StreamError::new(StreamErrorKind::Other(format!(
                "An unexpected error occurred during the response stream: {source:?}"
            )))
            .set_original_request_id(self.request_id.clone())
            .with_source(Arc::new(source)),
        }
    }

    fn make_metadata(&self) -> StreamEvent {
        StreamEvent::Metadata(MetadataEvent {
            metrics: Some(MetadataMetrics {
                request_start_time: self.request_start_time_sys,
                request_end_time: Utc::now(),
                time_to_first_chunk: self.time_to_first_chunk,
                time_between_chunks: if self.time_between_chunks.is_empty() {
                    None
                } else {
                    Some(self.time_between_chunks.clone())
                },
                response_stream_len: self.received_response_size as u32,
            }),
            // if only rts gave usage metrics...
            usage: None,
            service: Some(MetadataService {
                request_id: self.response.request_id().map(String::from),
                status_code: None,
            }),
        })
    }
}

#[derive(Debug)]
enum RecvError {
    Timeout { source: ApiClientError, duration: Duration },
    Other { source: ApiClientError },
}

#[cfg(test)]
mod tests {
    use tokio_stream::StreamExt as _;

    use super::*;
    use crate::database::Database;
    use crate::os::{
        Env,
        Fs,
    };
    use crate::util::env_var::is_integ_test;

    /// Manual test to verify cancellation succeeds in a timely manner.
    #[tokio::test]
    async fn integ_test_rts_cancel() {
        if !is_integ_test() {
            return;
        }

        let rts = RtsModel::new(
            ApiClient::new(&Env::new(), &Fs::new(), &mut Database::new().await.unwrap(), None)
                .await
                .unwrap(),
            Uuid::new_v4(),
            None,
        );
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            let mut stream = rts.stream(
                vec![Message::new(
                    Role::User,
                    vec![ContentBlock::Text(
                        "Hello, can you explain how to write hello world in c, python, and rust?".to_string(),
                    )],
                    None,
                )],
                None,
                None,
                token_clone,
            );
            while let Some(ev) = stream.next().await {
                let _ = tx.send(ev).await;
            }
        });

        // Assertion logic here is:
        // 1. Loop until we start receiving content
        // 2. Once content is received, cancel the stream
        // 3. Assert that we receive a metadata stream event, and then immediately followed by an
        //    Interrupted error. These events should be received almost immediately after cancelling.
        let mut was_cancelled = false;
        let mut cancelled_time = None;
        loop {
            let ev = rx.recv().await.expect("should not fail");
            if let StreamResult::Ok(StreamEvent::ContentBlockDelta(_)) = ev {
                if was_cancelled {
                    continue;
                }
                // We received content, so time to interrupt the stream.
                cancel_token.cancel();
                was_cancelled = true;
                cancelled_time = Some(Instant::now());
            }
            if let StreamResult::Ok(StreamEvent::Metadata(_)) = ev {
                // Next event should be an interrupted error.
                let ev = rx.recv().await.expect("should have another event after metadata");
                let err = ev.unwrap_err();
                assert!(matches!(err.kind, StreamErrorKind::Interrupted));
                let elapsed = cancelled_time.unwrap().elapsed();
                assert!(
                    elapsed.as_millis() < 25,
                    "stream should have been interrupted in a timely manner, instead took: {}ms",
                    elapsed.as_millis()
                );
                break;
            }
        }
        if !was_cancelled {
            panic!("stream was never cancelled");
        }
    }

    #[test]
    fn test_other_stream_err_downcasting() {
        let err = StreamError::new(StreamErrorKind::Interrupted).with_source(Arc::new(ConverseStreamError::new(
            ConverseStreamErrorKind::ModelOverloadedError,
            None::<aws_smithy_types::error::operation::BuildError>, /* annoying type inference
                                                                     * required */
        )));
        assert!(
            err.as_concrete_error::<ConverseStreamError>()
                .is_some_and(|r| matches!(r.kind, ConverseStreamErrorKind::ModelOverloadedError))
        );
    }
}
