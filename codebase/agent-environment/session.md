# Session

## Overview

The **Session** serves as the central orchestrator managing all Workers and Jobs. It provides worker factory methods, job launching, lifecycle management, and resource sharing across multiple concurrent agents.

## Implementation

**File**: `crates/chat-cli/src/agent_env/session.rs`

## Structure

```rust
pub struct Session {
    model_providers: Vec<BedrockConverseStreamModelProvider>,
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<Arc<WorkerJob>>>>,
}
```

### Fields

- **model_providers**: Shared LLM providers for all workers
- **workers**: Thread-safe collection of all created workers
- **jobs**: Thread-safe collection of all running/completed jobs

## Key Methods

### Constructor

**Location**: `session.rs` lines 16-22

```rust
pub fn new(model_providers: Vec<BedrockConverseStreamModelProvider>) -> Self
```

Creates a new Session with provided model providers. Initializes empty worker and job collections.

### Worker Factory

**Location**: `session.rs` lines 24-36

```rust
pub fn build_worker(&self, name: String) -> Arc<Worker>
```

Creates a new Worker with:
- Specified name for identification
- First available model provider
- Registered in session's worker collection
- Returns Arc-wrapped worker for shared ownership

**Parameters**:
- `name`: Human-readable name for the worker (e.g., "Worker#1")

**Example**:
```rust
let worker1 = session.build_worker("Worker#1".to_string());
let worker2 = session.build_worker("Worker#2".to_string());
```

### Task Launchers

#### Demo Loop

**Location**: `session.rs` lines 38-51

```rust
pub fn run_demo_loop(
    &self,
    worker: Arc<Worker>,
    input: WorkerInput,
    ui_interface: Arc<dyn WorkerToHostInterface>,
) -> Result<Arc<WorkerJob>, eyre::Error>
```

Launches a demo prototype loop task:
1. Creates cancellation token
2. Instantiates WorkerProtoLoop task
3. Calls internal `run()` method
4. Returns job handle

#### Agent Loop

**Location**: `session.rs` lines 53-66

```rust
pub fn run_agent_loop(
    &self,
    worker: Arc<Worker>,
    input: AgentLoopInput,
    ui_interface: Arc<dyn WorkerToHostInterface>,
) -> Result<Arc<WorkerJob>, eyre::Error>
```

Launches a real agent loop task:
1. Creates cancellation token
2. Instantiates AgentLoop task
3. Calls internal `run()` method
4. Returns job handle

### Internal Job Runner

**Location**: `session.rs` lines 68-83

```rust
fn run(
    &self,
    worker: Arc<Worker>,
    worker_task: Arc<dyn WorkerTask>,
    cancellation_token: CancellationToken,
) -> Result<Arc<WorkerJob>, eyre::Error>
```

Core job execution logic:
1. Creates WorkerJob with worker, task, and cancellation token
2. Launches the job (spawns async task)
3. Wraps job in Arc for shared ownership
4. Registers job in session's job collection
5. Returns job handle

### Cancellation

**Location**: `session.rs` lines 85-90

```rust
pub fn cancel_all_jobs(&self)
```

Cancels all running jobs:
- Iterates through job collection
- Calls `cancel()` on each job
- Jobs will complete gracefully via cancellation tokens

## Usage Pattern

```rust
// Create session with model providers
let session = Session::new(vec![bedrock_provider]);

// Create workers
let worker1 = session.build_worker("Worker#1".to_string());
let worker2 = session.build_worker("Worker#2".to_string());

// Launch agent loops
let job1 = session.run_agent_loop(
    worker1,
    AgentLoopInput { prompt: "Task 1".to_string() },
    ui_interface.clone(),
)?;

let job2 = session.run_agent_loop(
    worker2,
    AgentLoopInput { prompt: "Task 2".to_string() },
    ui_interface.clone(),
)?;

// Both jobs run concurrently

// Cancel all if needed
session.cancel_all_jobs();

// Wait for completion
job1.wait().await?;
job2.wait().await?;
```

## Design Notes

- Session owns shared resources (model providers)
- Workers and jobs are Arc-wrapped for shared ownership
- Thread-safe collections enable concurrent access
- Cancellation tokens enable graceful shutdown
- Each job gets independent cancellation token
- Session tracks all workers and jobs for lifecycle management

## Resource Sharing

The Session enables efficient resource sharing:
- **Model Providers**: Shared across all workers (connection pooling, rate limiting)
- **Thread Pool**: Tokio runtime shared for all async tasks
- **Future**: Tools, configuration, conversation storage

## Future Enhancements

- Job cleanup policy (max inactive jobs)
- Worker reuse across multiple tasks
- Resource limits (max concurrent jobs)
- Job queuing and scheduling
- Metrics and monitoring
- Graceful shutdown with timeout
- Tools hosting
- Configuration management
- Conversation persistence
