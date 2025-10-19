use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::error;
use uuid::Uuid;

use super::worker::Worker;
use super::worker_job::WorkerJob;
use super::worker_task::WorkerTask;
use super::model_providers::ModelProvider;
use super::worker_tasks::{AgentLoop, AgentLoopInput};
use super::event_bus::EventBus;
use super::events::{AgentEnvironmentEvent, JobEvent, WorkerEvent, WorkerLifecycleState, UserInteractionRequired};

/// Maximum number of inactive jobs to keep in memory
pub const MAX_INACTIVE_JOBS: usize = 3;

#[derive(Clone)]
pub struct Session {
    event_bus: EventBus,
    model_providers: Vec<Arc<dyn ModelProvider>>,
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<Arc<WorkerJob>>>>,
    os: Option<Arc<crate::os::Os>>,
}

impl Session {
    pub fn new(event_bus: EventBus, model_providers: Vec<Arc<dyn ModelProvider>>) -> Self {
        Self {
            event_bus,
            model_providers,
            workers: Arc::new(Mutex::new(Vec::new())),
            jobs: Arc::new(Mutex::new(Vec::new())),
            os: None,
        }
    }
    
    pub fn with_os(mut self, os: Arc<crate::os::Os>) -> Self {
        self.os = Some(os);
        self
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub fn build_worker(&self, name: String) -> Arc<Worker> {
        let model_provider = self.model_providers.first()
            .expect("At least one model provider required")
            .clone();
        
        let worker = Arc::new(Worker::new(
            name.clone(),
            model_provider,
        ));
        
        // Set Os if available
        if let Some(os) = &self.os {
            worker.set_os(os.clone());
        }
        
        self.workers.lock().unwrap().push(worker.clone());
        
        // Publish WorkerEvent::Created
        self.event_bus.publish(AgentEnvironmentEvent::Worker(
            WorkerEvent::Created {
                worker_id: worker.id,
                name,
                timestamp: Instant::now(),
            }
        ));
        
        worker
    }

    pub fn run_task__agent_loop(
        &self,
        worker: Arc<Worker>,
        input: AgentLoopInput,
    ) -> Result<Arc<WorkerJob>, eyre::Error> {
        use super::events::{JobEvent, JobCompletionResult};
        
        // Task 3.1.1: Set worker state to Busy
        self.set_worker_lifecycle_state(worker.id, WorkerLifecycleState::Busy);
        
        // Create cancellation token
        let cancellation_token = CancellationToken::new();
        
        // Create task with EventBus
        let task = AgentLoop::new(
            worker.clone(),
            input,
            self.event_bus.clone(),
            cancellation_token.clone(),
        );
        
        // Create job
        let mut job = WorkerJob::new(
            worker.clone(),
            Arc::new(task),
            cancellation_token,
        );
        
        // Task 3.1.2: Publish JobStarted event
        self.event_bus.publish(AgentEnvironmentEvent::Job(
            JobEvent::Started {
                worker_id: worker.id,
                job_id: Uuid::new_v4(), // TODO: Add job_id to WorkerJob
                task_type: "AgentLoop".to_string(),
                timestamp: Instant::now(),
            }
        ));
        
        // Launch the job
        job.launch();
        
        let job = Arc::new(job);
        
        // Register job
        self.jobs.lock().unwrap().push(job.clone());
        
        // Task 3.1.4: Spawn completion handler
        let session = self.clone();
        let job_for_completion = job.clone();
        let worker_id = worker.id;
        tokio::spawn(async move {
            // Wait for job to complete by polling
            while job_for_completion.is_active() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            
            // Get completion state with error details
            let continuations_state = job_for_completion.worker_job_continuations.get_state().await;
            let result = match continuations_state {
                super::worker_job_continuations::JobState::Running => Ok(()),
                super::worker_job_continuations::JobState::Done(completion_type, error_msg) => {
                    match completion_type {
                        super::worker_job_continuations::WorkerJobCompletionType::Normal => Ok(()),
                        super::worker_job_continuations::WorkerJobCompletionType::Failed => {
                            Err(eyre::eyre!(error_msg.unwrap_or_else(|| "Job failed with unknown error".to_string())))
                        },
                        super::worker_job_continuations::WorkerJobCompletionType::Cancelled => {
                            Err(eyre::eyre!("Job cancelled"))
                        },
                    }
                }
            };
            
            // Task 3.1.3: Handle job completion
            session.handle_job_completion(worker_id, result).await;
        });
        
        Ok(job)
    }
    
    /// Task 3.1.3: Handle job completion
    async fn handle_job_completion(
        &self,
        worker_id: Uuid,
        result: Result<(), eyre::Error>,
    ) {
        use super::events::{JobEvent, JobCompletionResult};
        
        // Get worker to extract task metadata
        let task_metadata = if let Some(worker) = self.get_worker(worker_id) {
            worker.task_metadata.lock().unwrap().clone()
        } else {
            std::collections::HashMap::new()
        };
        
        // Determine completion result
        let completion_result = match result {
            Ok(_) => {
                // Check if task requires user interaction (e.g., tool approval)
                let user_interaction_required = if let Some(completion_state) = task_metadata.get("agent_loop_completion_state") {
                    if completion_state.as_str() == Some("completed_with_tool_request") {
                        UserInteractionRequired::ToolApproval
                    } else {
                        UserInteractionRequired::None
                    }
                } else {
                    UserInteractionRequired::None
                };
                
                JobCompletionResult::Success { 
                    task_metadata,
                    user_interaction_required,
                }
            }
            Err(e) => {
                error!(
                    worker_id = %worker_id,
                    error = %e,
                    "Job failed for worker"
                );
                JobCompletionResult::Failed {
                    error: e.to_string(),
                }
            }
        };
        
        // Update worker lifecycle state
        let new_state = match &completion_result {
            JobCompletionResult::Success { .. } => WorkerLifecycleState::Idle,
            JobCompletionResult::Failed { .. } => WorkerLifecycleState::IdleFailed,
            JobCompletionResult::Cancelled => WorkerLifecycleState::Idle,
        };
        
        self.set_worker_lifecycle_state(worker_id, new_state);
        
        // Publish completion event
        self.event_bus.publish(AgentEnvironmentEvent::Job(
            JobEvent::Completed {
                worker_id,
                job_id: Uuid::new_v4(), // TODO: Add job_id to WorkerJob
                result: completion_result,
                timestamp: Instant::now(),
            }
        ));
        
        // Note: Continuations are already run by WorkerJob
    }

    
    /// Task 3.1.5: Stub for compact conversation task
    pub fn run_task__compact_conversation(
        &self,
        _worker: Arc<Worker>,
        _input: super::worker_tasks::CompactInput,
    ) -> Result<Arc<WorkerJob>, eyre::Error> {
        // TODO: Implement in Phase 10
        unimplemented!("run_task__compact_conversation will be implemented in Phase 10")
    }
    
    pub fn cancel_all_jobs(&self) {
        let jobs = self.jobs.lock().unwrap();
        for job in jobs.iter() {
            job.cancel();
        }
    }

    /// Cleanup old inactive jobs, keeping only MAX_INACTIVE_JOBS most recent
    pub fn cleanup_inactive_jobs(&self) {
        let mut jobs = self.jobs.lock().unwrap();
        
        // Separate active and inactive jobs
        let (active, mut inactive): (Vec<_>, Vec<_>) = jobs
            .iter()
            .cloned()
            .partition(|job| job.is_active());
        
        // Keep only last MAX_INACTIVE_JOBS inactive jobs
        if inactive.len() > MAX_INACTIVE_JOBS {
            let keep_from = inactive.len() - MAX_INACTIVE_JOBS;
            inactive.drain(0..keep_from);
        }
        
        // Rebuild jobs list: active + recent inactive
        *jobs = active;
        jobs.extend(inactive);
    }

    /// Get count of active and inactive jobs
    pub fn get_job_counts(&self) -> (usize, usize) {
        let jobs = self.jobs.lock().unwrap();
        let active = jobs.iter().filter(|j| j.is_active()).count();
        let inactive = jobs.len() - active;
        (active, inactive)
    }

    /// Check if there are any active jobs
    /// 
    /// Returns true if at least one job is active, false otherwise.
    /// Note: This checks the active state of jobs, not just if the jobs Vec is non-empty.
    pub fn has_active_jobs(&self) -> bool {
        let jobs = self.jobs.lock().unwrap();
        jobs.iter().any(|job| job.is_active())
    }

    /// Wait for all active jobs to complete
    pub async fn wait_for_all_jobs(&self) {
        loop {
            let has_active = {
                let jobs = self.jobs.lock().unwrap();
                jobs.iter().any(|job| job.is_active())
            };
            
            if !has_active {
                break;
            }
            
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    
    /// Set worker lifecycle state and publish event
    pub fn set_worker_lifecycle_state(
        &self,
        worker_id: Uuid,
        new_state: WorkerLifecycleState,
    ) {
        // Find worker
        let workers = self.workers.lock().unwrap();
        if let Some(worker) = workers.iter().find(|w| w.id == worker_id) {
            // Get old state
            let old_state = {
                let state = worker.lifecycle_state.lock().unwrap();
                *state
            };
            
            // Update state
            {
                let mut state = worker.lifecycle_state.lock().unwrap();
                *state = new_state;
            }
            
            // Publish event
            self.event_bus.publish(AgentEnvironmentEvent::Worker(
                WorkerEvent::LifecycleStateChanged {
                    worker_id,
                    old_state,
                    new_state,
                    timestamp: Instant::now(),
                }
            ));
        }
    }
    
    /// Delete worker and publish event
    pub fn delete_worker(&self, worker_id: Uuid) -> Result<(), eyre::Error> {
        // Cancel any active jobs for this worker
        self.cancel_worker_jobs(worker_id)?;
        
        // Remove worker
        let mut workers = self.workers.lock().unwrap();
        workers.retain(|w| w.id != worker_id);
        
        // Publish event
        self.event_bus.publish(AgentEnvironmentEvent::Worker(
            WorkerEvent::Deleted {
                worker_id,
                timestamp: Instant::now(),
            }
        ));
        
        Ok(())
    }
    
    /// Get worker by ID
    pub fn get_worker(&self, worker_id: Uuid) -> Option<Arc<Worker>> {
        let workers = self.workers.lock().unwrap();
        workers.iter().find(|w| w.id == worker_id).cloned()
    }
    
    /// Get all workers
    pub fn get_workers(&self) -> Vec<Arc<Worker>> {
        let workers = self.workers.lock().unwrap();
        workers.clone()
    }
    
    /// Cancel all jobs for a specific worker
    pub fn cancel_worker_jobs(&self, worker_id: Uuid) -> Result<(), eyre::Error> {
        let jobs = self.jobs.lock().unwrap();
        for job in jobs.iter() {
            if job.worker.id == worker_id {
                job.cancel();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::model_providers::{ModelProvider, ModelRequest, ModelResponse, ModelResponseChunk};
    use async_trait::async_trait;
    use eyre::Result;
    
    // Mock model provider for testing
    struct MockModelProvider;
    
    #[async_trait]
    impl ModelProvider for MockModelProvider {
        async fn request(
            &self,
            _request: ModelRequest,
            _when_receiving_begin: Box<dyn Fn() + Send>,
            _when_received: Box<dyn Fn(ModelResponseChunk) + Send>,
            _cancellation_token: CancellationToken,
        ) -> Result<ModelResponse> {
            Ok(ModelResponse {
                content: "mock response".to_string(),
                tool_requests: vec![],
            })
        }
    }
    
    fn create_test_session() -> Session {
        let event_bus = EventBus::default();
        let model_providers: Vec<Arc<dyn ModelProvider>> = vec![Arc::new(MockModelProvider)];
        Session::new(event_bus, model_providers)
    }
    
    #[test]
    fn test_worker_creation_publishes_event() {
        let session = create_test_session();
        let mut receiver = session.event_bus().subscribe();
        
        // Create worker
        let worker = session.build_worker("test_worker".to_string());
        
        // Verify event was published
        let event = receiver.try_recv().expect("Expected WorkerEvent::Created");
        match event {
            AgentEnvironmentEvent::Worker(WorkerEvent::Created { worker_id, name, .. }) => {
                assert_eq!(worker_id, worker.id);
                assert_eq!(name, "test_worker");
            }
            _ => panic!("Expected WorkerEvent::Created"),
        }
    }
    
    #[test]
    fn test_worker_deletion_publishes_event() {
        let session = create_test_session();
        let worker = session.build_worker("test_worker".to_string());
        let worker_id = worker.id;
        
        // Subscribe after creation to avoid getting Created event
        let mut receiver = session.event_bus().subscribe();
        
        // Delete worker
        session.delete_worker(worker_id).expect("Failed to delete worker");
        
        // Verify event was published
        let event = receiver.try_recv().expect("Expected WorkerEvent::Deleted");
        match event {
            AgentEnvironmentEvent::Worker(WorkerEvent::Deleted { worker_id: deleted_id, .. }) => {
                assert_eq!(deleted_id, worker_id);
            }
            _ => panic!("Expected WorkerEvent::Deleted"),
        }
        
        // Verify worker was removed
        assert!(session.get_worker(worker_id).is_none());
    }
    
    #[test]
    fn test_lifecycle_state_transitions() {
        let session = create_test_session();
        let worker = session.build_worker("test_worker".to_string());
        let worker_id = worker.id;
        
        // Subscribe after creation
        let mut receiver = session.event_bus().subscribe();
        
        // Transition to Busy
        session.set_worker_lifecycle_state(worker_id, WorkerLifecycleState::Busy);
        
        // Verify event was published
        let event = receiver.try_recv().expect("Expected LifecycleStateChanged");
        match event {
            AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged { 
                worker_id: wid, 
                old_state, 
                new_state, 
                .. 
            }) => {
                assert_eq!(wid, worker_id);
                assert_eq!(old_state, WorkerLifecycleState::Idle);
                assert_eq!(new_state, WorkerLifecycleState::Busy);
            }
            _ => panic!("Expected WorkerEvent::LifecycleStateChanged"),
        }
        
        // Verify state was updated in worker
        assert_eq!(*worker.lifecycle_state.lock().unwrap(), WorkerLifecycleState::Busy);
        
        // Transition to IdleFailed
        session.set_worker_lifecycle_state(worker_id, WorkerLifecycleState::IdleFailed);
        
        let event = receiver.try_recv().expect("Expected LifecycleStateChanged");
        match event {
            AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged { 
                old_state, 
                new_state, 
                .. 
            }) => {
                assert_eq!(old_state, WorkerLifecycleState::Busy);
                assert_eq!(new_state, WorkerLifecycleState::IdleFailed);
            }
            _ => panic!("Expected WorkerEvent::LifecycleStateChanged"),
        }
    }
    
    #[test]
    fn test_multiple_workers_dont_interfere() {
        let session = create_test_session();
        let mut receiver = session.event_bus().subscribe();
        
        // Create two workers
        let worker1 = session.build_worker("worker1".to_string());
        let worker2 = session.build_worker("worker2".to_string());
        
        // Verify both Created events
        let event1 = receiver.try_recv().expect("Expected first Created event");
        let event2 = receiver.try_recv().expect("Expected second Created event");
        
        // Verify worker_ids are different
        let id1 = event1.worker_id().expect("Expected worker_id");
        let id2 = event2.worker_id().expect("Expected worker_id");
        assert_ne!(id1, id2);
        
        // Change state of worker1
        session.set_worker_lifecycle_state(worker1.id, WorkerLifecycleState::Busy);
        
        // Verify only worker1's state changed
        assert_eq!(*worker1.lifecycle_state.lock().unwrap(), WorkerLifecycleState::Busy);
        assert_eq!(*worker2.lifecycle_state.lock().unwrap(), WorkerLifecycleState::Idle);
    }
    
    #[tokio::test]
    async fn test_job_lifecycle_events() {
        use crate::agent_env::events::{JobEvent, JobCompletionResult};
        
        let session = create_test_session();
        let worker = session.build_worker("test_worker".to_string());
        let worker_id = worker.id;
        
        // Subscribe after worker creation to avoid Created event
        let mut receiver = session.event_bus().subscribe();
        
        // Launch agent loop task
        let input = AgentLoopInput {};
        let _job = session.run_task__agent_loop(worker.clone(), input)
            .expect("Failed to launch agent loop");
        
        // Verify LifecycleStateChanged to Busy
        let event = receiver.recv().await.expect("Expected LifecycleStateChanged to Busy");
        match event {
            AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged { 
                worker_id: wid, 
                new_state, 
                .. 
            }) => {
                assert_eq!(wid, worker_id);
                assert_eq!(new_state, WorkerLifecycleState::Busy);
            }
            _ => panic!("Expected WorkerEvent::LifecycleStateChanged to Busy, got {:?}", event),
        }
        
        // Verify JobStarted event
        let event = receiver.recv().await.expect("Expected JobStarted");
        match event {
            AgentEnvironmentEvent::Job(JobEvent::Started { 
                worker_id: wid, 
                task_type, 
                .. 
            }) => {
                assert_eq!(wid, worker_id);
                assert_eq!(task_type, "AgentLoop");
            }
            _ => panic!("Expected JobEvent::Started, got {:?}", event),
        }
        
        // Wait for job to complete (with timeout)
        let timeout = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout);
        
        let mut got_lifecycle_change = false;
        let mut got_job_completed = false;
        
        loop {
            tokio::select! {
                result = receiver.recv() => {
                    match result {
                        Ok(event) => {
                            match event {
                                AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged { 
                                    worker_id: wid, 
                                    new_state, 
                                    .. 
                                }) if wid == worker_id => {
                                    // Should transition back to Idle or IdleFailed
                                    assert!(
                                        new_state == WorkerLifecycleState::Idle || 
                                        new_state == WorkerLifecycleState::IdleFailed
                                    );
                                    got_lifecycle_change = true;
                                }
                                AgentEnvironmentEvent::Job(JobEvent::Completed { 
                                    worker_id: wid, 
                                    result, 
                                    .. 
                                }) if wid == worker_id => {
                                    // Verify completion result structure
                                    match result {
                                        JobCompletionResult::Success { .. } | 
                                        JobCompletionResult::Failed { .. } | 
                                        JobCompletionResult::Cancelled => {
                                            got_job_completed = true;
                                        }
                                    }
                                }
                                _ => {} // Ignore other events
                            }
                            
                            if got_lifecycle_change && got_job_completed {
                                break;
                            }
                        }
                        Err(e) => panic!("Error receiving event: {}", e),
                    }
                }
                _ = &mut timeout => {
                    panic!("Timeout waiting for job completion events");
                }
            }
        }
        
        assert!(got_lifecycle_change, "Did not receive LifecycleStateChanged event");
        assert!(got_job_completed, "Did not receive JobCompleted event");
    }

    #[test]
    fn test_has_active_jobs_no_jobs() {
        let session = create_test_session();
        
        // No jobs exist
        assert!(!session.has_active_jobs());
    }

    #[tokio::test]
    async fn test_has_active_jobs_with_active_job() {
        let session = create_test_session();
        let worker = session.build_worker("test_worker".to_string());
        
        // Launch a job
        let input = AgentLoopInput {};
        let _job = session.run_task__agent_loop(worker.clone(), input)
            .expect("Failed to launch agent loop");
        
        // Should have active jobs
        assert!(session.has_active_jobs());
    }

    #[tokio::test]
    async fn test_has_active_jobs_after_completion() {
        let session = create_test_session();
        let worker = session.build_worker("test_worker".to_string());
        let mut receiver = session.event_bus().subscribe();
        
        // Launch a job
        let input = AgentLoopInput {};
        let _job = session.run_task__agent_loop(worker.clone(), input)
            .expect("Failed to launch agent loop");
        
        // Wait for job to complete
        let timeout = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout);
        
        loop {
            tokio::select! {
                result = receiver.recv() => {
                    match result {
                        Ok(AgentEnvironmentEvent::Job(JobEvent::Completed { .. })) => {
                            break;
                        }
                        Ok(_) => {} // Ignore other events
                        Err(e) => panic!("Error receiving event: {}", e),
                    }
                }
                _ = &mut timeout => {
                    panic!("Timeout waiting for job completion");
                }
            }
        }
        
        // After completion, should have no active jobs
        assert!(!session.has_active_jobs());
    }
}
