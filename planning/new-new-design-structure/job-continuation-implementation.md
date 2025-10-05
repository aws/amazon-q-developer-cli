# Job Continuation Implementation Plan

## Overview
Integrate the latched state + callback map pattern from `job-continuation.md` into the existing `chat-exp2/main.rs` to enable job completion callbacks with minimal code changes.

## Key Design Decisions

### 1. Ownership Strategy
- **Session.jobs**: Change to `Arc<Mutex<Vec<Arc<WorkerJob>>>>` to allow shared ownership
- **Session.run_demo_loop**: Return `Arc<WorkerJob>` for continuation registration
- **WorkerJob**: Add `continuations: Arc<Continuations>` field

### 2. Minimal Integration Approach
- Reuse existing `WorkerJob` structure, add continuation support
- Adapt the `Continuations` pattern to use `Arc<Worker>` instead of `LeTaskData`
- Keep existing cancellation and error handling intact

### 3. Continuation Callback Signature
```rust
#[derive(Debug, Clone, Copy)]
pub enum WorkerJobCompletionType {
    Normal,
    Cancelled,
    Failed,
}

type WorkerJobContinuationFn = Arc<
    dyn Fn(Arc<Worker>, WorkerJobCompletionType, Result<(), anyhow::Error>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send + Sync,
>;
```

## Implementation Steps

### Step 1: Add Continuation Types
Add to main.rs after the existing imports:
```rust
use std::collections::HashMap;
use std::pin::Pin;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy)]
pub enum WorkerJobCompletionType {
    Normal,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug)]
pub enum JobState {
    Running,
    Done(WorkerJobCompletionType, Result<(), anyhow::Error>),
}

pub type WorkerJobContinuationFn = Arc<
    dyn Fn(Arc<Worker>, WorkerJobCompletionType, Result<(), anyhow::Error>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send + Sync,
>;

pub struct Continuations {
    state: RwLock<JobState>,
    map: RwLock<HashMap<String, WorkerJobContinuationFn>>,
}
```

### Step 2: Implement Continuations
Add minimal implementation focusing on `add_or_run_now` and `complete`:
```rust
impl Continuations {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(JobState::Running),
            map: RwLock::new(HashMap::new()),
        }
    }

    pub fn boxed<F, Fut>(f: F) -> WorkerJobContinuationFn
    where
        F: Fn(Arc<Worker>, WorkerJobCompletionType, Result<(), anyhow::Error>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Arc::new(move |worker, completion_type, res| Box::pin(f(worker, completion_type, res)))
    }

    pub async fn add_or_run_now(&self, key: impl Into<String>, callback: WorkerJobContinuationFn, worker: Arc<Worker>) {
        match &*self.state.read().await {
            JobState::Running => {
                self.map.write().await.insert(key.into(), callback);
            }
            JobState::Done(completion_type, res) => {
                let completion_type = *completion_type;
                let res = res.clone();
                tokio::spawn(callback(worker, completion_type, res));
            }
        }
    }

    pub async fn complete(&self, result: Result<(), anyhow::Error>, worker: Arc<Worker>, cancellation_token: &CancellationToken) {
        let completion_type = if cancellation_token.is_cancelled() {
            WorkerJobCompletionType::Cancelled
        } else if result.is_err() {
            WorkerJobCompletionType::Failed
        } else {
            WorkerJobCompletionType::Normal
        };

        {
            let mut st = self.state.write().await;
            *st = JobState::Done(completion_type, result.clone());
        }
        let callbacks = {
            let mut map = self.map.write().await;
            std::mem::take(&mut *map)
        };
        for (_name, cb) in callbacks {
            let worker_clone = Arc::clone(&worker);
            let res = result.clone();
            tokio::spawn(cb(worker_clone, completion_type, res));
        }
    }
}
```

