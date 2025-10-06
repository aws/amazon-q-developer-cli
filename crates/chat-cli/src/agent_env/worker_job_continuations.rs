use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::worker::Worker;

#[derive(Debug, Clone, Copy)]
pub enum WorkerJobCompletionType {
    Normal,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug)]
pub enum JobState {
    Running,
    Done(WorkerJobCompletionType, Option<String>),
}

pub type WorkerJobContinuationFn = Arc<
    dyn Fn(Arc<Worker>, WorkerJobCompletionType, Option<String>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send + Sync,
>;

pub struct Continuations {
    state: RwLock<JobState>,
    map: RwLock<HashMap<String, WorkerJobContinuationFn>>,
}

impl Continuations {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(JobState::Running),
            map: RwLock::new(HashMap::new()),
        }
    }

    pub fn boxed<F, Fut>(f: F) -> WorkerJobContinuationFn
    where
        F: Fn(Arc<Worker>, WorkerJobCompletionType, Option<String>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Arc::new(move |worker, completion_type, error_msg| Box::pin(f(worker, completion_type, error_msg)))
    }

    pub async fn add_or_run_now(&self, key: impl Into<String>, callback: WorkerJobContinuationFn, worker: Arc<Worker>) {
        match &*self.state.read().await {
            JobState::Running => {
                self.map.write().await.insert(key.into(), callback);
            }
            JobState::Done(completion_type, error_msg) => {
                let completion_type = *completion_type;
                let error_msg = error_msg.clone();
                tokio::spawn(callback(worker, completion_type, error_msg));
            }
        }
    }

    pub async fn complete(&self, result: Result<(), eyre::Error>, worker: Arc<Worker>, cancellation_token: &CancellationToken) {
        let completion_type = if cancellation_token.is_cancelled() {
            WorkerJobCompletionType::Cancelled
        } else if result.is_err() {
            WorkerJobCompletionType::Failed
        } else {
            WorkerJobCompletionType::Normal
        };

        let error_msg = result.err().map(|e| e.to_string());

        {
            let mut st = self.state.write().await;
            *st = JobState::Done(completion_type, error_msg.clone());
        }
        let callbacks = {
            let mut map = self.map.write().await;
            std::mem::take(&mut *map)
        };
        for (_name, cb) in callbacks {
            let worker_clone = Arc::clone(&worker);
            let error_msg_clone = error_msg.clone();
            tokio::spawn(cb(worker_clone, completion_type, error_msg_clone));
        }
    }
}
