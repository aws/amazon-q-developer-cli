pub mod model;
pub mod protocol;
pub mod types;

use std::pin::Pin;
use std::str::FromStr as _;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use eyre::Result;
use futures::{
    Stream,
    StreamExt,
};
use model::Model;
use protocol::{
    AgentLoopEventKind,
    AgentLoopRequest,
    AgentLoopResponse,
    AgentLoopResponseError,
    LoopEndReason,
    LoopError,
    SendRequestArgs,
    StreamMetadata,
    StreamResult,
    UserTurnMetadata,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};
use types::{
    ContentBlock,
    Message,
    MessageStartEvent,
    MessageStopEvent,
    MetadataEvent,
    Role,
    StreamError,
    StreamErrorKind,
    StreamEvent,
    ToolUseBlock,
};
use uuid::Uuid;

use super::tools::BuiltInToolName;
use crate::agent::AgentId;
use crate::agent::util::request_channel::{
    RequestReceiver,
    RequestSender,
    new_request_channel,
    respond,
};
use crate::detect_invariant_violations;

/// Identifier for an instance of an executing loop. Derived from an agent id and some unique
/// identifier.
///
/// This type enables us to differentiate user turns for the same agent, while also allowing us to
/// ensure that only a single turn executes for an agent at any given time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLoopId {
    /// Id of the agent
    agent_id: AgentId,
    /// Random identifier
    rand: u32,
}

impl AgentLoopId {
    pub fn new(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            rand: rand::random::<u32>(),
        }
    }
}

impl std::fmt::Display for AgentLoopId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.agent_id, self.rand)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum LoopState {
    #[default]
    Idle,
    /// A request is currently being sent to the model.
    ///
    /// The loop is unable to handle new requests while in this state.
    SendingRequest,
    /// A model response is currently being consumed.
    ///
    /// The loop is unable to handle new requests while in this state.
    ConsumingResponse,
    /// The loop is waiting for tool use result(s) to be provided.
    PendingToolUseResults,
    /// The agent loop has completed all processing, and no pending work is left to do.
    ///
    /// This is generally the final state of the loop. If another request is sent, then the user
    /// turn will be continued for another cycle.
    UserTurnEnded,
    /// An error occurred that requires manual intervention.
    Errored,
}

/// Tracks the execution of a user turn, ending when either the model returns a response with no
/// tool uses, or a non-retryable error is encountered.
pub struct AgentLoop {
    /// Identifier for the loop.
    id: AgentLoopId,

    /// Current state of the loop
    execution_state: LoopState,

    /// Cancellation token used for gracefully cancelling the underlying response stream
    cancel_token: CancellationToken,

    /// The current response stream future being received
    curr_stream: Option<Pin<Box<dyn Stream<Item = StreamResult> + Send>>>,

    /// Parse state for the current stream (separate so cancel handler can access it)
    curr_stream_state: Option<StreamParseState>,

    /// List of completed stream parse states
    stream_states: Vec<StreamParseState>,

    // turn duration tracking
    loop_start_time: Option<Instant>,
    loop_end_time: Option<Instant>,

    loop_event_tx: mpsc::Sender<AgentLoopEventKind>,
    loop_req_rx: RequestReceiver<AgentLoopRequest, AgentLoopResponse, AgentLoopResponseError>,
    /// Only used in [Self::spawn]
    loop_event_rx: Option<mpsc::Receiver<AgentLoopEventKind>>,
    /// Only used in [Self::spawn]
    loop_req_tx: Option<RequestSender<AgentLoopRequest, AgentLoopResponse, AgentLoopResponseError>>,
}

impl std::fmt::Debug for AgentLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentLoop")
            .field("id", &self.id)
            .field("execution_state", &self.execution_state)
            .field("curr_stream_state", &self.curr_stream_state)
            .field("stream_states", &self.stream_states)
            .finish()
    }
}

impl AgentLoop {
    pub fn new(id: AgentLoopId, cancel_token: CancellationToken) -> Self {
        let (loop_event_tx, loop_event_rx) = mpsc::channel(16);
        let (loop_req_tx, loop_req_rx) = new_request_channel();
        Self {
            id,
            execution_state: LoopState::Idle,
            cancel_token,
            curr_stream: None,
            curr_stream_state: None,
            stream_states: Vec::new(),
            loop_start_time: None,
            loop_end_time: None,
            loop_event_tx,
            loop_event_rx: Some(loop_event_rx),
            loop_req_tx: Some(loop_req_tx),
            loop_req_rx,
        }
    }

    /// Spawns a new task for executing the agent loop, returning a handle for sending messages to
    /// the spawned task.
    pub fn spawn(mut self) -> AgentLoopHandle {
        let id_clone = self.id.clone();
        let loop_event_rx = self.loop_event_rx.take().expect("loop_event_rx should exist");
        let loop_req_tx = self.loop_req_tx.take().expect("loop_req_tx should exist");
        let handle = tokio::spawn(async move {
            info!("agent loop start");
            self.main_loop().await;
            info!("agent loop end");
        });
        AgentLoopHandle::new(id_clone, loop_req_tx, loop_event_rx, handle)
    }

