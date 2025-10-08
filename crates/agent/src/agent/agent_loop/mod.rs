pub mod model;
pub mod protocol;
pub mod types;

use std::pin::Pin;
use std::time::Instant;

use chrono::Utc;
use eyre::Result;
use futures::{
    Stream,
    StreamExt,
};
use model::AgentLoopModel;
use protocol::{
    AgentLoopEventKind,
    AgentLoopRequest,
    AgentLoopResponse,
    AgentLoopResponseError,
    EndReason,
    LoopError,
    SendRequestArgs,
    StreamMetadata,
    UserTurnMetadata,
};
use rand::seq::IndexedRandom;
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
    warn,
};
use types::{
    ContentBlock,
    Message,
    MessageStopEvent,
    MetadataEvent,
    Role,
    StreamError,
    StreamErrorKind,
    StreamEvent,
    ToolUseBlock,
};

use crate::agent::AgentId;
use crate::agent::util::request_channel::{
    RequestReceiver,
    RequestSender,
    new_request_channel,
    respond,
};

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

    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
}

impl std::fmt::Display for AgentLoopId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.agent_id, self.rand)
    }
}

// impl FromStr for AgentLoopId {
//     type Err = String;
//
//     fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
//         match s.find("/") {
//             Some(i) => Ok(Self {
//                 agent_id: s[..i].to_string(),
//                 rand: match s[i + 1..].to_string().parse() {
//                     Ok(v) => v,
//                     Err(_) => return Err(s.to_string()),
//                 },
//             }),
//             None => Err(s.to_string()),
//         }
//     }
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum LoopState {
    #[default]
    Idle,
    /// A request is currently being sent to the model
    SendingRequest,
    /// A model response is currently being consumed
    ConsumingResponse,
    /// The loop is waiting for tool use result(s) to be provided
    PendingToolUseResults,
    /// The agent loop has completed all processing, and no pending work is left to do.
    ///
    /// This is the final state of the loop - no further requests can be made.
    UserTurnEnded,
    /// An error occurred that requires manual intervention
    Errored,
}

// #[derive(Debug)]
// struct StreamRequest {
//     model: Box<dyn AgentLoopModel>,
//     messages: Vec<Message>,
//     tool_specs: Option<Vec<ToolSpec>>,
//     system_prompt: Option<String>,
// }

/// Tracks the execution of a user turn, ending when either the model returns a response with no
/// tool uses, or a non-retryable error is encountered.
pub struct AgentLoop {
    /// Identifier for the loop.
    id: AgentLoopId,

    /// Current state of the loop
    execution_state: LoopState,

    /// Cancellation token used for gracefully cancelling the underlying response stream
    cancel_token: CancellationToken,

