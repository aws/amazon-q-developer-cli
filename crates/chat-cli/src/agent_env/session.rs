use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

use super::worker::Worker;
use super::worker_job::WorkerJob;
use super::worker_task::WorkerTask;
use super::model_providers::ModelProvider;
use super::worker_interface::WorkerToHostInterface;
use super::demo::{WorkerProtoLoop, WorkerInput};
use super::worker_tasks::{AgentLoop, AgentLoopInput};

pub struct Session {
    model_providers: Vec<Arc<dyn ModelProvider>>,
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<Arc<WorkerJob>>>>,
}

impl Session {
    pub fn new(model_providers: Vec<Arc<dyn ModelProvider>>) -> Self {
        Self {
            model_providers,
            workers: Arc::new(Mutex::new(Vec::new())),
            jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn build_worker(&self) -> Arc<Worker> {
        let model_provider = self.model_providers.first()
            .expect("At least one model provider required")
            .clone();
        
        let worker = Arc::new(Worker::new(
            "Test worker".to_string(),
            model_provider,
        ));
        
        self.workers.lock().unwrap().push(worker.clone());
        worker
    }

    pub fn run_demo_loop(
        &self,
        worker: Arc<Worker>,
        input: WorkerInput,
        ui_interface: Arc<dyn WorkerToHostInterface>,
    ) -> Result<Arc<WorkerJob>, eyre::Error> {
        let cancellation_token = CancellationToken::new();
        
        let worker_loop = Arc::new(WorkerProtoLoop::new(
            worker.clone(),
            input,
            ui_interface,
            cancellation_token.clone(),
        ));
        
        self.run(worker, worker_loop, cancellation_token)
    }

    pub fn run_agent_loop(
        &self,
        worker: Arc<Worker>,
        input: AgentLoopInput,
        ui_interface: Arc<dyn WorkerToHostInterface>,
    ) -> Result<Arc<WorkerJob>, eyre::Error> {
        let cancellation_token = CancellationToken::new();
        
        let agent_loop = Arc::new(AgentLoop::new(
            worker.clone(),
            input,
            ui_interface,
            cancellation_token.clone(),
        ));
        
        self.run(worker, agent_loop, cancellation_token)
    }

    fn run(
        &self,
        worker: Arc<Worker>,
        worker_task: Arc<dyn WorkerTask>,
        cancellation_token: CancellationToken,
    ) -> Result<Arc<WorkerJob>, eyre::Error> {
        let mut job = WorkerJob::new(
            worker,
            worker_task,
            cancellation_token,
        );
        
        job.launch();

        let job = Arc::new(job);
        self.jobs.lock().unwrap().push(job.clone());
        Ok(job)
    }

    pub fn cancel_all_jobs(&self) {
        let jobs = self.jobs.lock().unwrap();
        for job in jobs.iter() {
            job.cancel();
        }
    }
}