    async fn main_loop(mut self) {
        loop {
            tokio::select! {
                // Branch for handling agent loop messages
                req = self.loop_req_rx.recv() => {
                    let Some(req) = req else {
                        warn!("Agent loop request channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_agent_loop_request(req.payload).await;
                    respond!(req, res);
                },

                // Branch for handling the next stream event.
                res = async {
                    match self.curr_stream.as_mut() {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    debug!(?self.id, ?res, "agent loop received stream event");

                    // Buffer for the stream parser to update with events to send
                    let mut loop_events: Vec<AgentLoopEventKind> = Vec::new();

                    // Advance the stream parse state
                    let stream_state = self.curr_stream_state.as_mut().expect("curr_stream_state should exist when curr_stream exists");
                    stream_state.next(res, &mut loop_events);

                    if stream_state.ended() {
                        // Stream ended, clean up
                        self.curr_stream = None;
                        let stream_state = self.curr_stream_state.take().unwrap();
                        self.stream_states.push(stream_state);
                        let stream_state = self.stream_states.last().expect("should exist after push");

                        if stream_state.errored {
                            // For errors, don't end the loop - wait for a retry request or a close request.
                            loop_events.push(self.set_execution_state(LoopState::Errored));
                        } else if stream_state.has_tool_uses() {
                            loop_events.push(self.set_execution_state(LoopState::PendingToolUseResults));
                        } else {
                            // For successful streams with no tool uses, this always ends a user turn.
                            loop_events.push(self.set_execution_state(LoopState::UserTurnEnded));
                            self.loop_end_time = Some(Instant::now());
                            loop_events.push(AgentLoopEventKind::UserTurnEnd(self.make_user_turn_metadata()));
                        }
                    }

                    // Send agent loop events back from the parsed state so far
                    for ev in loop_events.drain(..) {
                        self.loop_event_tx.send(ev).await.ok();
                    }
                }
            }
        }
    }

    async fn handle_agent_loop_request(
        &mut self,
        req: AgentLoopRequest,
    ) -> Result<AgentLoopResponse, AgentLoopResponseError> {
        debug!(?req, "agent loop handling new request");
        match req {
            AgentLoopRequest::GetExecutionState => Ok(AgentLoopResponse::ExecutionState(self.execution_state)),
            AgentLoopRequest::SendRequest { model, args } => {
                let violations = detect_invariant_violations(&args.messages);
                if !violations.is_valid() {
                    error!("unexpected conversation history invariant violation: {:#?}", violations);
                    return Err(AgentLoopResponseError::Custom(
                        "invalid conversation history received".to_string(),
                    ));
                }
                if !args.messages.last().is_some_and(|m| m.role == Role::User) {
                    error!(
                        "expected last message to be from the user, instead found: {:?}",
                        args.messages.last()
                    );
                    return Err(AgentLoopResponseError::Custom(
                        "expected last message to be from the user".to_string(),
                    ));
                }

                if self.curr_stream.is_some() {
                    return Err(AgentLoopResponseError::StreamCurrentlyExecuting);
                }

                // Ensure we are in a state that can handle a new request.
                match self.execution_state {
                    LoopState::Idle | LoopState::Errored | LoopState::PendingToolUseResults => {},
                    LoopState::UserTurnEnded => {},
                    other => {
                        error!(
                            ?other,
                            "Agent loop is in an unexpected state while the stream is none: {:?}", other
                        );
                        return Err(AgentLoopResponseError::StreamCurrentlyExecuting);
                    },
                }

                // Send the request, creating a new stream parse state for handling the response.

                self.loop_start_time = Some(self.loop_start_time.unwrap_or(Instant::now()));
                let state_change = self.set_execution_state(LoopState::SendingRequest);
                let _ = self.loop_event_tx.send(state_change).await;

                let next_user_message = args
                    .messages
                    .last()
                    .ok_or(AgentLoopResponseError::Custom(
                        "a user message must exist in order to send requests".to_string(),
                    ))?
                    .clone();

                let cancel_token = self.cancel_token.clone();
                let stream = model.stream(args.messages, args.tool_specs, args.system_prompt, cancel_token);
                self.curr_stream = Some(stream);
                self.curr_stream_state = Some(StreamParseState::new(next_user_message, model.model_id()));
                Ok(AgentLoopResponse::Success)
            },

            AgentLoopRequest::Cancel => {
                // Always cancel the token first - this will cause RTS to emit Interrupted
                self.cancel_token.cancel();

                let mut buf = Vec::new();

                // Drain the stream only if we have it (stream branch doesn't)
                if let Some(mut stream) = self.curr_stream.take() {
                    while let Some(ev) = stream.next().await {
                        if let Some(parse_state) = self.curr_stream_state.as_mut() {
                            parse_state.next(Some(ev), &mut buf);
                        }
                    }
                }
                // If stream branch has curr_stream, it will be dropped when select! cancels it

                // Finalize the parse state if it exists
                if let Some(mut parse_state) = self.curr_stream_state.take() {
                    parse_state.next(None, &mut buf);
                    self.stream_states.push(parse_state);
                }

                self.loop_end_time = Some(Instant::now());
                let metadata = self.make_user_turn_metadata();
                buf.push(self.set_execution_state(LoopState::UserTurnEnded));
                buf.push(AgentLoopEventKind::UserTurnEnd(metadata));

                for ev in buf.drain(..) {
                    self.loop_event_tx.send(ev).await.ok();
                }

                Ok(AgentLoopResponse::Success)
            },
        }
    }

    fn set_execution_state(&mut self, to: LoopState) -> AgentLoopEventKind {
        let from = self.execution_state;
        self.execution_state = to;
        AgentLoopEventKind::LoopStateChange { from, to }
    }

    /// Creates the user turn metadata.
    ///
    /// This should only be called after all completed stream parse states have been pushed to
    /// [Self::stream_states].
    fn make_user_turn_metadata(&self) -> UserTurnMetadata {
        debug_assert!(self.stream_states.iter().all(|s| s.ended()));
        debug_assert!(self.curr_stream.is_none());

        let mut message_ids = Vec::new();
        let mut number_of_cycles = 0_u32;
        let mut builtin_tool_uses = 0_u32;
        let mut input_token_count = 0_u32;
        let mut output_token_count = 0_u32;
        let mut cache_read_input_token_count = 0_u32;
        let mut cache_write_input_token_count = 0_u32;
        let mut model = None;
        let mut assistant_response_length = 0_usize;
        let mut request_attempts = None::<u32>;
        let mut context_usage_percentage = None;
        let mut metering_usage = Vec::new();

        for s in &self.stream_states {
            message_ids.push(s.user_message.id.clone());
            message_ids.push(s.message_id.clone());
            if s.model_id.is_some() {
                model.clone_from(&s.model_id);
            }
            assistant_response_length = assistant_response_length.saturating_add(s.assistant_text.len());
            if let Some(attempts) = s.request_attempts {
                request_attempts = Some(request_attempts.unwrap_or(0).saturating_add(attempts));
            }

            if s.has_tool_uses() {
                number_of_cycles = number_of_cycles.saturating_add(1);
                builtin_tool_uses = builtin_tool_uses.saturating_add(s.builtin_tool_uses());
            }

            if let Some(md) = s.metadata.as_ref()
                && let Some(md_usage) = md.usage.as_ref()
            {
                if let Some(token_count) = md_usage.input_tokens.as_ref() {
                    input_token_count = input_token_count.saturating_add(*token_count);
                }
                if let Some(token_count) = md_usage.output_tokens.as_ref() {
                    output_token_count = output_token_count.saturating_add(*token_count);
                }
                if let Some(token_count) = md_usage.cache_read_input_tokens.as_ref() {
                    cache_read_input_token_count = cache_read_input_token_count.saturating_add(*token_count);
                }
                if let Some(token_count) = md_usage.cache_write_input_tokens.as_ref() {
                    cache_write_input_token_count = cache_write_input_token_count.saturating_add(*token_count);
                }
                if let Some(percent) = md_usage.context_usage_percentage {
                    context_usage_percentage = Some(percent);
                }
            }
            if let Some(md) = s.metadata.as_ref() {
                metering_usage.extend(md.metering_usage.iter().cloned());
            }
        }

        let user_prompt_length = self.stream_states.first().map_or(0, |s| s.user_message.text().len());

        UserTurnMetadata {
            loop_id: self.id.clone(),
            result: self.stream_states.last().map(|s| s.make_result()),
            message_ids,
            total_request_count: self.stream_states.len() as u32,
            number_of_cycles,
            builtin_tool_uses,
            turn_duration: match (self.loop_start_time, self.loop_end_time) {
                (Some(start), Some(end)) => Some(end.duration_since(start)),
                _ => None,
            },
            end_reason: self.stream_states.last().map_or(LoopEndReason::DidNotRun, |s| {
                if s.interrupted() {
                    LoopEndReason::Cancelled
                } else if s.errored() {
                    LoopEndReason::Error
                } else if s.has_tool_uses() {
                    LoopEndReason::ToolUseRejected
                } else {
                    LoopEndReason::UserTurnEnd
                }
            }),
            end_timestamp: Utc::now(),
            input_token_count,
            output_token_count,
            cache_read_input_token_count,
            cache_write_input_token_count,
            model,
            assistant_response_length,
            request_attempts,
            context_usage_percentage,
            metering_usage,
            user_prompt_length,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidToolUse {
    pub tool_use_id: String,
    pub name: String,
    pub content: String,
}

/// State associated with parsing a stream of [StreamResult] into
/// [AgentLoopEventKind].
#[derive(Debug)]
struct StreamParseState {
    /// The next user message that was sent for this request
    user_message: Message,

    /// Tool uses returned by the response stream.
    tool_uses: Vec<ToolUseBlock>,
    /// Invalid tool uses returned by the response stream.
    ///
    /// If this is non-empty, then [Self::errored] would be true.
    invalid_tool_uses: Vec<InvalidToolUse>,

    /// Generated message id on a successful response stream end
    message_id: Option<String>,

    // mid-stream parse state
    /// Received assistant text
    assistant_text: String,
    /// Whether or not we are currently receiving tool use delta events. Tuple of
    /// `Some((tool_use_id, name, buf))` if true, [None] otherwise.
    parsing_tool_use: Option<(String, String, String)>,
    /// Whether we are currently receiving thinking delta events.
    parsing_thinking: Option<String>,
    /// Accumulated thinking blocks for this turn.
    thinking_blocks: Vec<types::ThinkingBlock>,
    /// Pending signature from a ReasoningEvent (set before ContentBlockStop).
    pending_signature: Option<String>,
    /// Pending redacted content from a ReasoningEvent.
    pending_redacted_content: Option<Vec<u8>>,
    /// Model ID that generated this response (used to tag thinking blocks).
    model_id: Option<String>,
    /// Buffered metadata event returned from the response stream
    metadata: Option<MetadataEvent>,
    /// Buffered message start event returned from the response stream
    message_start: Option<MessageStartEvent>,
    /// Buffered message stop event returned from the response stream
    message_stop: Option<MessageStopEvent>,
    /// Buffered error event returned from the response stream
    stream_err: Option<StreamError>,

    /// Number of HTTP-level attempts reported by the transport layer, if any.
    request_attempts: Option<u32>,

    ended_time: Option<Instant>,
    /// Whether or not the stream encountered an error.
    ///
    /// Once an error has occurred, no new events can be received
    errored: bool,
    /// Whether the stream ever carried any content event (text delta, thinking delta, or
    /// tool-use start). If this stays false through end-of-stream, the response is empty
    /// in the literal "nothing on the wire" sense and is safe to silently retry. If any
    /// content event was seen, even one later dropped during finalization (e.g. orphan
    /// thinking blocks), the user already saw something and the response must not be
    /// retried.
    received_content_event: bool,
}

impl StreamParseState {
    pub fn new(user_message: Message, model_id: Option<String>) -> Self {
        Self {
            assistant_text: String::new(),
            parsing_tool_use: None,
            parsing_thinking: None,
            thinking_blocks: Vec::new(),
            pending_signature: None,
            pending_redacted_content: None,
            model_id,
            tool_uses: Vec::new(),
            invalid_tool_uses: Vec::new(),
            user_message,
            message_id: None,
            metadata: None,
            message_start: None,
            message_stop: None,
            stream_err: None,
            request_attempts: None,
            ended_time: None,
            errored: false,
            received_content_event: false,
        }
    }

    pub fn next(&mut self, ev: Option<StreamResult>, buf: &mut Vec<AgentLoopEventKind>) {
        let Some(ev) = ev else {
            // No event received means the stream has ended.
            debug_assert!(
                self.ended_time.is_none(),
                "unexpected call to next after stream has already ended"
            );

            // If we were mid-parse on a tool use, the stream ended without a ContentBlockStop.
            // Treat it as an invalid tool use so the retry logic can ask the model to split up.
            if let Some((tool_use_id, name, content)) = self.parsing_tool_use.take() {
                warn!(
                    tool_use_id,
                    name, "stream ended with incomplete tool use (no ContentBlockStop received)"
                );
                self.invalid_tool_uses.push(InvalidToolUse {
                    tool_use_id,
                    name,
                    content,
                });
            }

            self.ended_time = Some(self.ended_time.unwrap_or(Instant::now()));
            self.errored = self.errored || !self.invalid_tool_uses.is_empty() || self.is_empty_response();
            let result = self.make_result();
            self.message_id = result.as_ref().map(|r| r.id.clone()).ok().flatten();
            buf.push(AgentLoopEventKind::ResponseStreamEnd {
                result,
                metadata: self.make_stream_metadata(),
            });
            return;
        };

        if self.errored {
            warn!(?ev, "ignoring unexpected event after having received an error");
            return;
        }

        // Debug assertion: the first event must be MessageStart, Metadata, or an error.
        // Metadata can arrive first when a request is cancelled before the backend sends
        // any message content (RTS emits Metadata then Interrupted on cancel).
        match &ev {
            StreamResult::Ok(
                StreamEvent::MessageStart(_)
                | StreamEvent::Metadata(_)
                | StreamEvent::RetryWarning(_)
                | StreamEvent::RequestAttempts(_),
            )
            | StreamResult::Err(_) => (),
            other @ StreamResult::Ok(_) => debug_assert!(
                self.message_start.is_some(),
                "received an unexpected event at the start of the response stream: {other:?}"
            ),
        }

        // Pushing low-level stream events in case end users want to consume these directly. Likely
        // not required.
        buf.push(AgentLoopEventKind::Stream(ev.clone()));

        match ev {
            StreamResult::Ok(s) => match s {
                StreamEvent::MessageStart(ev) => {
                    debug_assert!(self.message_start.is_none());
                    debug_assert!(ev.role == Role::Assistant);
                    self.message_start = Some(ev);
                },
                StreamEvent::MessageStop(ev) => {
                    debug_assert!(self.message_stop.is_none());
                    self.message_stop = Some(ev);
                },

                StreamEvent::ContentBlockStart(ev) => {
                    self.received_content_event = true;
                    if let Some(start) = ev.content_block_start {
                        match start {
                            types::ContentBlockStart::ToolUse(v) => {
                                self.parsing_tool_use = Some((v.tool_use_id.clone(), v.name.clone(), String::new()));
                                buf.push(AgentLoopEventKind::ToolUseStart {
                                    id: v.tool_use_id,
                                    name: v.name,
                                });
                            },
                            types::ContentBlockStart::Thinking => {
                                self.parsing_thinking = Some(String::new());
                            },
                        }
                    }
                },

                StreamEvent::ContentBlockDelta(ev) => {
                    self.received_content_event = true;
                    match ev.delta {
                        types::ContentBlockDelta::Text(text) => {
                            self.assistant_text.push_str(&text);
                            buf.push(AgentLoopEventKind::AssistantText(text));
                        },
                        types::ContentBlockDelta::ToolUse(ev) => {
                            debug_assert!(self.parsing_tool_use.is_some());
                            match self.parsing_tool_use.as_mut() {
                                Some((_, _, buf)) => {
                                    buf.push_str(&ev.input);
                                },
                                None => {
                                    warn!(?ev, "received a tool use delta with no corresponding tool use");
                                },
                            }
                        },
                        types::ContentBlockDelta::Reasoning(text) => {
                            if let Some(thinking_buf) = self.parsing_thinking.as_mut() {
                                thinking_buf.push_str(&text);
                                buf.push(AgentLoopEventKind::ThinkingText(text));
                            }
                        },
                        types::ContentBlockDelta::ReasoningSignature {
                            signature,
                            redacted_content,
                        } => {
                            self.pending_signature = signature;
                            self.pending_redacted_content = redacted_content;
                        },
                        types::ContentBlockDelta::Document => (),
                    }
                },

                StreamEvent::ContentBlockStop(_) => {
                    if let Some(thinking_text) = self.parsing_thinking.take() {
                        let signature = self.pending_signature.take();
                        let redacted_content = self.pending_redacted_content.take().unwrap_or_default();
                        // Skip orphan thinking blocks: no signature AND no redacted content.
                        // Bedrock's API rejects history that contains a thinking block missing
                        // both fields ("messages.N.content.0.thinking.signature: Field required"),
                        // and once one lands in conversation history every subsequent turn
                        // replays it and fails — bricking long sessions until /compact, /rewind,
                        // or /chat new. Orphans can occur when the upstream stream truncates
                        // mid-thinking-block: the RTS parser synthesizes a ContentBlockStop at
                        // end-of-stream while in_thinking, but no ReasoningSignature delta ever
                        // arrived. Drop the thinking text rather than poison history; the
                        // assistant's spoken text and tool calls are preserved separately.
                        if signature.is_none() && redacted_content.is_empty() {
                            warn!(
                                len = thinking_text.len(),
                                "dropping orphan thinking block: no signature or redacted content received"
                            );
                        } else {
                            self.thinking_blocks.push(types::ThinkingBlock {
                                text: thinking_text,
                                signature,
                                redacted_content,
                                model_id: self.model_id.clone(),
                            });
                        }
                    } else if let Some((tool_use_id, name, tool_content)) = self.parsing_tool_use.take() {
                        // Defensively clear any stale signature state from protocol violations.
                        self.pending_signature.take();
                        self.pending_redacted_content.take();
                        // Empty content → `{}`. Matches V1 parser behaviour and handles
                        // zero-arg tool uses where the model emits "" instead of "{}".
                        let parsed = if tool_content.is_empty() {
                            Ok(serde_json::json!({}))
                        } else {
                            serde_json::from_str::<serde_json::Value>(&tool_content)
                        };
                        match parsed {
                            Ok(val) => {
                                let tool_use = ToolUseBlock {
                                    tool_use_id,
                                    name,
                                    input: val,
                                };
                                buf.push(AgentLoopEventKind::ToolUse(tool_use.clone()));
                                self.tool_uses.push(tool_use);
                            },
                            Err(err) => {
                                error!(?err, "received an invalid tool use from the response stream");
                                self.invalid_tool_uses.push(InvalidToolUse {
                                    tool_use_id,
                                    name,
                                    content: tool_content,
                                });
                            },
                        }
                    }
                },

                StreamEvent::Metadata(ev) => {
                    debug_assert!(
                        self.metadata.is_none(),
                        "Only one metadata event is expected. Previously found: {:?}, just received: {:?}",
                        self.metadata,
                        ev
                    );
                    self.metadata = Some(ev);
                },

                StreamEvent::RetryWarning(_) => {
                    // Already pushed to buf as AgentLoopEventKind::Stream above.
                    // No parse state to update — the ACP layer handles forwarding.
                },

                StreamEvent::RequestAttempts(ev) => {
                    // Record the max attempt count seen — defensive against duplicate events.
                    self.request_attempts = Some(self.request_attempts.unwrap_or(0).max(ev.count));
                },
            },

            // Parse invariant - we don't expect any further events after receiving a single
            // error.
            StreamResult::Err(err) => {
                debug_assert!(
                    self.stream_err.is_none(),
                    "Only one stream error event is expected. Previously found: {:?}, just received: {:?}",
                    self.stream_err,
                    err
                );
                self.stream_err = Some(err);
                self.errored = true;
            },
        }
    }

    pub fn has_tool_uses(&self) -> bool {
        !self.tool_uses.is_empty()
    }

    pub fn builtin_tool_uses(&self) -> u32 {
        self.tool_uses
            .iter()
            .filter(|tool_use| BuiltInToolName::from_str(tool_use.name.as_str()).is_ok())
            .count() as u32
    }

    pub fn ended(&self) -> bool {
        self.ended_time.is_some()
    }

    pub fn errored(&self) -> bool {
        self.errored
    }

    pub fn interrupted(&self) -> bool {
        self.stream_err
            .as_ref()
            .is_some_and(|e| matches!(e.kind, StreamErrorKind::Interrupted))
    }

    fn make_stream_metadata(&self) -> StreamMetadata {
        StreamMetadata {
            stream: self.metadata.clone(),
            tool_uses: self.tool_uses.clone(),
            request_attempts: self.request_attempts,
        }
    }

    /// Whether the stream completed cleanly without ever carrying any content event.
    /// Safe to retry because nothing was rendered to the client.
    fn is_empty_response(&self) -> bool {
        let empty = self.message_stop.is_some() && self.stream_err.is_none() && !self.received_content_event;
        if empty {
            trace!(
                message_stop = ?self.message_stop,
                received_content_event = self.received_content_event,
                assistant_text_len = self.assistant_text.len(),
                tool_uses_count = self.tool_uses.len(),
                thinking_blocks_count = self.thinking_blocks.len(),
                "is_empty_response=true — model stream completed with no content events"
            );
        }
        empty
    }

    /// Create the final result value from parsing the model response stream
    fn make_result(&self) -> Result<Message, LoopError> {
        if let Some(err) = self.stream_err.as_ref() {
            Err(LoopError::Stream(err.clone()))
        } else if !self.invalid_tool_uses.is_empty() {
            Err(LoopError::InvalidJson {
                invalid_tools: self.invalid_tool_uses.clone(),
                valid_tools: self.tool_uses.clone(),
                assistant_text: self.assistant_text.clone(),
            })
        } else if self.is_empty_response() {
            Err(LoopError::EmptyResponse)
        } else {
            debug_assert!(
                self.message_stop.is_some(),
                "Expected a message stop event before the stream has ended"
            );
            // Build the response message. Note: interleaved thinking order is flattened —
            // all thinking blocks come first, then text, then tool uses. The original
            // stream order (think → text → think → text) is not preserved, but this is
            // acceptable since the API only supports a single reasoning_content per turn.
            let mut content = Vec::new();
            for thinking_block in &self.thinking_blocks {
                content.push(ContentBlock::Thinking(thinking_block.clone()));
            }
            content.push(ContentBlock::Text(self.assistant_text.clone()));
            for tool_use in &self.tool_uses {
                content.push(ContentBlock::ToolUse(tool_use.clone()));
            }
            let message = Message::new(Uuid::new_v4().to_string(), Role::Assistant, content, Some(Utc::now()));
            Ok(message)
        }
    }
}

/// Handle for communicating with an [`AgentLoop`] actor.
#[derive(Debug)]
pub struct AgentLoopHandle {
    /// Identifier for the loop.
    id: AgentLoopId,
    /// Sender for sending requests to the agent loop
    sender: RequestSender<AgentLoopRequest, AgentLoopResponse, AgentLoopResponseError>,
    loop_event_rx: mpsc::Receiver<AgentLoopEventKind>,
    /// The [JoinHandle] to the task executing the agent loop.
    handle: JoinHandle<()>,
}

impl AgentLoopHandle {
    fn new(
        id: AgentLoopId,
        sender: RequestSender<AgentLoopRequest, AgentLoopResponse, AgentLoopResponseError>,
        loop_event_rx: mpsc::Receiver<AgentLoopEventKind>,
        handle: JoinHandle<()>,
    ) -> Self {
        Self {
            id,
            sender,
            loop_event_rx,
            handle,
        }
    }

    /// Identifier for the loop.
    pub fn id(&self) -> &AgentLoopId {
        &self.id
    }

    pub async fn recv(&mut self) -> Option<AgentLoopEventKind> {
        self.loop_event_rx.recv().await
    }

    pub async fn send_request(
        &mut self,
        model: Arc<dyn Model>,
        args: SendRequestArgs,
    ) -> Result<AgentLoopResponse, AgentLoopResponseError> {
        self.sender
            .send_recv(AgentLoopRequest::SendRequest { model, args })
            .await
            .unwrap_or(Err(AgentLoopResponseError::AgentLoopExited))
    }

    pub async fn get_loop_state(&self) -> Result<LoopState, AgentLoopResponseError> {
        match self
            .sender
            .send_recv(AgentLoopRequest::GetExecutionState)
            .await
            .unwrap_or(Err(AgentLoopResponseError::AgentLoopExited))?
        {
            AgentLoopResponse::ExecutionState(state) => Ok(state),
            other => Err(AgentLoopResponseError::Custom(format!(
                "unknown response getting execution state: {other:?}",
            ))),
        }
    }

    /// Ends the agent loop
    pub async fn cancel(&self) -> Result<(), AgentLoopResponseError> {
        _ = self.sender.send_recv(AgentLoopRequest::Cancel).await;

        Ok(())
    }
}

impl Drop for AgentLoopHandle {
    fn drop(&mut self) {
        debug!(?self.id, "agent loop handle has dropped, aborting");
        self.handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_loop::types::*;

    fn user_message() -> Message {
        Message::new(
            Uuid::new_v4().to_string(),
            Role::User,
            vec![ContentBlock::Text("test".into())],
            None,
        )
    }

    fn message_start() -> StreamResult {
        StreamResult::Ok(StreamEvent::MessageStart(MessageStartEvent { role: Role::Assistant }))
    }

    fn message_stop() -> StreamResult {
        StreamResult::Ok(StreamEvent::MessageStop(MessageStopEvent {
            stop_reason: StopReason::EndTurn,
        }))
    }

    fn text_delta(text: &str) -> StreamResult {
        StreamResult::Ok(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            delta: ContentBlockDelta::Text(text.into()),
            content_block_index: None,
        }))
    }

    fn tool_start(id: &str, name: &str) -> StreamResult {
        StreamResult::Ok(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
            content_block_start: Some(ContentBlockStart::ToolUse(ToolUseBlockStart {
                tool_use_id: id.into(),
                name: name.into(),
            })),
            content_block_index: None,
        }))
    }

    fn tool_delta(input: &str) -> StreamResult {
        StreamResult::Ok(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            delta: ContentBlockDelta::ToolUse(ToolUseBlockDelta { input: input.into() }),
            content_block_index: None,
        }))
    }

    fn block_stop() -> StreamResult {
        StreamResult::Ok(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
            content_block_index: None,
        }))
    }

    /// Feed events into state and return the result from ResponseStreamEnd.
    fn run_stream(events: Vec<StreamResult>) -> Result<Message, LoopError> {
        let mut state = StreamParseState::new(user_message(), None);
        let mut buf = Vec::new();
        for ev in events {
            state.next(Some(ev), &mut buf);
        }
        state.next(None, &mut buf);
        buf.into_iter()
            .find_map(|ev| match ev {
                AgentLoopEventKind::ResponseStreamEnd { result, .. } => Some(result),
                _ => None,
            })
            .expect("expected ResponseStreamEnd event")
    }

    /// Like `run_stream` but returns both the result and the metadata.
    fn run_stream_with_metadata(
        events: Vec<StreamResult>,
    ) -> (
        Result<Message, LoopError>,
        crate::agent::agent_loop::protocol::StreamMetadata,
    ) {
        let mut state = StreamParseState::new(user_message(), None);
        let mut buf = Vec::new();
        for ev in events {
            state.next(Some(ev), &mut buf);
        }
        state.next(None, &mut buf);
        buf.into_iter()
            .find_map(|ev| match ev {
                AgentLoopEventKind::ResponseStreamEnd { result, metadata } => Some((result, metadata)),
                _ => None,
            })
            .expect("expected ResponseStreamEnd event")
    }

    #[test]
    fn user_turn_metadata_records_first_prompt_length() {
        fn ended_stream_state(user_message: Message) -> StreamParseState {
            let mut state = StreamParseState::new(user_message, None);
            let mut events = Vec::new();
            state.next(Some(message_start()), &mut events);
            state.next(Some(message_stop()), &mut events);
            state.next(None, &mut events);
            state
        }

        let cancel_token = CancellationToken::new();
        let mut agent_loop = AgentLoop::new(AgentLoopId::new(AgentId::default()), cancel_token);
        agent_loop.stream_states.push(ended_stream_state(Message::new(
            Uuid::new_v4().to_string(),
            Role::User,
            vec![ContentBlock::Text("count me".into())],
            None,
        )));
        agent_loop.stream_states.push(ended_stream_state(Message::new(
            Uuid::new_v4().to_string(),
            Role::User,
            vec![ContentBlock::ToolResult(ToolResultBlock {
                tool_use_id: "tu_1".into(),
                content: vec![ToolResultContentBlock::Text("do not count me".into())],
                status: ToolResultStatus::Success,
            })],
            None,
        )));

        let metadata = agent_loop.make_user_turn_metadata();

        assert_eq!(metadata.user_prompt_length, "count me".len());
    }

    #[test]
    fn normal_stream_with_valid_tool_uses() {
        let result = run_stream(vec![
            message_start(),
            text_delta("hello"),
            tool_start("tu_1", "fs_read"),
            tool_delta(r#"{"path": "/tmp"}"#),
            block_stop(),
            message_stop(),
        ]);
        let msg = result.expect("expected Ok");
        assert_eq!(msg.content.len(), 2);
        match &msg.content[1] {
            ContentBlock::ToolUse(tool) => {
                assert_eq!(tool.tool_use_id, "tu_1");
                assert_eq!(tool.name, "fs_read");
            },
            other => panic!("expected ToolUse, got: {other:?}"),
        }
    }

    #[test]
    fn invalid_json_only_no_valid_tools() {
        let result = run_stream(vec![
            message_start(),
            text_delta("thinking"),
            tool_start("tu_1", "fs_write"),
            tool_delta("{invalid json"),
            block_stop(),
            message_stop(),
        ]);
        let err = result.expect_err("expected InvalidJson error");
        match err {
            LoopError::InvalidJson {
                invalid_tools,
                valid_tools,
                assistant_text,
            } => {
                assert_eq!(invalid_tools.len(), 1);
                assert_eq!(invalid_tools[0].tool_use_id, "tu_1");
                assert!(valid_tools.is_empty());
                assert_eq!(assistant_text, "thinking");
            },
            other => panic!("expected InvalidJson, got: {other:?}"),
        }
    }

    #[test]
    fn mixed_valid_and_invalid_tool_uses() {
        let result = run_stream(vec![
            message_start(),
            text_delta("text"),
            tool_start("tu_1", "fs_read"),
            tool_delta(r#"{"path": "/tmp"}"#),
            block_stop(),
            tool_start("tu_2", "fs_write"),
            tool_delta("{bad json"),
            block_stop(),
            message_stop(),
        ]);
        let err = result.expect_err("expected InvalidJson error");
        match err {
            LoopError::InvalidJson {
                invalid_tools,
                valid_tools,
                assistant_text,
            } => {
                assert_eq!(valid_tools.len(), 1);
                assert_eq!(valid_tools[0].tool_use_id, "tu_1");
                assert_eq!(valid_tools[0].name, "fs_read");
                assert_eq!(invalid_tools.len(), 1);
                assert_eq!(invalid_tools[0].tool_use_id, "tu_2");
                assert_eq!(assistant_text, "text");
            },
            other => panic!("expected InvalidJson, got: {other:?}"),
        }
    }

    #[test]
    fn incomplete_tool_use_stream_ends_mid_parse() {
        // Stream ends while a tool use is still being parsed (no ContentBlockStop).
        // The incomplete tool (tu_2) has truncated JSON, so the parser returns
        // InvalidJson with the valid tool (tu_1) and the invalid one (tu_2).
        let result = run_stream(vec![
            message_start(),
            tool_start("tu_1", "fs_read"),
            tool_delta(r#"{"path": "/tmp"}"#),
            block_stop(),
            tool_start("tu_2", "fs_write"),
            tool_delta(r#"{"path": "/tmp/f"#),
            // No block_stop for tu_2, but message_stop still arrives
            message_stop(),
        ]);
        match result.expect_err("expected InvalidJson error") {
            LoopError::InvalidJson {
                invalid_tools,
                valid_tools,
                assistant_text,
            } => {
                assert_eq!(valid_tools.len(), 1);
                assert_eq!(valid_tools[0].tool_use_id, "tu_1");
                assert_eq!(valid_tools[0].name, "fs_read");
                assert_eq!(invalid_tools.len(), 1);
                assert_eq!(invalid_tools[0].tool_use_id, "tu_2");
                assert_eq!(invalid_tools[0].name, "fs_write");
                assert!(assistant_text.is_empty());
            },
            other => panic!("expected InvalidJson, got: {other:?}"),
        }
    }

    #[test]
    fn stream_error_takes_priority_over_invalid_json() {
        let mut state = StreamParseState::new(user_message(), None);
        let mut buf = Vec::new();
        state.next(Some(message_start()), &mut buf);
        state.next(
            Some(StreamResult::Err(StreamError::new(StreamErrorKind::ServiceFailure))),
            &mut buf,
        );
        state.next(None, &mut buf);
        let result = buf
            .into_iter()
            .find_map(|ev| match ev {
                AgentLoopEventKind::ResponseStreamEnd { result, .. } => Some(result),
                _ => None,
            })
            .expect("expected ResponseStreamEnd");
        assert!(matches!(result, Err(LoopError::Stream(_))));
    }

    fn thinking_start() -> StreamResult {
        StreamResult::Ok(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
            content_block_start: Some(ContentBlockStart::Thinking),
            content_block_index: None,
        }))
    }

    fn thinking_delta(text: &str) -> StreamResult {
        StreamResult::Ok(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            delta: ContentBlockDelta::Reasoning(text.into()),
            content_block_index: None,
        }))
    }

    fn reasoning_signature(sig: &str) -> StreamResult {
        StreamResult::Ok(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            delta: ContentBlockDelta::ReasoningSignature {
                signature: Some(sig.into()),
                redacted_content: None,
            },
            content_block_index: None,
        }))
    }

    #[test]
    fn thinking_block_parsed_with_signature() {
        let msg = run_stream(vec![
            message_start(),
            thinking_start(),
            thinking_delta("Let me think..."),
            thinking_delta(" about this."),
            reasoning_signature("sig123"),
            block_stop(),
            text_delta("Here's my answer."),
            message_stop(),
        ])
        .expect("expected Ok");

        assert_eq!(msg.content.len(), 2);
        match &msg.content[0] {
            ContentBlock::Thinking(tb) => {
                assert_eq!(tb.text, "Let me think... about this.");
                assert_eq!(tb.signature.as_deref(), Some("sig123"));
                assert!(tb.redacted_content.is_empty());
            },
            other => panic!("expected Thinking, got: {other:?}"),
        }
        match &msg.content[1] {
            ContentBlock::Text(t) => assert_eq!(t, "Here's my answer."),
            other => panic!("expected Text, got: {other:?}"),
        }
    }

    #[test]
    fn interleaved_thinking_produces_multiple_blocks() {
        let msg = run_stream(vec![
            message_start(),
            thinking_start(),
            thinking_delta("first thought"),
            reasoning_signature("sig1"),
            block_stop(),
            text_delta("response part 1"),
            thinking_start(),
            thinking_delta("second thought"),
            reasoning_signature("sig2"),
            block_stop(),
            text_delta(" and part 2"),
            message_stop(),
        ])
        .expect("expected Ok");

        let thinking_blocks: Vec<_> = msg
            .content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Thinking(tb) => Some(tb),
                _ => None,
            })
            .collect();
        assert_eq!(thinking_blocks.len(), 2);
        assert_eq!(thinking_blocks[0].text, "first thought");
        assert_eq!(thinking_blocks[0].signature.as_deref(), Some("sig1"));
        assert_eq!(thinking_blocks[1].text, "second thought");
        assert_eq!(thinking_blocks[1].signature.as_deref(), Some("sig2"));

        assert_eq!(msg.text(), "response part 1 and part 2");
    }

    #[test]
    fn thinking_with_redacted_content() {
        let msg = run_stream(vec![
            message_start(),
            thinking_start(),
            thinking_delta(""),
            StreamResult::Ok(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::ReasoningSignature {
                    signature: None,
                    redacted_content: Some(vec![0xde, 0xad]),
                },
                content_block_index: None,
            })),
            block_stop(),
            text_delta("answer"),
            message_stop(),
        ])
        .expect("expected Ok");

        match &msg.content[0] {
            ContentBlock::Thinking(tb) => {
                assert!(tb.signature.is_none());
                assert_eq!(tb.redacted_content, vec![0xde, 0xad]);
            },
            other => panic!("expected Thinking, got: {other:?}"),
        }
    }

    /// Regression test for orphan thinking blocks bricking long sessions.
    ///
    /// When the upstream stream ends mid-thinking-block (no signature, no
    /// redacted content ever delivered), the RTS parser synthesizes a
    /// ContentBlockStop at end-of-stream while `in_thinking`. Without
    /// guarding against this, the agent_loop seals an "orphan" ThinkingBlock
    /// with `signature: None` and empty `redacted_content`. That orphan
    /// then poisons conversation history — every subsequent turn sends it
    /// back to Bedrock and gets rejected with
    /// `messages.N.content.0.thinking.signature: Field required`.
    ///
    /// Observed in production: a session was bricked after exactly one
    /// truncated thinking block landed in its persisted history.
    #[test]
    fn orphan_thinking_block_dropped_when_no_signature_or_redacted_content() {
        let msg = run_stream(vec![
            message_start(),
            thinking_start(),
            thinking_delta("Let me think about this..."),
            // No reasoning_signature event — simulates upstream truncation.
            block_stop(),
            message_stop(),
        ])
        .expect("expected Ok");

        let thinking_blocks: Vec<_> = msg
            .content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Thinking(tb) => Some(tb),
                _ => None,
            })
            .collect();

        assert!(
            thinking_blocks.is_empty(),
            "orphan thinking block (no signature, no redacted_content) should be dropped \
             at parse time so it cannot poison conversation history; got: {thinking_blocks:?}"
        );
    }

    /// Empty stream: messageStart -> messageStop with no content events in between. Must
    /// produce LoopError::EmptyResponse so the agent layer can retry.
    #[test]
    fn empty_stream_produces_empty_response_error() {
        let result = run_stream(vec![message_start(), message_stop()]);
        assert!(
            matches!(result, Err(LoopError::EmptyResponse)),
            "stream with no content events should produce EmptyResponse, got: {result:?}"
        );
    }

    /// A stream that delivered content events to the client must NOT be classified as
    /// EmptyResponse, even if the content was later dropped (e.g. orphan thinking blocks).
    /// Retrying after the user already saw deltas would double-render content.
    #[test]
    fn orphan_thinking_is_not_empty_response() {
        let result = run_stream(vec![
            message_start(),
            thinking_start(),
            thinking_delta("Let me think..."),
            block_stop(),
            message_stop(),
        ]);
        assert!(
            result.is_ok(),
            "stream that streamed thinking deltas must not be EmptyResponse, got: {result:?}"
        );
    }

    #[test]
    fn thinking_emits_thinking_text_events() {
        let mut state = StreamParseState::new(user_message(), None);
        let mut buf = Vec::new();
        state.next(Some(message_start()), &mut buf);
        state.next(Some(thinking_start()), &mut buf);
        state.next(Some(thinking_delta("hello")), &mut buf);
        state.next(Some(thinking_delta(" world")), &mut buf);

        let thinking_texts: Vec<_> = buf
            .iter()
            .filter_map(|ev| match ev {
                AgentLoopEventKind::ThinkingText(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(thinking_texts, vec!["hello", " world"]);
    }

    #[test]
    fn retry_warning_before_message_start_does_not_panic() {
        // RetryWarning can arrive before MessageStart (during HTTP retries).
        // This must not trigger the debug assertion that requires MessageStart first.
        let mut state = StreamParseState::new(user_message(), None);
        let mut buf = Vec::new();
        let warning = StreamResult::Ok(StreamEvent::RetryWarning(RetryWarningEvent {
            attempt: 2,
            max_attempts: 6,
            delay_secs: 5.0,
            message: "Retrying in 5s (attempt 2/6)".into(),
        }));
        state.next(Some(warning), &mut buf);
        // Should be forwarded as a Stream event
        assert!(
            buf.iter().any(|ev| matches!(
                ev,
                AgentLoopEventKind::Stream(StreamResult::Ok(StreamEvent::RetryWarning(_)))
            )),
            "RetryWarning should be forwarded as a Stream event"
        );
    }

    #[test]
    fn retry_warning_during_stream_does_not_affect_parse() {
        // RetryWarning events interspersed with normal content should not
        // affect the final parsed message.
        let result = run_stream(vec![
            StreamResult::Ok(StreamEvent::RetryWarning(RetryWarningEvent {
                attempt: 2,
                max_attempts: 6,
                delay_secs: 5.0,
                message: "Retrying in 5s (attempt 2/6)".into(),
            })),
            message_start(),
            text_delta("hello"),
            StreamResult::Ok(StreamEvent::RetryWarning(RetryWarningEvent {
                attempt: 3,
                max_attempts: 6,
                delay_secs: 10.0,
                message: "Retrying in 10.0s (attempt 3/6)".into(),
            })),
            message_stop(),
        ]);
        let msg = result.expect("expected Ok");
        assert_eq!(msg.text(), "hello");
    }

    #[test]
    fn request_attempts_event_populates_stream_metadata() {
        // RequestAttempts emitted mid-stream (after retries, before the actual response)
        // should be captured in StreamMetadata.request_attempts.
        let (result, metadata) = run_stream_with_metadata(vec![
            StreamResult::Ok(StreamEvent::RequestAttempts(RequestAttemptsEvent { count: 3 })),
            message_start(),
            text_delta("hello"),
            message_stop(),
        ]);
        assert!(result.is_ok());
        assert_eq!(metadata.request_attempts, Some(3));
    }

    #[test]
    fn request_attempts_event_propagates_on_error() {
        // For error paths (e.g. dispatch failure after retries), the transport layer emits
        // RequestAttempts before the Err — the parser must capture it so telemetry can
        // distinguish "failed on first attempt" from "failed after retries".
        let (result, metadata) = run_stream_with_metadata(vec![
            StreamResult::Ok(StreamEvent::RequestAttempts(RequestAttemptsEvent { count: 3 })),
            StreamResult::Err(StreamError::new(StreamErrorKind::Other {
                reason_code: Some("dispatch failure (io error): connection refused".into()),
                message: "dispatch failure".into(),
            })),
        ]);
        assert!(matches!(result, Err(LoopError::Stream(_))));
        assert_eq!(metadata.request_attempts, Some(3));
    }

    #[test]
    fn request_attempts_takes_max_across_duplicate_events() {
        // Defensive: if multiple RequestAttempts events arrive (e.g. implementation bug),
        // the parser keeps the max rather than last-wins, since the max represents the
        // total attempts made.
        let (_, metadata) = run_stream_with_metadata(vec![
            StreamResult::Ok(StreamEvent::RequestAttempts(RequestAttemptsEvent { count: 3 })),
            StreamResult::Ok(StreamEvent::RequestAttempts(RequestAttemptsEvent { count: 1 })),
            message_start(),
            text_delta("x"),
            message_stop(),
        ]);
        assert_eq!(metadata.request_attempts, Some(3));
    }

    /// Empty tool_content (e.g. zero-arg tool with "" input) should be coerced
    /// to `{}`, not classified as InvalidJson.
    #[test]
    fn empty_tool_content_is_coerced_to_empty_object() {
        let result = run_stream(vec![
            message_start(),
            tool_start("tooluse_repro_001", "list_crons"),
            // No tool_delta — simulates empty-string content.
            block_stop(),
            message_stop(),
        ]);

        let msg = result.expect("expected Ok — empty content should coerce to {}");
        assert_eq!(msg.content.len(), 2, "expected Text + ToolUse blocks");
        match &msg.content[1] {
            ContentBlock::ToolUse(tool) => {
                assert_eq!(tool.tool_use_id, "tooluse_repro_001");
                assert_eq!(tool.name, "list_crons");
                assert_eq!(tool.input, serde_json::json!({}));
            },
            other => panic!("expected ToolUse, got: {other:?}"),
        }
    }

    #[test]
    fn thinking_block_tagged_with_model_id() {
        let mut state = StreamParseState::new(user_message(), Some("claude-opus-4.7".to_string()));
        let mut buf = Vec::new();
        state.next(Some(message_start()), &mut buf);
        state.next(Some(thinking_start()), &mut buf);
        state.next(Some(thinking_delta("reasoning")), &mut buf);
        state.next(Some(reasoning_signature("sig")), &mut buf);
        state.next(Some(block_stop()), &mut buf);
        state.next(Some(text_delta("answer")), &mut buf);
        state.next(Some(message_stop()), &mut buf);
        state.next(None, &mut buf);

        let msg = buf
            .into_iter()
            .find_map(|ev| match ev {
                AgentLoopEventKind::ResponseStreamEnd { result, .. } => Some(result),
                _ => None,
            })
            .expect("expected ResponseStreamEnd")
            .expect("expected Ok result");
        let tb = msg
            .content
            .iter()
            .find_map(|c| match c {
                ContentBlock::Thinking(tb) => Some(tb),
                _ => None,
            })
            .expect("expected ThinkingBlock");
        assert_eq!(tb.model_id, Some("claude-opus-4.7".to_string()));
    }

    #[test]
    fn thinking_block_model_id_none_when_no_model() {
        let mut state = StreamParseState::new(user_message(), None);
        let mut buf = Vec::new();
        state.next(Some(message_start()), &mut buf);
        state.next(Some(thinking_start()), &mut buf);
        state.next(Some(thinking_delta("reasoning")), &mut buf);
        state.next(Some(reasoning_signature("sig")), &mut buf);
        state.next(Some(block_stop()), &mut buf);
        state.next(Some(text_delta("answer")), &mut buf);
        state.next(Some(message_stop()), &mut buf);
        state.next(None, &mut buf);

        let msg = buf
            .into_iter()
            .find_map(|ev| match ev {
                AgentLoopEventKind::ResponseStreamEnd { result, .. } => Some(result),
                _ => None,
            })
            .expect("expected ResponseStreamEnd")
            .expect("expected Ok result");
        let tb = msg
            .content
            .iter()
            .find_map(|c| match c {
                ContentBlock::Thinking(tb) => Some(tb),
                _ => None,
            })
            .expect("expected ThinkingBlock");
        assert_eq!(tb.model_id, None);
    }

    #[test]
    fn thinking_block_deserializes_without_model_id_field() {
        // Simulates loading a session from an old binary that didn't write modelId
        let json = r#"{"text":"reasoning","signature":"sig123","redactedContent":[]}"#;
        let tb: types::ThinkingBlock = serde_json::from_str(json).unwrap();
        assert_eq!(tb.text, "reasoning");
        assert_eq!(tb.signature, Some("sig123".to_string()));
        assert_eq!(tb.model_id, None);
    }

    #[test]
    fn thinking_block_deserializes_with_model_id_field() {
        // Simulates loading a session from the new binary
        let json = r#"{"text":"reasoning","signature":"sig123","redactedContent":[],"modelId":"claude-opus-4.7"}"#;
        let tb: types::ThinkingBlock = serde_json::from_str(json).unwrap();
        assert_eq!(tb.model_id, Some("claude-opus-4.7".to_string()));
    }

    #[test]
    fn thinking_block_serializes_without_model_id_when_none() {
        // Old binary ignores unknown fields, so None should not emit modelId
        let tb = types::ThinkingBlock {
            text: "reasoning".to_string(),
            signature: Some("sig".to_string()),
            redacted_content: vec![],
            model_id: None,
        };
        let json = serde_json::to_string(&tb).unwrap();
        assert!(!json.contains("modelId"), "None model_id should be omitted: {json}");
    }

    #[test]
    fn thinking_block_serializes_with_model_id_when_present() {
        let tb = types::ThinkingBlock {
            text: "reasoning".to_string(),
            signature: Some("sig".to_string()),
            redacted_content: vec![],
            model_id: Some("claude-opus-4.7".to_string()),
        };
        let json = serde_json::to_string(&tb).unwrap();
        assert!(
            json.contains(r#""modelId":"claude-opus-4.7""#),
            "model_id should be serialized: {json}"
        );
    }

    #[test]
    fn old_binary_session_with_thinking_loads_on_new_binary() {
        // Old binary wrote ThinkingBlock without modelId. New binary must load it
        // and treat it as model_id = None (which gets stripped on send — safe default).
        let old_session_json = r#"{
            "text": "The user wants to understand async...",
            "signature": "EogFClsIDRABGAI=",
            "redactedContent": []
        }"#;
        let tb: types::ThinkingBlock = serde_json::from_str(old_session_json).unwrap();
        assert_eq!(
            tb.model_id, None,
            "old session without modelId should deserialize as None"
        );
        assert_eq!(tb.text, "The user wants to understand async...");
        assert_eq!(tb.signature, Some("EogFClsIDRABGAI=".to_string()));
    }

    #[test]
    fn new_binary_session_with_model_id_loads_on_old_binary_simulation() {
        // New binary writes modelId. Old binary (simulated) should ignore unknown fields.
        // serde_json with deny_unknown_fields would fail, but our struct uses default serde
        // which ignores unknown fields. Verify round-trip works.
        let new_session_json = r#"{
            "text": "reasoning text",
            "signature": "sig123",
            "redactedContent": [],
            "modelId": "claude-opus-4.7",
            "futureField": "should be ignored"
        }"#;
        // This simulates what an old binary would do — it doesn't have modelId in its struct
        // but serde ignores unknown fields by default
        let tb: types::ThinkingBlock = serde_json::from_str(new_session_json).unwrap();
        assert_eq!(tb.text, "reasoning text");
        // The old binary wouldn't have model_id field, but since we're using the new struct
        // it picks it up. The key point is: no deserialization error.
    }
}
