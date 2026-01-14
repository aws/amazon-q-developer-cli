use std::sync::{
    Arc,
    Mutex,
};

use crate::constants::DEFAULT_AGENT_NAME;

/// Shared state for agent swap operations
#[derive(Clone, Debug, Default)]
pub struct AgentSwapState {
    inner: Arc<Mutex<AgentSwapStateInner>>,
}

#[derive(Debug, Default)]
struct AgentSwapStateInner {
    pending_swap: Option<String>,
    pending_prompt: Option<String>,
    pending_message: Option<(String, String)>, // (agent_name, welcome_message)
    current_agent: String,
    previous_agent: Option<String>,
}

impl AgentSwapState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_current_agent(&self, agent_name: String) {
        let mut inner = self.inner.lock().unwrap();
        // Track previous agent for toggle-back (only if actually changing)
        if !inner.current_agent.is_empty() && inner.current_agent != agent_name {
            inner.previous_agent = Some(inner.current_agent.clone());
        }
        inner.current_agent = agent_name;
    }

    pub fn get_current_agent(&self) -> String {
        self.inner.lock().unwrap().current_agent.clone()
    }

    /// Generic trigger swap for any agent with keyboard shortcut.
    /// If already ON target_agent: swaps back to previous agent.
    /// Otherwise: swaps to target_agent
    pub fn trigger_swap(&self, target_agent: &str, welcome_message: Option<String>, prompt: Option<String>) {
        let mut inner = self.inner.lock().unwrap();
        inner.pending_prompt = prompt;

        if inner.current_agent == target_agent {
            // Already on target agent: go back to previous
            let target = inner
                .previous_agent
                .clone()
                .unwrap_or_else(|| DEFAULT_AGENT_NAME.to_string());
            inner.pending_swap = Some(target);
        } else {
            // Swap to target agent
            inner.pending_swap = Some(target_agent.to_string());
            inner.pending_message = welcome_message.map(|msg| (target_agent.to_string(), msg));
        }
    }

    /// Swap back to the previous agent unconditionally.
    pub fn toggle_to_previous_agent(&self, prompt: Option<String>) {
        let mut inner = self.inner.lock().unwrap();
        inner.pending_prompt = prompt;
        let target = inner
            .previous_agent
            .clone()
            .unwrap_or_else(|| DEFAULT_AGENT_NAME.to_string());
        inner.pending_swap = Some(target);
    }

    pub fn take_pending_swap(&self) -> Option<String> {
        self.inner.lock().unwrap().pending_swap.take()
    }

    pub fn take_pending_message(&self) -> Option<(String, String)> {
        self.inner.lock().unwrap().pending_message.take()
    }

    pub fn take_pending_prompt(&self) -> Option<String> {
        self.inner.lock().unwrap().pending_prompt.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_swap_toggle() {
        let state = AgentSwapState::new();
        state.set_current_agent("default".to_string());

        // First swap: default -> planner
        state.trigger_swap("planner", Some("Welcome".to_string()), None);
        assert_eq!(state.take_pending_swap(), Some("planner".to_string()));

        // Simulate swap completion
        state.set_current_agent("planner".to_string());

        // Toggle back: planner -> default
        state.trigger_swap("planner", Some("Welcome".to_string()), None);
        assert_eq!(state.take_pending_swap(), Some("default".to_string()));
    }

    #[test]
    fn test_toggle_to_previous_agent() {
        let state = AgentSwapState::new();
        state.set_current_agent("default".to_string());
        state.set_current_agent("planner".to_string());

        // Toggle back unconditionally
        state.toggle_to_previous_agent(Some("prompt".to_string()));
        assert_eq!(state.take_pending_swap(), Some("default".to_string()));
        assert_eq!(state.take_pending_prompt(), Some("prompt".to_string()));
    }
}
