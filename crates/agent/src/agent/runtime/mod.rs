pub mod agent_loop;
pub mod types;

use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};
use std::pin::Pin;
use std::sync::Arc;

use agent_loop::{
    AgentLoop,
    AgentLoopEvent,
    AgentLoopEventKind,
    AgentLoopHandle,
    AgentLoopId,
    AgentLoopResponseError,
    AgentLoopWeakHandle,
    LoopError,
    LoopState,
    Model,
    SendRequestArgs,
    StreamErrorKind,
    UserTurnMetadata,
};
use chrono::Utc;
use eyre::Result;
use futures::stream::FuturesUnordered;
use futures::{
    FutureExt,
    Stream,
    StreamExt,
};
use rand::seq::IndexedRandom;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{
    debug,
    error,
    trace,
    warn,
};
use types::{
    ContentBlock,
    ToolResultBlock,
    ToolResultContentBlock,
    ToolResultStatus,
};
use uuid::Uuid;

use crate::chat::agent::AgentId;
use super::consts::MAX_CONVERSATION_STATE_HISTORY_LEN;
use crate::chat::consts::DUMMY_TOOL_NAME;
use crate::chat::rts::RtsModel;
use crate::chat::runtime::types::{
    Message,
    Role,
    ToolSpec,
    ToolUseBlock,
};
use crate::chat::util::{
    RequestReceiver,
    RequestSender,
    respond,
};

/// A handle to an agent
#[derive(Debug, Clone)]
pub struct AgentHandle {
    id: AgentId,
    sender: RequestSender<RuntimeRequest, RuntimeResponse, RuntimeError>,
}

impl AgentHandle {
    pub fn new(id: AgentId, sender: RequestSender<RuntimeRequest, RuntimeResponse, RuntimeError>) -> Self {
        Self { id, sender }
    }

    pub fn id(&self) -> &AgentId {
        &self.id
    }

    pub async fn get_loop_state(&self) -> Result<Option<(AgentLoopId, LoopState)>, RuntimeError> {
        match self
            .sender
            .send_recv(RuntimeRequest::GetLoopState {
                agent_id: self.id.clone(),
            })
            .await
            .unwrap_or(Err(RuntimeError::Channel))?
        {
            RuntimeResponse::LoopState(state) => Ok(state),
            other => {
                error!(?other, "received unexpected response");
                Err(RuntimeError::Custom("received unexpected response".to_string()))
            },
        }
    }

    /// Sends a new user prompt for the agent to begin executing, returning a receiver that will
    /// receive agent loop events.
    pub async fn send_prompt(
        &self,
        content: Vec<ContentBlock>,
        args: Option<SendPromptArgs>,
    ) -> Result<mpsc::Receiver<AgentLoopEventKind>, RuntimeError> {
        let (tx, rx) = mpsc::channel(16);
        match self
            .sender
            .send_recv(RuntimeRequest::SendPrompt(SendPrompt {
                agent_id: self.id.clone(),
                content,
                args,
                tx: Some(tx),
            }))
            .await
            .unwrap_or(Err(RuntimeError::Channel))?
        {
            RuntimeResponse::Success => Ok(rx),
            other => {
                error!(?other, "received unexpected response");
                Err(RuntimeError::Custom("received unexpected response".to_string()))
            },
        }
    }

    pub async fn interrupt(&self) -> Result<InterruptResult, RuntimeError> {
        match self
            .sender
            .send_recv(RuntimeRequest::Interrupt {
                agent_id: self.id.clone(),
            })
            .await
            .unwrap_or(Err(RuntimeError::Channel))?
        {
            RuntimeResponse::InterruptResult(res) => Ok(res),
            other => {
                error!(?other, "received unexpected response");
                Err(RuntimeError::Custom("received unexpected response".to_string()))
            },
        }
    }

    pub async fn export_agent_state(&self) -> Result<AgentState, RuntimeError> {
        match self
            .sender
            .send_recv(RuntimeRequest::ExportAgentState {
                agent_id: self.id.clone(),
            })
            .await
            .unwrap_or(Err(RuntimeError::Channel))?
        {
            RuntimeResponse::AgentState(res) => Ok(res),
            other => {
                error!(?other, "received unexpected response");
                Err(RuntimeError::Custom("received unexpected response".to_string()))
            },
        }
    }
}

/// A serializable representation of a runtime agent's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Agent identifier
    pub id: AgentId,
    /// System prompt
    pub system_prompt: Option<String>,
    pub conversation_state: ConversationState,
    /// The backend/model provider
    pub model: ModelsState,
}

#[derive(Debug, Clone)]
struct Agent {
    /// Agent identifier
    id: AgentId,
    /// System prompt
    system_prompt: Option<String>,
    conversation_state: ConversationState,
    /// The backend/model provider
    model: Models,
}

