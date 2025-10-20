use std::pin::Pin;

use futures::Stream;
use serde::{
    Deserialize,
    Serialize,
};
use tokio_util::sync::CancellationToken;

use super::protocol::StreamResult;
use super::types::{
    Message,
    ToolSpec,
};
use crate::agent::rts::RtsModel;

/// Represents a backend implementation for a converse stream compatible API.
///
/// **Important** - implementations should be cancel safe
pub trait Model: std::fmt::Debug + Send + Sync + 'static {
    /// Sends a conversation to a model, returning a stream of events as the response.
    fn stream(
        &self,
        messages: Vec<Message>,
        tool_specs: Option<Vec<ToolSpec>>,
        system_prompt: Option<String>,
        cancel_token: CancellationToken,
    ) -> Pin<Box<dyn Stream<Item = StreamResult> + Send + 'static>>;

    /// Dump serializable state required by the model implementation.
    ///
    /// This is intended to provide the ability to save and restore state
    /// associated with an implementation, useful for restoring a previous conversation.
    fn state(&self) -> Option<serde_json::Value> {
        None
    }
}

/// The supported backends
#[derive(Debug, Clone)]
pub enum Models {
    Rts(RtsModel),
    Test(TestModel),
}

impl Models {
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

impl Model for Models {
    fn stream(
        &self,
        messages: Vec<Message>,
        tool_specs: Option<Vec<ToolSpec>>,
        system_prompt: Option<String>,
        cancel_token: CancellationToken,
    ) -> Pin<Box<dyn Stream<Item = StreamResult> + Send + 'static>> {
        match self {
            Models::Rts(rts_model) => rts_model.stream(messages, tool_specs, system_prompt, cancel_token),
            Models::Test(test_model) => test_model.stream(messages, tool_specs, system_prompt, cancel_token),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TestModel {}

impl TestModel {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Model for TestModel {
    fn stream(
        &self,
        _messages: Vec<Message>,
        _tool_specs: Option<Vec<ToolSpec>>,
        _system_prompt: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Pin<Box<dyn Stream<Item = StreamResult> + Send + 'static>> {
        panic!("unimplemented")
    }
}
