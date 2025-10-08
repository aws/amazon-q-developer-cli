use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use super::worker::Worker;
use super::worker_task::WorkerTask;
use super::worker_job_continuations::{Continuations, WorkerJobCompletionType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Active,
    Completed,
    Cancelled,
    Failed,
}

pub struct WorkerJob {
    pub worker: Arc<Worker>,
    pub worker_task: Arc<dyn WorkerTask>,
    pub cancellation_token: CancellationToken,
    pub task_handle: Option<tokio::task::JoinHandle<Result<(), eyre::Error>>>,
    pub worker_job_continuations: Arc<Continuations>,
}

impl WorkerJob {
    pub fn new(
        worker: Arc<Worker>,
        worker_task: Arc<dyn WorkerTask>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            worker,
            worker_task,
            cancellation_token,
            task_handle: None,
            worker_job_continuations: Arc::new(Continuations::new()),
        }
    }

    pub fn launch(&mut self) {
        let worker_task_clone = self.worker_task.clone();
        let continuations = Arc::clone(&self.worker_job_continuations);
        let worker = Arc::clone(&self.worker);
        let cancellation_token = self.cancellation_token.clone();
        
        let task_handle = tokio::spawn(async move {
            let result = worker_task_clone.run().await;
            continuations.complete(result, worker, &cancellation_token).await;
            Ok(())
        });
        self.task_handle = Some(task_handle);
    }

    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    pub async fn wait(self) -> Result<(), eyre::Error> {
        match self.task_handle {
            Some(handle) => match handle.await {
                Ok(result) => result,
                Err(join_error) => Err(eyre::eyre!("Task panicked: {}", join_error)),
            },
            None => Err(eyre::eyre!("Task not launched")),
        }
    }

    /// Check if job is still active (running)
    pub fn is_active(&self) -> bool {
        match &self.task_handle {
            Some(handle) => !handle.is_finished(),
            None => false,
        }
    }

    /// Check if job is complete (finished, cancelled, or failed)
    pub fn is_complete(&self) -> bool {
        !self.is_active()
    }

    /// Get current job state
    pub async fn get_state(&self) -> JobState {
        if self.is_active() {
            return JobState::Active;
        }

        // Job is complete, check completion type from continuations
        let state = self.worker_job_continuations.get_state().await;
        match state {
            super::worker_job_continuations::JobState::Running => JobState::Active,
            super::worker_job_continuations::JobState::Done(completion_type, _) => {
                match completion_type {
                    WorkerJobCompletionType::Normal => JobState::Completed,
                    WorkerJobCompletionType::Cancelled => JobState::Cancelled,
                    WorkerJobCompletionType::Failed => JobState::Failed,
                }
            }
        }
    }
}