impl Agent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Returns the tool specs used for the most recent request.
    fn last_request_tool_specs(&self) -> Option<&[ToolSpec]> {
        self.conversation_state
            .metadata
            .last_request
            .as_ref()
            .and_then(|v| v.tool_specs.as_deref())
    }

    fn set_user_turn_start_request(&mut self, args: SendRequestArgs) {
        self.conversation_state.metadata.user_turn_start_request = Some(args);
    }

    fn set_last_request(&mut self, args: SendRequestArgs) {
        self.conversation_state.metadata.last_request = Some(args);
    }
}

/// State associated with a history of messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationState {
    pub id: Uuid,
    pub messages: Vec<Message>,
    metadata: ConversationMetadata,
}

impl ConversationState {
    /// Creates a new conversation state with a new id and empty history.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            messages: Vec::new(),
            metadata: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationMetadata {
    /// History of user turns
    user_turn_metadatas: Vec<UserTurnMetadata>,
    /// The request that started the most recent user turn
    user_turn_start_request: Option<SendRequestArgs>,
    /// The most recent request sent
    ///
    /// This is equivalent to user_turn_start_request for the first request of a user turn
    last_request: Option<SendRequestArgs>,
}

type AgentLoopFutures = FuturesUnordered<
    Pin<Box<dyn Future<Output = (AgentLoopId, AgentLoopHandle, Option<AgentLoopEventKind>)> + Send + Sync>>,
>;

#[derive(Debug)]
pub struct AgentRuntimeHandle {
    rx: mpsc::Receiver<RuntimeEvent>,
    cancel_token: CancellationToken,
}

impl AgentRuntimeHandle {
    pub async fn recv(&mut self) -> Option<RuntimeEvent> {
        self.rx.recv().await
    }
}

impl Drop for AgentRuntimeHandle {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

/// Main entrypoint to all agent usage. [AgentRuntime] is both a collection of agents and a
/// runtime responsible for polling and receiving agent events.
///
/// *Note*: tool execution is not performed by the runtime and left to consumers to provide to
/// agents as a tool result.
///
/// Conceptually, [AgentRuntime] acts as a separate task that manages agent interactions through a
/// request/response paradigm. Agent interactions are done through an [AgentHandle], a cloneable
/// thread-safe type that enables sending requests to a specific agent.
///
/// Common agent requests may include:
/// - Getting conversation state
/// - Sending a new prompt
/// - Providing tool use results
/// - Cancelling an ongoing response stream
///
/// # Background
///
/// The term "agent" typically refers to some AI that can autonomously reason through a problem
/// using some set of tools.
///
/// Within the context of this app, an **agent** can be generally described as a collection of:
/// - Conversation messages
/// - A system prompt
/// - A model/backend provider
#[derive(Debug)]
pub struct AgentRuntime {
    /// Buffer to hold runtime events
    event_buf: Vec<RuntimeEvent>,

    /// Sender for agent runtime requests.
    ///
    /// Used to create new senders, e.g. for spawned agents.
    runtime_request_tx: RequestSender<RuntimeRequest, RuntimeResponse, RuntimeError>,
    /// Receiver for agent runtime requests.
    runtime_request_rx: RequestReceiver<RuntimeRequest, RuntimeResponse, RuntimeError>,

    /// Map of agent name to state.
    agents: HashMap<AgentId, Agent>,

    /// Currently executing agents.
    ///
    /// Map from an agent name to an agent loop handle, and a channel for sending events back to the
    /// original requester (if available).
    executing_agents: HashMap<
        AgentId,
        (
            AgentLoopId,
            Option<mpsc::Sender<AgentLoopEventKind>>,
            AgentLoopWeakHandle,
        ),
    >,

    /// Collection of executing [AgentLoop] to continually poll for events.
    ///
    /// This can be seen as a set of `"(AgentLoopHandle, NextLoopEvent)"` pairs, where it contains
    /// the next loop event future along with the respective loop handle. Using a single collection
    /// with [FuturesUnordered] enables the runtime to execute multiple agents in parallel and poll
    /// all of them at once.
    agent_loop_futures: AgentLoopFutures,
}

impl AgentRuntime {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(16);
        let tx = RequestSender::new(tx);
        Self {
            event_buf: Vec::new(),
            runtime_request_tx: tx,
            runtime_request_rx: rx,
            agents: HashMap::new(),
            executing_agents: HashMap::new(),
            agent_loop_futures: FuturesUnordered::new(),
        }
    }

    pub fn spawn(self) -> AgentRuntimeHandle {
        let (tx, rx) = mpsc::channel(32);
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();
        tokio::spawn(async move { self.main_loop(tx, token_clone).await });
        AgentRuntimeHandle { rx, cancel_token }
    }