### Step 3: Modify WorkerJob
Add continuation support to `WorkerJob`:
```rust
pub struct WorkerJob {
    pub worker: Arc<Worker>,
    pub worker_task: Arc<dyn WorkerTask>,
    pub cancellation_token: CancellationToken,
    pub task_handle: Option<tokio::task::JoinHandle<Result<(), anyhow::Error>>>,
    pub continuations: Arc<Continuations>, // NEW
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
            continuations: Arc::new(Continuations::new()), // NEW
        }
    }

    pub fn launch(&mut self) {
        let worker_task_clone = self.worker_task.clone();
        let continuations = Arc::clone(&self.continuations);
        let worker = Arc::clone(&self.worker);
        let cancellation_token = self.cancellation_token.clone();
        
        let task_handle = tokio::spawn(async move {
            let result = worker_task_clone.run().await;
            continuations.complete(result.clone(), worker, &cancellation_token).await;
            result
        });
        self.task_handle = Some(task_handle);
    }
}
```

### Step 4: Update Session
Change Session to return job references:
```rust
pub struct Session {
    model_providers: Vec<BedrockConverseStreamModelProvider>,
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<Arc<WorkerJob>>>>, // Changed to Arc<WorkerJob>
}

impl Session {
    pub fn run_demo_loop(
        &self,
        worker: Arc<Worker>,
        input: WorkerInput,
        ui_interface: Arc<dyn WorkerToHostInterface>,
    ) -> Result<Arc<WorkerJob>, anyhow::Error> { // Return Arc<WorkerJob>
        let cancellation_token = CancellationToken::new();
        
        let worker_loop = Arc::new(WorkerProtoLoop::new(
            worker.clone(),
            input,
            ui_interface,
            cancellation_token.clone(),
        ));
        
        self.run(worker, worker_loop, cancellation_token)
    }

    fn run(
        &self,
        worker: Arc<Worker>,
        worker_task: Arc<dyn WorkerTask>,
        cancellation_token: CancellationToken,
    ) -> Result<Arc<WorkerJob>, anyhow::Error> { // Return Arc<WorkerJob>
        let mut job = WorkerJob::new(worker, worker_task, cancellation_token);
        job.launch();
        
        let job = Arc::new(job);
        self.jobs.lock().unwrap().push(job.clone());
        Ok(job)
    }
}
```

### Step 5: Add UI Method
Add completion reporting to `CliUi`:
```rust
impl CliUi {
    pub fn report_job_completion(&self, worker: Arc<Worker>, completion_type: WorkerJobCompletionType) -> impl Future<Output = ()> + Send {
        let worker_id = worker.id;
        async move {
            match completion_type {
                WorkerJobCompletionType::Normal => println!("Worker {} completed successfully", worker_id),
                WorkerJobCompletionType::Cancelled => println!("Worker {} was cancelled", worker_id),
                WorkerJobCompletionType::Failed => println!("Worker {} failed", worker_id),
            }
        }
    }
}
```

### Step 6: Update main() Demo
Modify main() to use continuations:
```rust
// Replace job creation with:
let job1 = session.run_demo_loop(
    worker.clone(),
    WorkerInput { prompt: "lorem ipsum please, twice".to_string() },
    Arc::new(ui.interface(AnsiColor::Cyan)),
)?;

let job2 = session.run_demo_loop(
    worker2.clone(),
    WorkerInput { prompt: "introduce yourself".to_string() },
    Arc::new(ui.interface(AnsiColor::Green)),
)?;

// Add continuations:
job1.continuations.add_or_run_now(
    "completion_report",
    Continuations::boxed(|worker, completion_type, _result| ui.report_job_completion(worker, completion_type)),
    job1.worker.clone(),
).await;

job2.continuations.add_or_run_now(
    "completion_report", 
    Continuations::boxed(|worker, completion_type, _result| ui.report_job_completion(worker, completion_type)),
    job2.worker.clone(),
).await;
```

## Benefits of This Approach

1. **Minimal Changes**: Reuses existing structures, adds continuation support incrementally
2. **Shared Ownership**: `Arc<WorkerJob>` allows multiple references without ownership issues
3. **Latched Behavior**: Late-registered continuations run immediately if job already completed
4. **Concurrent Execution**: Continuations run in parallel via `tokio::spawn`
5. **Clean API**: Simple `add_or_run_now` method for most use cases

## Testing Strategy

1. Test continuation registration before job completion
2. Test late registration (after completion) - should run immediately
3. Test multiple continuations on same job - should run in parallel
4. Test cancellation behavior - continuations should still run with cancellation error
