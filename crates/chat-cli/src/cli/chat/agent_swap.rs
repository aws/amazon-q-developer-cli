use std::sync::{
    Arc,
    Mutex,
};

use crate::constants::{
    DEFAULT_AGENT_NAME,
    PLANNER_AGENT_NAME,
    PLANNER_WELCOME_MESSAGE,
};

/// Shared state for agent swap operations
#[derive(Clone, Debug, Default)]
pub struct AgentSwapState {
    inner: Arc<Mutex<AgentSwapStateInner>>,
}

#[derive(Debug, Default)]
struct AgentSwapStateInner {
    pending_swap: Option<String>,
    pending_prompt: Option<String>,
    pending_message: Option<String>,
    current_agent: String,
    previous_agent: Option<String>,
}

impl AgentSwapState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_current_agent(&self, agent_name: String) {
        self.inner.lock().unwrap().current_agent = agent_name;
    }

    pub fn get_current_agent(&self) -> String {
        self.inner.lock().unwrap().current_agent.clone()
    }

    /// Toggle between planner and previous agent.
    /// Sets all necessary pending fields in one atomic operation:
    pub fn planner_toggle(&self, prompt: Option<String>) {
        let mut inner = self.inner.lock().unwrap();
        inner.pending_prompt = prompt;

        if inner.current_agent == PLANNER_AGENT_NAME {
            // From planner: go back to previous agent
            let target = inner
                .previous_agent
                .clone()
                .unwrap_or_else(|| DEFAULT_AGENT_NAME.to_string());
            inner.pending_swap = Some(target);
        } else {
            // To planner: save current as previous, set welcome message
            inner.previous_agent = Some(inner.current_agent.clone());
            inner.pending_swap = Some(PLANNER_AGENT_NAME.to_string());
            inner.pending_message = Some(PLANNER_WELCOME_MESSAGE.to_string());
        }
    }

    pub fn take_pending_swap(&self) -> Option<String> {
        self.inner.lock().unwrap().pending_swap.take()
    }

    pub fn take_pending_prompt(&self) -> Option<String> {
        self.inner.lock().unwrap().pending_prompt.take()
    }

    pub fn take_pending_message(&self) -> Option<String> {
        self.inner.lock().unwrap().pending_message.take()
    }
}