    async fn main_loop(mut self, tx: mpsc::Sender<RuntimeEvent>, cancel_token: CancellationToken) {
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    break;
                },
                res = self.runtime_request_rx.recv() => {
                    let Some(req) = res else {
                        warn!("agent runtime request channel has closed");
                        break;
                    };
                    let res = self.handle_agent_runtime_request(req.payload).await;
                    respond!(req, res);
                },
                res = self.agent_loop_futures.next(), if !self.agent_loop_futures.is_empty() => {
                    if let Some((id, handle, loop_ev)) = res {
                        self.handle_next_agent_loop_event(id, handle, loop_ev).await;
                    }
                }
            }
            for ev in self.event_buf.drain(..) {
                let _ = tx.send(ev).await;
            }
        }
    }

    /// Creates a new [Agent] with a new conversation history.
    pub async fn spawn_agent(
        &mut self,
        agent_id: AgentId,
        system_prompt: Option<String>,
        conversation_state: ConversationState,
        model: Models,
    ) -> Result<AgentHandle, RuntimeError> {
        let sender = self.runtime_request_tx.clone();

        self.agents.contains_key(&agent_id);

        self.agents.insert(agent_id.clone(), Agent {
            id: agent_id.clone(),
            system_prompt,
            conversation_state,
            model,
        });

        Ok(AgentHandle::new(agent_id, sender))
    }

    async fn handle_agent_runtime_request(&mut self, request: RuntimeRequest) -> Result<RuntimeResponse, RuntimeError> {
        debug!(?request, "agent runtime handling request");

        match request {
            RuntimeRequest::SendPrompt(send_prompt) => self.send_prompt(send_prompt).await,
            RuntimeRequest::GetConversationState { agent_id } => {
                let Some(agent_state) = self.agents.get(&agent_id) else {
                    return Err(RuntimeError::AgentNameNotFound { id: agent_id });
                };

                // todo - messages
                Ok(RuntimeResponse::Success)
            },
            RuntimeRequest::Interrupt { agent_id } => self.interrupt(&agent_id).await,
            RuntimeRequest::RetryLastRequest { agent_id } => {
                todo!()
            },
            RuntimeRequest::GetLoopState { agent_id } => match self.executing_agents.get(&agent_id) {
                Some((id, _, handle)) => {
                    let loop_state = handle.get_loop_state().await?;
                    Ok(RuntimeResponse::LoopState(Some((id.clone(), loop_state))))
                },
                None => Ok(RuntimeResponse::LoopState(None)),
            },
            RuntimeRequest::ExportAgentState { agent_id } => {
                let agent = self.get_agent(&agent_id)?;
                let state = AgentState {
                    id: agent.id.clone(),
                    system_prompt: agent.system_prompt.clone(),
                    conversation_state: agent.conversation_state.clone(),
                    model: agent.model.state(),
                };
                Ok(RuntimeResponse::AgentState(state))
            },
        }
    }

    async fn handle_next_agent_loop_event(
        &mut self,
        loop_id: AgentLoopId,
        mut handle: AgentLoopHandle,
        loop_ev: Option<AgentLoopEventKind>,
    ) {
        debug!(?loop_id, ?loop_ev, "agent runtime received a new agent loop event");

        // Check to ensure that the agent loop event we're handling actually corresponds to the
        // currently executing loop.
        //
        // Should never happen, but done as a precautionary check.
        match self.executing_agents.get(loop_id.agent_id()) {
            Some((id, _, _)) if *id != loop_id => {
                error!(
                    %loop_id,
                    agent_id = handle.agent_id().to_string(),
                    "received an agent event for an agent that is not executing"
                );
                return;
            },
            Some(_) => (),
            None => {
                error!(
                    %loop_id,
                    agent_id = handle.agent_id().to_string(),
                    "received an agent event for an agent that is not executing"
                );
                return;
            },
        }

        // If the event is None, then the channel has dropped, meaning the agent loop has exited.
        // Thus, return early.
        let Some(ev) = loop_ev else {
            self.executing_agents.remove(handle.agent_id());
            return;
        };

        let loop_event = AgentLoopEvent::new(handle.id().clone(), ev);

        // First, update agent state if required
        debug_assert!(self.agents.contains_key(handle.agent_id()));
        let Some(agent) = self.agents.get_mut(handle.agent_id()) else {
            error!(
                agent_id = handle.agent_id().to_string(),
                "received an agent event for an agent that does not exist"
            );
            return;
        };

        if let AgentLoopEventKind::ResponseStreamEnd { result, .. } = &loop_event.kind {
            match result {
                Ok(msg) => {
                    agent.conversation_state.messages.push(msg.clone());
                },
                Err(err) => {
                    error!(?err, ?loop_id, "response stream encountered an error");
                    self.handle_loop_error_on_stream_end(&mut handle, err).await;
                },
            }
        }

        self.event_buf.push(RuntimeEvent::AgentLoop(loop_event.clone()));

        // Send the event to the original requester.
        match self.executing_agents.get(handle.agent_id()) {
            Some((_, Some(tx), _)) => {
                let _ = tx.send(loop_event.kind.clone()).await;
            },
            Some(_) => (),
            None => {
                let id = handle.id();
                warn!(?id, "expected agent loop with id to be executing");
            },
        }

        // Insert the next event future.
        self.agent_loop_futures.push(Box::pin(async move {
            let r = handle.recv().await;
            (loop_id, handle, r)
        }));
    }

    async fn handle_loop_error_on_stream_end(&mut self, handle: &mut AgentLoopHandle, loop_err: &LoopError) {
        let agent = self.agents.get_mut(handle.agent_id()).expect("agent exists");
        match loop_err {
            LoopError::InvalidJson {
                assistant_text,
                invalid_tools,
            } => {
                // Historically, we've found the model to produce invalid JSON when
                // handling a complicated tool use - often times, the stream just ends
                // as if everything is ok while in the middle of returning the tool use
                // content.
                //
                // In this case, retry the request, except tell the model to split up
                // the work into simpler tool uses.

                // Create a fake assistant message
                let mut assistant_content = vec![ContentBlock::Text(assistant_text.clone())];
                let val = serde_json::Value::Object(
                    [(
                        "key".to_string(),
                        serde_json::Value::String(
                            "SYSTEM NOTE: the actual tool use arguments were too complicated to be generated"
                                .to_string(),
                        ),
                    )]
                    .into_iter()
                    .collect(),
                );
                assistant_content.append(
                    &mut invalid_tools
                        .iter()
                        .map(|v| {
                            ContentBlock::ToolUse(ToolUseBlock {
                                tool_use_id: v.tool_use_id.clone(),
                                name: v.name.clone(),
                                input: val.clone(),
                            })
                        })
                        .collect(),
                );
                agent.conversation_state.messages.push(Message {
                    id: None,
                    role: Role::Assistant,
                    content: assistant_content,
                    timestamp: Some(Utc::now()),
                });

                agent.conversation_state.messages.push(Message {
                        id: None,
                        role: Role::User,
                        content: vec![ContentBlock::Text(
                            "The generated tool was too large, try again but this time split up the work between multiple tool uses"
                                .to_string(),
                        )],
                        timestamp: Some(Utc::now()),
                    });

                let tool_specs = agent.last_request_tool_specs().map(|v| v.to_vec());
                let request_args = SendRequestArgs::new(
                    agent.conversation_state.messages.clone(),
                    tool_specs,
                    agent.system_prompt().map(String::from),
                );
                agent.set_last_request(request_args.clone());
                handle
                    .send_request(agent.model.clone(), request_args)
                    .await
                    .expect("request should not fail");
            },
            LoopError::Stream(stream_err) => match &stream_err.kind {
                StreamErrorKind::StreamTimeout { .. } => {
                    agent.conversation_state.messages.push(Message {
                        id: None,
                        role: Role::Assistant,
                        content: vec![ContentBlock::Text(
                            "Response timed out - message took too long to generate".to_string(),
                        )],
                        timestamp: Some(Utc::now()),
                    });
                    agent.conversation_state.messages.push(Message {
                        id: None,
                        role: Role::User,
                        content: vec![ContentBlock::Text(
                            "You took too long to respond - try to split up the work into smaller steps.".to_string(),
                        )],
                        timestamp: Some(Utc::now()),
                    });
                    let tool_specs = agent.last_request_tool_specs().map(|v| v.to_vec());
                    let request_args = SendRequestArgs::new(
                        agent.conversation_state.messages.clone(),
                        tool_specs,
                        agent.system_prompt().map(String::from),
                    );
                    agent.set_last_request(request_args.clone());
                    handle
                        .send_request(agent.model.clone(), request_args)
                        .await
                        .expect("request should not fail");
                },
                StreamErrorKind::Interrupted => {
                    // close the loop
                },
                StreamErrorKind::Validation { .. }
                | StreamErrorKind::ServiceFailure
                | StreamErrorKind::Throttling
                | StreamErrorKind::ContextWindowOverflow
                | StreamErrorKind::Other(_) => {
                    // todo!()
                    self.event_buf.push(RuntimeEvent::AgentLoopError {
                        id: handle.id().clone(),
                        error: loop_err.clone(),
                    });
                },
            },
        }
    }

    fn get_agent(&self, agent_id: &AgentId) -> Result<&Agent, RuntimeError> {
        match self.agents.get(agent_id) {
            Some(agent) => Ok(agent),
            None => Err(RuntimeError::AgentNameNotFound { id: agent_id.clone() }),
        }
    }

    fn get_agent_mut(&mut self, agent_id: &AgentId) -> Result<&mut Agent, RuntimeError> {
        match self.agents.get_mut(agent_id) {
            Some(agent) => Ok(agent),
            None => Err(RuntimeError::AgentNameNotFound { id: agent_id.clone() }),
        }
    }

    async fn get_execution_state(&self, agent_id: &AgentId) -> Result<Option<LoopState>, RuntimeError> {
        match self.executing_agents.get(agent_id) {
            Some((_, _, handle)) => Ok(Some(handle.get_loop_state().await?)),
            None => Ok(None),
        }
    }

    fn get_executing_agent(
        &self,
        agent_id: &AgentId,
    ) -> Result<
        &(
            AgentLoopId,
            Option<mpsc::Sender<AgentLoopEventKind>>,
            AgentLoopWeakHandle,
        ),
        RuntimeError,
    > {
        self.executing_agents
            .get(agent_id)
            .ok_or(RuntimeError::AgentNameNotFound { id: agent_id.clone() })
    }

    /// Handles a [RuntimeRequest::SendPrompt].
    async fn send_prompt(&mut self, prompt: SendPrompt) -> Result<RuntimeResponse, RuntimeError> {
        let agent_id = &prompt.agent_id;
        let mut tool_specs = prompt.tool_specs().unwrap_or_default().to_vec();
        let is_retry = prompt.is_retry();

        // Check if the agent is in a valid state for handling the next prompt, creating a new
        // agent loop if required.
        let new_user_turn = match self.get_execution_state(agent_id).await? {
            Some(state) => {
                let (_, _, h) = self.executing_agents.get(agent_id).expect("agent exists");
                match state {
                    // Loop somehow never did any work - this state should never happen.
                    LoopState::Idle => true,
                    // Nothing to do.
                    LoopState::UserTurnEnded => true,
                    loop_state @ LoopState::PendingToolUseResults => {
                        // debug assertion check
                        {
                            let last_msg = self.get_agent(agent_id)?.conversation_state.messages.last();
                            debug_assert!(
                                last_msg.is_some_and(|m| m.role == Role::Assistant && m.tool_uses().is_some()),
                                "loop state: {} should have the last message in the history be from the assistant with tool uses: {:?}",
                                loop_state,
                                last_msg,
                            );
                        }

                        // If the next prompt does not contain results for all of the pending tool
                        // uses, then a new agent loop will be created.
                        let pending_tool_use_ids: HashSet<_> = h
                            .get_pending_tool_uses()
                            .await?
                            .into_iter()
                            .flat_map(|v| v.into_iter().map(|t| t.tool_use_id))
                            .collect();
                        let prompt_tool_results = &prompt
                            .content
                            .iter()
                            .filter_map(|v| match v {
                                ContentBlock::ToolResult(block) => Some(block.tool_use_id.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        let is_tool_use_result = prompt_tool_results.iter().all(|id| pending_tool_use_ids.contains(id));
                        if !is_tool_use_result {
                            debug!(
                                ?pending_tool_use_ids,
                                ?prompt_tool_results,
                                is_tool_use_result,
                                "prompt does not contain tool results, creating a new user turn"
                            );
                            match h.close().await {
                                Ok(_) => (),
                                Err(err) => {
                                    error!(?err, "failed to close the current agent loop");
                                },
                            }
                            true
                        } else {
                            debug!(
                                ?pending_tool_use_ids,
                                ?prompt_tool_results,
                                is_tool_use_result,
                                "prompt contains tool results, continuing the user turn"
                            );
                            false
                        }
                    },
                    LoopState::Errored => {
                        if !is_retry {
                            // Don't error out here if for some unknown reason the loop fails to
                            // close successfully - a new loop will be created immediately
                            // afterwards.
                            match h.close().await {
                                Ok(_) => (),
                                Err(err) => {
                                    error!(?err, "failed to close the current agent loop");
                                },
                            }
                            true
                        } else {
                            false
                        }
                    },
                    LoopState::SendingRequest | LoopState::ConsumingResponse => {
                        error!(?state, "cannot send prompt to an agent that is not idle");
                        return Err(RuntimeError::AgentNotIdle { id: agent_id.clone() });
                    },
                }
            },
            // If the agent isn't executing, then we need to create a new agent loop.
            None => true,
        };

        // Update agent state with the next message to send
        let Some(agent) = self.agents.get_mut(agent_id) else {
            return Err(RuntimeError::AgentNameNotFound { id: agent_id.clone() });
        };

        agent
            .conversation_state
            .messages
            .push(Message::new(Role::User, prompt.content.clone(), Some(Utc::now())));

        let mut messages = VecDeque::from(agent.conversation_state.messages.clone());
        enforce_conversation_invariants(&mut messages, &mut tool_specs);

        // Send the message
        if new_user_turn {
            let request_args = SendRequestArgs::new(
                agent.conversation_state.messages.clone(),
                Some(tool_specs),
                agent.system_prompt().map(String::from),
            );
            agent.set_user_turn_start_request(request_args.clone());
            agent.set_last_request(request_args.clone());

            // Create a new agent loop, and send the request.
            let cancel_token = CancellationToken::new();
            let loop_id = AgentLoopId::new(agent_id.clone());
            let mut handle = AgentLoop::new(loop_id.clone(), cancel_token).spawn();
            handle
                .send_request(agent.model.clone(), request_args)
                .await
                .expect("first agent loop request should never fail");

            self.executing_agents
                .insert(agent_id.clone(), (loop_id.clone(), prompt.tx, handle.clone_weak()));
            self.agent_loop_futures.push(Box::pin(async move {
                let r = handle.recv().await;
                (loop_id, handle, r)
            }));
        } else {
            let request_args = SendRequestArgs::new(
                agent.conversation_state.messages.clone(),
                Some(tool_specs),
                agent.system_prompt().map(String::from),
            );
            agent.set_last_request(request_args.clone());
            let (_, _, h) = self.executing_agents.get(agent_id).expect("agent exists");
            h.send_request(agent.model.clone(), request_args)
                .await
                .expect("should not fail");
        }

        Ok(RuntimeResponse::Success)
    }

    /// Handles a [RuntimeRequest::Interrupt].
    async fn interrupt(&mut self, agent_id: &AgentId) -> Result<RuntimeResponse, RuntimeError> {
        match self.get_execution_state(agent_id).await? {
            Some(state) => match state {
                loop_state @ (LoopState::SendingRequest | LoopState::ConsumingResponse) => {
                    let (_, _, h) = self.get_executing_agent(agent_id)?;
                    let md = h.close().await?;
                    Ok(RuntimeResponse::InterruptResult(Some((loop_state, md))))
                },
                loop_state @ LoopState::PendingToolUseResults => {
                    // if the agent is in the middle of sending tool uses, then add two new
                    // messages:
                    // 1. user tool results replaced with content: "Tool use was cancelled by the user"
                    // 2. assistant message with content: "Tool uses were interrupted, waiting for the next user prompt"
                    let (_, _, h) = self.get_executing_agent(agent_id)?;
                    let md = h.close().await?;
                    let agent = self.get_agent_mut(agent_id)?;
                    let tool_results = agent
                        .conversation_state
                        .messages
                        .last()
                        .iter()
                        .flat_map(|m| {
                            m.content.iter().filter_map(|c| match c {
                                ContentBlock::ToolUse(tool_use) => Some(ContentBlock::ToolResult(ToolResultBlock {
                                    tool_use_id: tool_use.tool_use_id.clone(),
                                    content: vec![ToolResultContentBlock::Text(
                                        "Tool use was cancelled by the user".to_string(),
                                    )],
                                    status: ToolResultStatus::Error,
                                })),
                                _ => None,
                            })
                        })
                        .collect::<Vec<_>>();
                    agent
                        .conversation_state
                        .messages
                        .push(Message::new(Role::User, tool_results, Some(Utc::now())));
                    agent.conversation_state.messages.push(Message::new(
                        Role::Assistant,
                        vec![ContentBlock::Text(
                            "Tool uses were interrupted, waiting for the next user prompt".to_string(),
                        )],
                        Some(Utc::now()),
                    ));
                    Ok(RuntimeResponse::InterruptResult(Some((loop_state, md))))
                },
                LoopState::Idle | LoopState::UserTurnEnded | LoopState::Errored => {
                    Ok(RuntimeResponse::InterruptResult(None))
                },
            },
            None => Ok(RuntimeResponse::InterruptResult(None)),
        }
    }
}

/// Updates the history so that, when non-empty, the following invariants are in place:
/// - The history length is `<= MAX_CONVERSATION_STATE_HISTORY_LEN`. Oldest messages are dropped.
/// - Any tool uses that do not exist in the provided tool specs will have their arguments replaced
///   with dummy content.
fn enforce_conversation_invariants(messages: &mut VecDeque<Message>, tools: &mut Vec<ToolSpec>) {
    // First, trim the conversation history by finding the second oldest message from the user without
    // tool results - this will be the new oldest message in the history.
    //
    // Note that we reserve extra slots for context messages.
    const MAX_HISTORY_LEN: usize = MAX_CONVERSATION_STATE_HISTORY_LEN - 2;
    let need_to_trim_front = messages
        .front()
        .is_none_or(|m| !(m.role == Role::User && m.tool_results().is_none()))
        || messages.len() > MAX_HISTORY_LEN;
    if need_to_trim_front {
        match messages
            .iter()
            .enumerate()
            .find(|(i, v)| (messages.len() - i) < MAX_HISTORY_LEN && v.role == Role::User && v.tool_results().is_none())
        {
            Some((i, m)) => {
                trace!(i, ?m, "found valid starting user message with no tool results");
                messages.drain(0..i);
            },
            None => {
                trace!("no valid starting user message found in the history, clearing");
                messages.clear();
                return;
            },
        }
    }

    // Replace any missing tool use references with a dummy tool spec.
    let tool_names: HashSet<_> = tools.iter().map(|t| t.name.clone()).collect();
    let mut insert_dummy_spec = false;
    for msg in messages {
        for block in &mut msg.content {
            if let ContentBlock::ToolUse(v) = block {
                if !tool_names.contains(&v.name) {
                    v.name = DUMMY_TOOL_NAME.to_string();
                    insert_dummy_spec = true;
                }
            }
        }
    }
    if insert_dummy_spec {
        tools.push(ToolSpec {
            name: DUMMY_TOOL_NAME.to_string(),
            description: "This is a dummy tool. If you are seeing this that means the tool associated with this tool call is not in the list of available tools. This could be because a wrong tool name was supplied or the list of tools has changed since the conversation has started. Do not show this when user asks you to list tools.".to_string(),
            input_schema: serde_json::from_str(r#"{"type": "object", "properties": {}, "required": [] }"#).unwrap(),
        });
    }
}

/// Arguments to the [RuntimeRequest::SendPrompt] request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPrompt {
    /// Id of the agent
    agent_id: AgentId,
    /// The prompt to submit
    content: Vec<ContentBlock>,
    /// Additional optional arguments
    args: Option<SendPromptArgs>,
    /// Sender for sending agent events back to the requester
    ///
    /// If provided, the runtime will send all agent-specific events using this channel
    #[serde(skip)]
    tx: Option<mpsc::Sender<AgentLoopEventKind>>,
}

impl SendPrompt {
    pub fn tool_specs(&self) -> Option<&[ToolSpec]> {
        self.args.as_ref().map(|v| v.tool_specs.as_slice())
    }

    pub fn is_retry(&self) -> bool {
        self.args.as_ref().map(|v| v.is_retry).unwrap_or_default()
    }
}

/// Optional arguments to [SendPrompt].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendPromptArgs {
    /// Tool specs to include as part of the request
    pub tool_specs: Vec<ToolSpec>,
    /// Context entries
    ///
    /// Each context entry will be included at the start of the conversation inside special
    /// faked messages called **context messages**.
    pub context_entries: Vec<String>,
    /// Runtime-evaluated context entries
    ///
    /// TODO - make deserialize compatible somehow?
    /// TODO - is this going to be required? this is only needed if we want to have dynamic context
    /// entries for retry requests, which is unlikely.
    #[serde(skip)]
    pub context_providers: Vec<Arc<dyn ContextProvider>>,
    /// Whether or not this prompt is retrying a failure state
    pub is_retry: bool,
}

pub trait ContextProvider: std::fmt::Debug + Send + Sync {
    fn provide(&self) -> Pin<Box<dyn Future<Output = String> + Send + '_>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeRequest {
    /// Send a new prompt
    SendPrompt(SendPrompt),
    /// Retry the last request for a given agent
    RetryLastRequest {
        agent_id: AgentId,
    },
    /// Get an agent's conversation state (messages, summary, etc.)
    GetConversationState {
        agent_id: AgentId,
    },
    /// Get the current execution state of an agent
    GetLoopState {
        agent_id: AgentId,
    },
    /// Cancels an executing agent, otherwise does nothing.
    ///
    /// This will always end a user turn if the agent is currently executing.
    Interrupt {
        agent_id: AgentId,
    },
    ExportAgentState {
        agent_id: AgentId,
    },
}

/// Successful response for agent runtime requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeResponse {
    /// Generic success response containing no data
    Success,
    /// Result of a [RuntimeRequest::Interrupt].
    ///
    /// Contains the state the agent was in, along with the turn metadata if the interrupt stopped
    /// an executing agent.
    ///
    /// Essentially: only [Some] if the interrupt actually did anything meaningful.
    InterruptResult(InterruptResult),
    LoopState(Option<(AgentLoopId, LoopState)>),
    Messages(Vec<Message>),
    AgentState(AgentState),
}

