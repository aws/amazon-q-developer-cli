use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use super::conversation_history::ConversationHistory;

fn default_agent_prompt() -> Arc<Mutex<Option<String>>> {
    Arc::new(Mutex::new(None))
}

fn default_resource_references() -> Arc<Mutex<Vec<String>>> {
    Arc::new(Mutex::new(Vec::new()))
}

/// Container for all contextual information available to a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextContainer {
    #[serde(skip)]
    pub conversation_history: Arc<Mutex<ConversationHistory>>,
    
    #[serde(skip, default = "default_agent_prompt")]
    pub agent_prompt: Arc<Mutex<Option<String>>>,
    
    /// Resource file paths (file:// URLs) to be loaded when building ModelRequest
    #[serde(skip, default = "default_resource_references")]
    pub resource_references: Arc<Mutex<Vec<String>>>,
}

impl ContextContainer {
    pub fn new() -> Self {
        Self {
            conversation_history: Arc::new(Mutex::new(ConversationHistory::new())),
            agent_prompt: Arc::new(Mutex::new(None)),
            resource_references: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub fn set_agent_prompt(&self, prompt: String) {
        *self.agent_prompt.lock().unwrap() = Some(prompt);
    }
    
    pub fn get_agent_prompt(&self) -> Option<String> {
        self.agent_prompt.lock().unwrap().clone()
    }
    
    pub fn set_resource_references(&self, references: Vec<String>) {
        *self.resource_references.lock().unwrap() = references;
    }
    
    pub fn get_resource_references(&self) -> Vec<String> {
        self.resource_references.lock().unwrap().clone()
    }
}

impl Default for ContextContainer {
    fn default() -> Self {
        Self::new()
    }
}
