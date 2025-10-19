use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use serde::{Serialize, Deserialize};

use super::context_container::ContextContainer;
use super::model_providers::ModelProvider;
use super::events::WorkerLifecycleState;
use crate::os::Os;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum WorkerStates {
    #[default]
    Inactive,
    Working,
    Requesting,
    Receiving,
    Waiting,
    UsingTool,
    InactiveFailed,
}

#[derive(Serialize, Deserialize)]
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub context_container: ContextContainer,
    
    /// Lifecycle state (managed by Session)
    #[serde(skip, default = "default_lifecycle_state")]
    pub lifecycle_state: Arc<Mutex<WorkerLifecycleState>>,
    
    /// Task-specific metadata (managed by Tasks)
    #[serde(skip, default = "default_task_metadata")]
    pub task_metadata: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    
    /// Non-serializable runtime dependencies
    #[serde(skip, default)]
    pub model_provider: Option<Arc<dyn ModelProvider>>,
    
    #[serde(skip, default = "default_os")]
    pub os: Arc<Mutex<Option<Arc<Os>>>>,
    
    /// Legacy state tracking (to be removed)
    #[serde(skip, default = "default_worker_state")]
    pub state: Arc<Mutex<WorkerStates>>,
    #[serde(skip, default)]
    pub last_failure: Arc<Mutex<Option<String>>>,
}

fn default_lifecycle_state() -> Arc<Mutex<WorkerLifecycleState>> {
    Arc::new(Mutex::new(WorkerLifecycleState::Idle))
}

fn default_task_metadata() -> Arc<Mutex<HashMap<String, serde_json::Value>>> {
    Arc::new(Mutex::new(HashMap::new()))
}

fn default_worker_state() -> Arc<Mutex<WorkerStates>> {
    Arc::new(Mutex::new(WorkerStates::Inactive))
}

fn default_os() -> Arc<Mutex<Option<Arc<Os>>>> {
    Arc::new(Mutex::new(None))
}