    /// The current response stream future being received along with it's associated parse state
    curr_stream: Option<(
        StreamParseState,
        Pin<Box<dyn Stream<Item = Result<StreamEvent, StreamError>> + Send>>,
    )>,

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
            .field("curr_stream", &self.curr_stream.as_ref().map(|s| &s.0))
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
        let cancel_token_clone = self.cancel_token.clone();
        let loop_event_rx = self.loop_event_rx.take().expect("loop_event_rx should exist");
        let loop_req_tx = self.loop_req_tx.take().expect("loop_req_tx should exist");
        let handle = tokio::spawn(async move {
            info!("agent loop start");
            self.run().await;
            info!("agent loop end");
        });
        AgentLoopHandle::new(id_clone, loop_req_tx, loop_event_rx, cancel_token_clone, handle)
    }

    async fn run(mut self) {
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
                //
                // We do some trickery to return a future that never resolves if we're not currently
                // consuming a response stream.
                res = async {
                    match self.curr_stream.take() {
                        Some((state, mut stream)) => {
                            let next_ev = stream.next().await;
                            (state, stream, next_ev)
                        },
                        None => std::future::pending().await,
                    }
                } => {
                    let (mut stream_state, stream, stream_event) = res;
                    debug!(?self.id, ?stream_event, "agent loop received stream event");

                    // Buffer for the stream parser to update with events to send
                    let mut loop_events: Vec<AgentLoopEventKind> = Vec::new();

                    // Advance the stream parse state
                    stream_state.next(stream_event, &mut loop_events);

                    if stream_state.ended() {
                        // Pushing the state early here to ensure the metadata event is created
                        // correctly in the case of UserTurnEnded.
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
                            loop_events.push(AgentLoopEventKind::UserTurnEnd(self.make_user_turn_metadata()));
                        }
                    } else {
                        // Stream is still being consumed, so add back to curr_stream.
                        self.curr_stream = Some((stream_state, stream));
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
        debug!(?self, ?req, "agent loop handling new request");
        match req {
            AgentLoopRequest::GetExecutionState => Ok(AgentLoopResponse::ExecutionState(self.execution_state)),
            AgentLoopRequest::SendRequest { model, args } => {
                if self.curr_stream.is_some() {
                    return Err(AgentLoopResponseError::StreamCurrentlyExecuting);
                }

                // Ensure we are in a state that can handle a new request.
                match self.execution_state {
                    LoopState::Idle | LoopState::PendingToolUseResults => {},
                    LoopState::UserTurnEnded => {
                        // TODO - custom message?
                        return Err(AgentLoopResponseError::AgentLoopExited);
                    },
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
                self.curr_stream = Some((StreamParseState::new(next_user_message), stream));
                Ok(AgentLoopResponse::Success)
            },

            AgentLoopRequest::Close => {
                let mut buf = Vec::new();
                // If there's an active stream, then interrupt it.
                if let Some((mut parse_state, mut fut)) = self.curr_stream.take() {
                    debug_assert!(self.execution_state == LoopState::ConsumingResponse);
                    self.cancel_token.cancel();
                    while let Some(ev) = fut.next().await {
                        parse_state.next(Some(ev), &mut buf);
                    }
                    parse_state.next(None, &mut buf);
                    debug_assert!(parse_state.ended());
                    self.stream_states.push(parse_state);
                }

                let metadata = self.make_user_turn_metadata();
                buf.push(self.set_execution_state(LoopState::UserTurnEnded));
                buf.push(AgentLoopEventKind::UserTurnEnd(metadata.clone()));

                for ev in buf.drain(..) {
                    self.loop_event_tx.send(ev).await.ok();
                }

                Ok(AgentLoopResponse::Metadata(metadata))
            },

            AgentLoopRequest::GetPendingToolUses => {
                if self.execution_state != LoopState::PendingToolUseResults {
                    return Ok(AgentLoopResponse::PendingToolUses(None));
                }
                let tool_uses = self.stream_states.last().map(|s| s.tool_uses.clone());
                debug_assert!(tool_uses.as_ref().is_some_and(|v| !v.is_empty()));
                Ok(AgentLoopResponse::PendingToolUses(tool_uses))
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
        for s in &self.stream_states {
            message_ids.push(s.user_message.id.clone());
            message_ids.push(s.message_id.clone());
        }

        UserTurnMetadata {
            loop_id: self.id.clone(),
            result: self.stream_states.last().map(|s| s.make_result()),
            message_ids,
            total_request_count: self.stream_states.len() as u32,
            number_of_cycles: self.stream_states.iter().filter(|s| s.has_tool_uses()).count() as u32,
            turn_duration: match (self.loop_start_time, self.loop_end_time) {
                (Some(start), Some(end)) => Some(end.duration_since(start)),
                _ => None,
            },
            end_reason: self.stream_states.last().map_or(EndReason::DidNotRun, |s| {
                if s.interrupted() {
                    EndReason::Cancelled
                } else if s.errored() {
                    EndReason::Error
                } else if s.has_tool_uses() {
                    EndReason::ToolUseRejected
                } else {
                    EndReason::UserTurnEnd
                }
            }),
            end_timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidToolUse {
    pub tool_use_id: String,
    pub name: String,
    pub content: String,
}

/// State associated with parsing a stream of [Result<StreamEvent, StreamError>] into
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
    /// Buffered metadata event returned from the response stream
    metadata: Option<MetadataEvent>,
    /// Buffered message stop event returned from the response stream
    message_stop: Option<MessageStopEvent>,
    /// Buffered error event returned from the response stream
    stream_err: Option<StreamError>,

    ended_time: Option<Instant>,
    /// Whether or not the stream encountered an error.
    ///
    /// Once an error has occurred, no new events can be received
    errored: bool,
}

impl StreamParseState {
    pub fn new(user_message: Message) -> Self {
        Self {
            assistant_text: String::new(),
            parsing_tool_use: None,
            tool_uses: Vec::new(),
            invalid_tool_uses: Vec::new(),
            user_message,
            message_id: None,
            metadata: None,
            message_stop: None,
            stream_err: None,
            ended_time: None,
            errored: false,
        }
    }

    pub fn next(&mut self, ev: Option<Result<StreamEvent, StreamError>>, buf: &mut Vec<AgentLoopEventKind>) {
        if self.errored {
            if let Some(ev) = ev {
                warn!(?ev, "ignoring unexpected event after having received an error");
            }
            return;
        }

        let Some(ev) = ev else {
            // No event received means the stream has ended.
            self.ended_time = Some(self.ended_time.unwrap_or(Instant::now()));
            self.errored = self.errored || !self.invalid_tool_uses.is_empty();
            let result = self.make_result();
            self.message_id = result.as_ref().map(|r| r.id.clone()).ok().flatten();
            buf.push(AgentLoopEventKind::ResponseStreamEnd {
                result,
                metadata: self.make_stream_metadata(),
            });
            return;
        };

        // Pushing low-level stream events in case end users want to consume these directly. Likely
        // not required.
        match &ev {
            Ok(e) => buf.push(AgentLoopEventKind::StreamEvent(e.clone())),
            Err(e) => buf.push(AgentLoopEventKind::StreamError(e.clone())),
        }

        match ev {
            Ok(s) => match s {
                StreamEvent::MessageStart(ev) => {
                    debug_assert!(ev.role == Role::Assistant);
                },
                StreamEvent::MessageStop(ev) => {
                    debug_assert!(self.message_stop.is_none());
                    self.message_stop = Some(ev);
                },

                StreamEvent::ContentBlockStart(ev) => {
                    if let Some(start) = ev.content_block_start {
                        match start {
                            types::ContentBlockStart::ToolUse(v) => {
                                self.parsing_tool_use = Some((v.tool_use_id.clone(), v.name.clone(), String::new()));
                                buf.push(AgentLoopEventKind::ToolUseStart {
                                    id: v.tool_use_id,
                                    name: v.name,
                                });
                            },
                        }
                    }
                },

                StreamEvent::ContentBlockDelta(ev) => match ev.delta {
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
                    types::ContentBlockDelta::Reasoning => (),
                    types::ContentBlockDelta::Document => (),
                },

                StreamEvent::ContentBlockStop(_) => {
                    if let Some((tool_use_id, name, tool_content)) = self.parsing_tool_use.take() {
                        match serde_json::from_str::<serde_json::Value>(&tool_content) {
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
            },

            // Parse invariant - we don't expect any further events after receiving a single
            // error.
            Err(err) => {
                debug_assert!(
                    self.stream_err.is_none(),
                    "Only one stream error event is expected. Previously found: {:?}, just received: {:?}",
                    self.stream_err,
                    err
                );
                self.stream_err = Some(err);
                self.errored = true;
                self.ended_time = Some(Instant::now());
            },
        }
    }

    pub fn has_tool_uses(&self) -> bool {
        !self.tool_uses.is_empty()
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
        }
    }

    /// Create the final result value from parsing the model response stream
    fn make_result(&self) -> Result<Message, LoopError> {
        if let Some(err) = self.stream_err.as_ref() {
            Err(LoopError::Stream(err.clone()))
        } else if !self.invalid_tool_uses.is_empty() {
            Err(LoopError::InvalidJson {
                invalid_tools: self.invalid_tool_uses.clone(),
                assistant_text: self.assistant_text.clone(),
            })
        } else {
            debug_assert!(
                self.message_stop.is_some(),
                "Expected a message stop event before the stream has ended"
            );
            let mut content = Vec::new();
            content.push(ContentBlock::Text(self.assistant_text.clone()));
            for tool_use in &self.tool_uses {
                content.push(ContentBlock::ToolUse(tool_use.clone()));
            }
            let message = Message::new(Role::Assistant, content, Some(Utc::now()));
            Ok(message)
        }
    }
}

#[derive(Debug)]
pub struct AgentLoopHandle {
    /// Identifier for the loop.
    id: AgentLoopId,
    /// Sender for sending requests to the agent loop
    sender: RequestSender<AgentLoopRequest, AgentLoopResponse, AgentLoopResponseError>,
    loop_event_rx: mpsc::Receiver<AgentLoopEventKind>,
    /// A [CancellationToken] used for gracefully closing the agent loop.
    cancel_token: CancellationToken,
    /// The [JoinHandle] to the task executing the agent loop.
    handle: JoinHandle<()>,
}

impl AgentLoopHandle {
    fn new(
        id: AgentLoopId,
        sender: RequestSender<AgentLoopRequest, AgentLoopResponse, AgentLoopResponseError>,
        loop_event_rx: mpsc::Receiver<AgentLoopEventKind>,
        cancel_token: CancellationToken,
        handle: JoinHandle<()>,
    ) -> Self {
        Self {
            id,
            sender,
            loop_event_rx,
            cancel_token,
            handle,
        }
    }

    /// Identifier for the loop.
    pub fn id(&self) -> &AgentLoopId {
        &self.id
    }

    /// Id of the agent this loop was created for.
    pub fn agent_id(&self) -> &AgentId {
        self.id.agent_id()
    }

    pub fn clone_weak(&self) -> AgentLoopWeakHandle {
        AgentLoopWeakHandle {
            id: self.id.clone(),
            sender: self.sender.clone(),
            cancel_token: self.cancel_token.clone(),
        }
    }

    pub async fn recv(&mut self) -> Option<AgentLoopEventKind> {
        self.loop_event_rx.recv().await
    }

    pub async fn send_request<M: AgentLoopModel>(
        &mut self,
        model: M,
        args: SendRequestArgs,
    ) -> Result<AgentLoopResponse, AgentLoopResponseError> {
        self.sender
            .send_recv(AgentLoopRequest::SendRequest {
                model: Box::new(model),
                args,
            })
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
                "unknown response getting execution state: {:?}",
                other,
            ))),
        }
    }

    pub async fn get_pending_tool_uses(&self) -> Result<Option<Vec<ToolUseBlock>>, AgentLoopResponseError> {
        match self
            .sender
            .send_recv(AgentLoopRequest::GetPendingToolUses)
            .await
            .unwrap_or(Err(AgentLoopResponseError::AgentLoopExited))?
        {
            AgentLoopResponse::PendingToolUses(v) => Ok(v),
            other => Err(AgentLoopResponseError::Custom(format!(
                "unknown response getting stream metadata: {:?}",
                other,
            ))),
        }
    }

    /// Ends the agent loop
    pub async fn close(&self) -> Result<UserTurnMetadata, AgentLoopResponseError> {
        match self
            .sender
            .send_recv(AgentLoopRequest::Close)
            .await
            .unwrap_or(Err(AgentLoopResponseError::AgentLoopExited))?
        {
            AgentLoopResponse::Metadata(md) => Ok(md),
            other => Err(AgentLoopResponseError::Custom(format!(
                "unknown response getting execution state: {:?}",
                other,
            ))),
        }
    }
}

impl Drop for AgentLoopHandle {
    fn drop(&mut self) {
        debug!(?self.id, "agent loop handle has dropped, aborting");
        self.handle.abort();
    }
}

/// A weak handle to an executing agent loop.
///
/// Where [AgentLoopHandle] can receive agent loop events and abort the task on drop,
/// [AgentLoopWeakHandle] is only used for sending messages to the agent loop.
#[derive(Debug, Clone)]
pub struct AgentLoopWeakHandle {
    id: AgentLoopId,
    sender: RequestSender<AgentLoopRequest, AgentLoopResponse, AgentLoopResponseError>,
    cancel_token: CancellationToken,
}

impl AgentLoopWeakHandle {
    pub async fn send_request<M: AgentLoopModel>(
        &self,
        model: M,
        args: SendRequestArgs,
    ) -> Result<AgentLoopResponse, AgentLoopResponseError> {
        self.sender
            .send_recv(AgentLoopRequest::SendRequest {
                model: Box::new(model),
                args,
            })
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
                "unknown response getting execution state: {:?}",
                other,
            ))),
        }
    }

    pub async fn get_pending_tool_uses(&self) -> Result<Option<Vec<ToolUseBlock>>, AgentLoopResponseError> {
        match self
            .sender
            .send_recv(AgentLoopRequest::GetPendingToolUses)
            .await
            .unwrap_or(Err(AgentLoopResponseError::AgentLoopExited))?
        {
            AgentLoopResponse::PendingToolUses(v) => Ok(v),
            other => Err(AgentLoopResponseError::Custom(format!(
                "unknown response getting stream metadata: {:?}",
                other,
            ))),
        }
    }

    /// Ends the agent loop
    pub async fn close(&self) -> Result<UserTurnMetadata, AgentLoopResponseError> {
        match self
            .sender
            .send_recv(AgentLoopRequest::Close)
            .await
            .unwrap_or(Err(AgentLoopResponseError::AgentLoopExited))?
        {
            AgentLoopResponse::Metadata(md) => Ok(md),
            other => Err(AgentLoopResponseError::Custom(format!(
                "unknown response getting execution state: {:?}",
                other,
            ))),
        }
    }

    /// Cancel the executing loop for graceful shutdown.
    fn cancel(&self) {
        self.cancel_token.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::api_client::error::{
        ConverseStreamError,
        ConverseStreamErrorKind,
    };

    #[test]
    fn test_other_stream_err_downcasting() {
        let err = StreamError::new(StreamErrorKind::Interrupted).with_source(Arc::new(ConverseStreamError::new(
            ConverseStreamErrorKind::ModelOverloadedError,
            None::<aws_smithy_types::error::operation::BuildError>, /* annoying type inference
                                                                     * required */
        )));
        assert!(
            err.as_rts_error()
                .is_some_and(|r| matches!(r.kind, ConverseStreamErrorKind::ModelOverloadedError))
        );
    }
}