type InterruptResult = Option<(LoopState, UserTurnMetadata)>;

/// Error response for agent runtime requests
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum RuntimeError {
    #[error("No agent exists with the id: '{}'", .id)]
    AgentNameNotFound { id: AgentId },
    #[error("Agent with the name: '{}' is not idle", .id)]
    AgentNotIdle { id: AgentId },
    #[error("Agent with the name: '{}' already exists", .id)]
    AgentAlreadyExists { id: AgentId },
    #[error("A failure occurred with the underlying channel")]
    Channel,
    #[error("{}", .0)]
    AgentLoop(#[from] AgentLoopResponseError),
    #[error("{}", .0)]
    Custom(String),
}

impl<T> From<mpsc::error::SendError<T>> for RuntimeError {
    fn from(value: mpsc::error::SendError<T>) -> Self {
        Self::Custom(format!("channel failure: {}", value))
    }
}

/// The supporte
#[derive(Debug, Clone)]
pub enum Models {
    Rts(RtsModel),
    Test(TestModel),
}

impl Models {
    pub fn supported_model(&self) -> SupportedModel {
        match self {
            Models::Rts(_) => SupportedModel::Rts,
            Models::Test(_) => SupportedModel::Test,
        }
    }

    pub fn state(&self) -> ModelsState {
        match self {
            Models::Rts(v) => ModelsState::Rts {
                conversation_id: Some(v.conversation_id().to_string()),
                model_id: v.model_id().map(String::from),
            },
            Models::Test(_) => ModelsState::Test,
        }
    }
}

/// Identifier for the models we support.
///
/// TODO - probably not required, use [ModelsState] instead
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum SupportedModel {
    Rts,
    Test,
}

impl agent_loop::Model for Models {
    fn stream(
        &self,
        messages: Vec<Message>,
        tool_specs: Option<Vec<ToolSpec>>,
        system_prompt: Option<String>,
        cancel_token: CancellationToken,
    ) -> Pin<Box<dyn Stream<Item = Result<agent_loop::StreamEvent, agent_loop::StreamError>> + Send + 'static>> {
        match self {
            Models::Rts(rts_model) => rts_model.stream(messages, tool_specs, system_prompt, cancel_token),
            Models::Test(test_model) => todo!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestModel {}

impl TestModel {
    pub fn new() -> Self {
        Self {}
    }
}

/// A serializable representation of the state contained within [Models].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelsState {
    Rts {
        conversation_id: Option<String>,
        model_id: Option<String>,
    },
    Test,
}

impl Default for ModelsState {
    fn default() -> Self {
        Self::Rts {
            conversation_id: None,
            model_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum RuntimeEvent {
    /// An agent was spawned
    AgentSpawn {
        id: AgentId,
        system_prompt: String,
        conversation_state: Option<ConversationState>,
    },
    AgentLoop(AgentLoopEvent),
    /// An error occurred while executing the agent loop that could not be handled.
    ///
    /// This variant contains errors returned by [AgentLoopEventKind::ResponseStreamEnd] where
    /// the result ended in [Err] and the runtime was unable to handle it.
    AgentLoopError {
        /// Id of the agent loop
        id: AgentLoopId,
        /// The error that occurred
        error: LoopError,
    },
}

impl RuntimeEvent {
    /// Returns the [AgentId] for the associated event
    pub fn agent_id(&self) -> &AgentId {
        match self {
            RuntimeEvent::AgentSpawn { id, .. } => &id,
            RuntimeEvent::AgentLoop(ev) => ev.agent_id(),
            RuntimeEvent::AgentLoopError { id, .. } => id.agent_id(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::time::Duration;

    use super::types::*;
    use super::*;
    use crate::chat::runtime::agent_loop::StreamEvent;

    macro_rules! test_ser_deser {
        ($ty:ident, $variant:expr, $text:expr) => {
            let quoted = format!("\"{}\"", $text);
            assert_eq!(quoted, serde_json::to_string(&$variant).unwrap());
            assert_eq!($variant, serde_json::from_str(&quoted).unwrap());
            assert_eq!($variant, $ty::from_str($text).unwrap());
            assert_eq!($text, $variant.to_string());
        };
    }

    #[test]
    fn test_supported_models_ser_deser() {
        test_ser_deser!(SupportedModel, SupportedModel::Rts, "rts");
        test_ser_deser!(SupportedModel, SupportedModel::Test, "test");
    }

    #[test]
    fn test_stub_response() {
        let msgs = vec![
            StreamEvent::MessageStart(MessageStartEvent { role: Role::Assistant }),
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                content_block_start: None,
                content_block_index: None,
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::Text("hello".into()),
                content_block_index: None,
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                content_block_index: None,
            }),
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                content_block_start: Some(ContentBlockStart::ToolUse(ToolUseBlockStart {
                    tool_use_id: "893581".into(),
                    name: "fs_read".into(),
                })),
                content_block_index: None,
            }),
            StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                delta: ContentBlockDelta::ToolUse(ToolUseBlockDelta {
                    input: r#"{"operations":[{"mode":"Line","path":"/test_file.txt","start_line":null}]}"#.into(),
                }),
                content_block_index: None,
            }),
            StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                content_block_index: None,
            }),
            StreamEvent::MessageStop(MessageStopEvent {
                stop_reason: StopReason::ToolUse,
            }),
            StreamEvent::Metadata(MetadataEvent {
                metrics: Some(MetadataMetrics {
                    time_to_first_chunk: Some(Duration::from_millis(1500)),
                    time_between_chunks: Some(vec![
                        Duration::from_millis(23),
                        Duration::from_millis(4),
                        Duration::from_millis(5),
                        Duration::from_millis(1),
                    ]),
                    response_stream_len: 250,
                }),
                usage: None,
                service: None,
            }),
        ];

        let out = serde_json::to_string_pretty(&msgs).unwrap();
        println!("{}\n\n", out);
    }
}
