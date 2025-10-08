# WorkerJob

## Overview

The **WorkerJob** combines a Worker, a WorkerTask, and execution infrastructure into an active running unit. It manages the async task lifecycle, cancellation, and completion callbacks.

## Implementation

**File**: `crates/chat-cli/src/agent_env/worker_job.rs`

## Structure

```rust
pub struct WorkerJob {
    pub worker: Arc<Worker>,
    pub worker_task: Arc<dyn WorkerTask>,
    pub cancellation_token: CancellationToken,
    pub task_handle: Option<tokio::task::JoinHandle<Result<(), eyre::Error>>>,
    pub worker_job_continuations: Arc<Continuations>,
}
```

### Fields

- **worker**: The Worker executing the task
- **worker_task**: The task being executed (trait object)
- **cancellation_token**: Token for graceful cancellation
- **task_handle**: Tokio task handle for the running async task
- **worker_job_continuations**: Callbacks to run on job completion

## Key Methods

### Constructor

**Location**: `worker_job.rs` lines 13-24

```rust
pub fn new(
    worker: Arc<Worker>,
    worker_task: Arc<dyn WorkerTask>,
    cancellation_token: CancellationToken,
) -> Self
```

Creates a new WorkerJob:
- Stores worker, task, and cancellation token
- Initializes empty task handle (not launched yet)
- Creates new Continuations for completion callbacks

### Launch

**Location**: `worker_job.rs` lines 26-39

```rust
pub fn launch(&mut self)
```

Spawns the async task:
1. Clones task, continuations, worker, and cancellation token
2. Spawns tokio task that:
   - Calls `worker_task.run().await`
   - Calls `continuations.complete()` with result
3. Stores task handle for later waiting/cancellation

### Cancellation

**Location**: `worker_job.rs` lines 41-43

```rust
pub fn cancel(&self)
```

Requests graceful cancellation:
- Cancels the cancellation token
- Task checks token periodically and exits cleanly
- Does not force-kill the task

### Wait for Completion

**Location**: `worker_job.rs` lines 45-53

```rust
pub async fn wait(self) -> Result<(), eyre::Error>
```

Waits for task to complete:
- Awaits the task handle
- Returns task result or join error
- Consumes self (can only wait once)

## Continuations System

**File**: `crates/chat-cli/src/agent_env/worker_job_continuations.rs`

The Continuations system enables callbacks to run when a job completes.

### Structure

```rust
pub struct Continuations {
    state: RwLock<JobState>,
    map: RwLock<HashMap<String, WorkerJobContinuationFn>>,
}

pub enum JobState {
    Running,
    Done(WorkerJobCompletionType, Option<String>),
}

pub enum WorkerJobCompletionType {
    Normal,
    Cancelled,
    Failed,
}
```

### Key Methods

**Location**: `worker_job_continuations.rs` lines 30-32

```rust
pub fn new() -> Self
```

Creates new Continuations with Running state and empty callback map.

**Location**: `worker_job_continuations.rs` lines 34-41

```rust
pub fn boxed<F, Fut>(f: F) -> WorkerJobContinuationFn
```

Helper to convert closure into continuation callback.

**Location**: `worker_job_continuations.rs` lines 43-54

```rust
pub async fn add_or_run_now(&self, key: impl Into<String>, callback: WorkerJobContinuationFn, worker: Arc<Worker>)
```

Adds callback or runs immediately if job already done:
- If Running: adds to map
- If Done: spawns callback immediately with completion info

**Location**: `worker_job_continuations.rs` lines 56-76

```rust
pub async fn complete(&self, result: Result<(), eyre::Error>, worker: Arc<Worker>, cancellation_token: &CancellationToken)
```

Marks job complete and runs all callbacks:
1. Determines completion type (Normal/Cancelled/Failed)
2. Extracts error message if failed
3. Updates state to Done
4. Takes all callbacks from map
5. Spawns each callback with completion info

## Usage Pattern

```rust
// Create job
let mut job = WorkerJob::new(
    worker,
    task,
    cancellation_token,
);

// Add completion callback
job.worker_job_continuations.add_or_run_now(
    "cleanup",
    Continuations::boxed(|worker, completion_type, error_msg| async move {
        println!("Job completed: {:?}", completion_type);
    }),
    worker.clone(),
).await;

// Launch
job.launch();

// Cancel if needed
job.cancel();

// Wait for completion
job.wait().await?;
```

## Lifecycle

```
new() → launch() → [running] → cancel() (optional) → wait() → completed
                       ↓
                  continuations.complete()
                       ↓
                  callbacks run
```

## Design Notes

- Job owns the task execution lifecycle
- Cancellation is cooperative (task must check token)
- Continuations enable cleanup, logging, chaining tasks
- Task handle allows waiting for completion
- Completion callbacks run asynchronously
- Job can only be waited once (consumes self)

## Thread Safety

- Continuations use RwLock for concurrent access
- Multiple threads can add callbacks safely
- Completion is atomic (state transition + callback execution)

## Future Enhancements

- Timeout support
- Progress reporting
- Retry logic
- Job priority
- Resource limits
- Metrics collection
