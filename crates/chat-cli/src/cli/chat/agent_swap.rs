use std::sync::{
    Arc,
    Mutex,
};

use crate::constants::{
    DEFAULT_AGENT_NAME,
    PLANNER_AGENT_NAME,
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

    pub fn toggle_to_planner(&self) -> String {
        let mut inner = self.inner.lock().unwrap();
        if inner.current_agent != PLANNER_AGENT_NAME {
            inner.previous_agent = Some(inner.current_agent.clone());
        }
        inner.current_agent = PLANNER_AGENT_NAME.to_string();
        PLANNER_AGENT_NAME.to_string()
    }

    pub fn toggle_from_planner(&self) -> String {
        let mut inner = self.inner.lock().unwrap();
        let target = inner
            .previous_agent
            .clone()
            .unwrap_or_else(|| DEFAULT_AGENT_NAME.to_string());
        inner.current_agent = target.clone();
        target
    }

    pub fn toggle(&self) -> String {
        let current = self.get_current_agent();
        if current == PLANNER_AGENT_NAME {
            self.toggle_from_planner()
        } else {
            self.toggle_to_planner()
        }
    }

    pub fn set_pending_swap(&self, agent_name: String) {
        self.inner.lock().unwrap().pending_swap = Some(agent_name);
    }

    pub fn take_pending_swap(&self) -> Option<String> {
        self.inner.lock().unwrap().pending_swap.take()
    }

    pub fn take_pending_prompt(&self) -> Option<String> {
        self.inner.lock().unwrap().pending_prompt.take()
    }

    pub fn set_previous_agent(&self, agent_name: String) {
        self.inner.lock().unwrap().previous_agent = Some(agent_name);
    }
}
