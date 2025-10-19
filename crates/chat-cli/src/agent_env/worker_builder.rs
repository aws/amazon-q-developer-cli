use std::sync::Arc;
use std::borrow::Borrow;
use eyre::Result;

use super::{Session, Worker};
use crate::cli::Agent;
use crate::cli::chat::Platform;
use crate::os::Os;

pub struct WorkerBuilder {
    agent_name: Option<String>,
    platform: Platform,
    model: Option<String>,
    initial_input: Option<String>,
}

impl WorkerBuilder {
    pub fn new() -> Self {
        Self {
            agent_name: None,
            platform: Platform::default(),
            model: None,
            initial_input: None,
        }
    }
    
    pub fn agent(mut self, agent_name: Option<String>) -> Self {
        self.agent_name = agent_name;
        self
    }
    
    pub fn platform(mut self, platform: Platform) -> Self {
        self.platform = platform;
        self
    }
    
    pub fn model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }
    
    pub fn initial_input(mut self, input: Option<String>) -> Self {
        self.initial_input = input;
        self
    }
    
    pub async fn build(
        self,
        session: Arc<Session>,
        os: &Os,
    ) -> Result<Arc<Worker>> {
        // 1. Load agent config or use default
        let agent = if let Some(agent_name) = &self.agent_name {
            let (agent, _path) = Agent::get_agent_by_name(os, agent_name).await?;
            agent
        } else {
            Agent::default()
        };
        
        // 2. Extract resource references (file:// URLs)
        let resource_refs: Vec<String> = agent.resources
            .iter()
            .map(|r| {
                let s: &str = r.borrow();
                s.to_string()
            })
            .filter(|r| r.starts_with("file://"))
            .collect();
        
        // 3. Create worker through Session (maintains event publishing)
        let worker = session.build_worker("main".to_string());
        
        // 4. Set Os in worker for resource loading
        worker.set_os(Arc::new(os.clone()));
        
        // 5. Populate context container with references only
        if let Some(prompt) = &agent.prompt {
            worker.context_container.set_agent_prompt(prompt.clone());
        }
        
        if !resource_refs.is_empty() {
            worker.context_container.set_resource_references(resource_refs);
        }
        
        // 6. Add initial input if provided
        if let Some(input) = &self.initial_input {
            worker.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input.clone());
        }
        
        Ok(worker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::{EventBus, ModelProvider, ModelRequest, ModelResponse, ModelResponseChunk};
    use tokio_util::sync::CancellationToken;

    // Mock model provider for testing
    #[derive(Clone)]
    struct MockModelProvider;

    #[async_trait::async_trait]
    impl ModelProvider for MockModelProvider {
        async fn request(
            &self,
            _request: ModelRequest,
            _when_receiving_begin: Box<dyn Fn() + Send>,
            _when_received: Box<dyn Fn(ModelResponseChunk) + Send>,
            _cancellation_token: CancellationToken,
        ) -> Result<ModelResponse, eyre::Error> {
            Ok(ModelResponse {
                content: "test".to_string(),
                tool_requests: vec![],
            })
        }
    }

    #[tokio::test]
    async fn test_build_with_default_agent() {
        let event_bus = EventBus::default();
        let session = Arc::new(Session::new(event_bus, vec![Arc::new(MockModelProvider)]));
        let os = Os::new().await.unwrap();
        
        let worker = WorkerBuilder::new()
            .build(session, &os)
            .await
            .unwrap();
        
        assert!(worker.context_container.get_agent_prompt().is_none());
        // Default agent has resources, so don't check if empty
    }

    #[tokio::test]
    async fn test_build_with_initial_input() {
        let event_bus = EventBus::default();
        let session = Arc::new(Session::new(event_bus, vec![Arc::new(MockModelProvider)]));
        let os = Os::new().await.unwrap();
        
        let worker = WorkerBuilder::new()
            .initial_input(Some("Hello world".to_string()))
            .build(session, &os)
            .await
            .unwrap();
        
        let history = worker.context_container.conversation_history.lock().unwrap();
        assert_eq!(history.len(), 1);
    }
}
