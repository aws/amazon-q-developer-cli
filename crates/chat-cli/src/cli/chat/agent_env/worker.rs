use std::sync::{Arc, Mutex};
use uuid::Uuid;

use super::model_providers::BedrockConverseStreamModelProvider;
use super::worker_interface::WorkerToHostInterface;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerStates {
    Inactive,
    Working,
    Requesting,
    Receiving,
    Waiting,
    UsingTool,
    InactiveFailed,
}

pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub model_provider: BedrockConverseStreamModelProvider,
    pub state: Arc<Mutex<WorkerStates>>,
    pub last_failure: Arc<Mutex<Option<String>>>,
}

impl Worker {
    pub fn new(name: String, model_provider: BedrockConverseStreamModelProvider) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            model_provider,
            state: Arc::new(Mutex::new(WorkerStates::Inactive)),
            last_failure: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_state(&self, new_state: WorkerStates, interface: &dyn WorkerToHostInterface) {
        {
            let mut state = self.state.lock().unwrap();
            *state = new_state;
        }
        interface.worker_state_change(self.id, new_state);
    }

    pub fn get_state(&self) -> WorkerStates {
        *self.state.lock().unwrap()
    }

    pub fn set_failure(&self, error: String) {
        let mut failure = self.last_failure.lock().unwrap();
        *failure = Some(error);
    }

    pub fn get_failure(&self) -> Option<String> {
        self.last_failure.lock().unwrap().clone()
    }
}