impl Worker {
    pub fn new(name: String, model_provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            context_container: ContextContainer::new(),
            lifecycle_state: Arc::new(Mutex::new(WorkerLifecycleState::Idle)),
            task_metadata: Arc::new(Mutex::new(HashMap::new())),
            model_provider: Some(model_provider),
            os: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(WorkerStates::Inactive)),
            last_failure: Arc::new(Mutex::new(None)),
        }
    }
    
    pub fn set_os(&self, os: Arc<Os>) {
        *self.os.lock().unwrap() = Some(os);
    }
    
    pub fn get_os(&self) -> Option<Arc<Os>> {
        self.os.lock().unwrap().clone()
    }

    pub fn set_state(&self, new_state: WorkerStates) {
        let mut state = self.state.lock().unwrap();
        *state = new_state;
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
    
    /// Set task-specific metadata
    pub fn set_task_metadata(&self, key: &str, value: serde_json::Value) {
        self.task_metadata.lock().unwrap().insert(key.to_string(), value);
    }
    
    /// Get task-specific metadata
    pub fn get_task_metadata(&self, key: &str) -> Option<serde_json::Value> {
        self.task_metadata.lock().unwrap().get(key).cloned()
    }
    
    /// Get task metadata as string
    pub fn get_task_metadata_string(&self, key: &str) -> Option<String> {
        self.task_metadata
            .lock()
            .unwrap()
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

/// Namespaced metadata keys to avoid conflicts
pub mod task_metadata_keys {
    pub const AGENT_LOOP_COMPLETION_STATE: &str = "agent_loop.completion_state";
    pub const AGENT_LOOP_LAST_TOOL: &str = "agent_loop.last_tool";
    pub const COMPACT_LAST_RUN: &str = "compact.last_run_timestamp";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::model_providers::{ModelProvider, ModelRequest, ModelResponse};
    use async_trait::async_trait;
    use eyre::Result;
    use tokio_util::sync::CancellationToken;
    
    // Mock model provider for testing
    struct MockModelProvider;
    
    #[async_trait]
    impl ModelProvider for MockModelProvider {
        async fn request(
            &self,
            _request: ModelRequest,
            _when_receiving_begin: Box<dyn Fn() + Send>,
            _when_received: Box<dyn Fn(crate::agent_env::model_providers::ModelResponseChunk) + Send>,
            _cancellation_token: CancellationToken,
        ) -> Result<ModelResponse> {
            Ok(ModelResponse {
                content: "mock response".to_string(),
                tool_requests: vec![],
            })
        }
    }
    
    fn create_test_worker() -> Worker {
        let model_provider = Arc::new(MockModelProvider);
        Worker::new("test_worker".to_string(), model_provider)
    }
    
    #[test]
    fn test_worker_serialization() {
        // Create a worker
        let worker = create_test_worker();
        
        // Add some metadata
        worker.set_task_metadata("test_key", serde_json::json!("test_value"));
        
        // Serialize to JSON
        let json = serde_json::to_string(&worker).expect("Failed to serialize worker");
        
        // Verify serialization succeeded
        assert!(json.contains("test_worker"));
        
        // Note: task_metadata is skipped during serialization (Arc<Mutex<>> not serializable)
        // In real usage, you would serialize/deserialize the metadata separately if needed
        
        // Verify skipped fields are not in JSON
        assert!(!json.contains("model_provider"));
        assert!(!json.contains("lifecycle_state"));
        assert!(!json.contains("task_metadata"));
    }
    
    #[test]
    fn test_worker_deserialization() {
        // Create JSON with Worker data (minimal fields)
        let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "test_worker",
            "context_container": {}
        }"#;
        
        // Deserialize
        let worker: Worker = serde_json::from_str(json).expect("Failed to deserialize worker");
        
        // Verify fields are correctly populated
        assert_eq!(worker.name, "test_worker");
        
        // Verify skipped fields have default values
        assert_eq!(*worker.lifecycle_state.lock().unwrap(), WorkerLifecycleState::Idle);
        assert_eq!(*worker.state.lock().unwrap(), WorkerStates::Inactive);
        
        // task_metadata should be empty after deserialization
        assert!(worker.task_metadata.lock().unwrap().is_empty());
        
        // model_provider should be None after deserialization
        assert!(worker.model_provider.is_none());
        
        // In real usage, you would set runtime fields manually after deserialization:
        // worker.model_provider = Some(Arc::new(some_provider));
        // worker.set_task_metadata("key", value);
    }
    
    #[test]
    fn test_task_metadata_operations() {
        let worker = create_test_worker();
        
        // Set metadata
        worker.set_task_metadata("string_key", serde_json::json!("string_value"));
        worker.set_task_metadata("number_key", serde_json::json!(42));
        worker.set_task_metadata("bool_key", serde_json::json!(true));
        
        // Get metadata
        assert_eq!(
            worker.get_task_metadata("string_key"),
            Some(serde_json::json!("string_value"))
        );
        assert_eq!(
            worker.get_task_metadata("number_key"),
            Some(serde_json::json!(42))
        );
        
        // Get string metadata
        assert_eq!(
            worker.get_task_metadata_string("string_key"),
            Some("string_value".to_string())
        );
        
        // Non-string metadata returns None for get_task_metadata_string
        assert_eq!(worker.get_task_metadata_string("number_key"), None);
        
        // Non-existent key returns None
        assert_eq!(worker.get_task_metadata("nonexistent"), None);
        assert_eq!(worker.get_task_metadata_string("nonexistent"), None);
    }
    
    #[test]
    fn test_task_metadata_keys_constants() {
        // Verify constants are defined correctly
        assert_eq!(task_metadata_keys::AGENT_LOOP_COMPLETION_STATE, "agent_loop.completion_state");
        assert_eq!(task_metadata_keys::AGENT_LOOP_LAST_TOOL, "agent_loop.last_tool");
        assert_eq!(task_metadata_keys::COMPACT_LAST_RUN, "compact.last_run_timestamp");
    }
}
