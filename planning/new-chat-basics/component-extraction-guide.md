# Component Extraction Guide

This document provides detailed instructions for extracting each component from the prototype into the new structure.

## 1. Worker Job Continuations System (`worker_job_continuations.rs`)

### Extract from prototype (lines ~13-95):
- `WorkerJobCompletionType` enum
- `JobState` enum
- `WorkerJobContinuationFn` type alias
- `Continuations` struct and implementation

### Key points:
- No external dependencies (self-contained)
- Uses `std::collections::HashMap`, `std::pin::Pin`, `std::future::Future`
- Uses `tokio::sync::RwLock` for async locking
- Generic over worker type (uses `Arc<Worker>`)

### Module structure:
```rust
use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy)]
pub enum WorkerJobCompletionType { ... }

#[derive(Clone, Debug)]
pub enum JobState { ... }

pub type WorkerJobContinuationFn = Arc<...>;

pub struct Continuations { ... }

impl Continuations { ... }
```

## 2. Model Provider (`model_provider.rs` + `model_provider_impls/`)

### Extract from prototype (lines ~97-265):
- `ModelRequest` struct
- `ModelResponseChunk` enum
- `ModelResponse` struct
- `ToolRequest` struct
- `ModelProvider` trait
- `BedrockConverseStreamModelProvider` struct and implementation (goes in separate file)

### Key points:
- Trait and data structures in `model_provider.rs`
- Implementations in `model_provider_impls/` subdirectory
- Depends on `aws_sdk_bedrockruntime` for Bedrock client
- Uses `tokio_util::sync::CancellationToken`
- Async trait requires `#[async_trait::async_trait]`
- Handles streaming with cancellation support

### Module structure (`model_provider.rs`):
```rust
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ModelRequest { ... }

#[derive(Debug, Clone)]
pub enum ModelResponseChunk { ... }

#[derive(Debug, Clone)]
pub struct ModelResponse { ... }

#[derive(Debug, Clone)]
pub struct ToolRequest { ... }

#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync { ... }
```

### Module structure (`model_provider_impls/bedrock_converse_stream.rs`):
```rust
use aws_sdk_bedrockruntime::{Client as BedrockClient, types::*};
use tokio_util::sync::CancellationToken;
use crate::agent_env::model_provider::*;

#[derive(Clone)]
pub struct BedrockConverseStreamModelProvider { ... }

#[async_trait::async_trait]
impl ModelProvider for BedrockConverseStreamModelProvider { ... }
```

### Module structure (`model_provider_impls/mod.rs`):
```rust
pub mod bedrock_converse_stream;

pub use bedrock_converse_stream::BedrockConverseStreamModelProvider;
```

## 3. Worker (`worker.rs`)

### Extract from prototype (lines ~267-330):
- `WorkerStates` enum
- `Worker` struct
- Worker state management methods

### Key points:
- Depends on `model_provider_impls::BedrockConverseStreamModelProvider`
- Depends on `worker_interface.rs` for `WorkerToHostInterface`
- Uses `Arc<Mutex<T>>` for thread-safe state
- Uses `uuid::Uuid` for worker IDs

### Module structure:
```rust
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use super::model_provider_impls::BedrockConverseStreamModelProvider;
use super::worker_interface::WorkerToHostInterface;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerStates { ... }

pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub model_provider: BedrockConverseStreamModelProvider,
    pub state: Arc<Mutex<WorkerStates>>,
    pub last_failure: Arc<Mutex<Option<String>>>,
}

impl Worker {
    pub fn new(...) -> Self { ... }
    pub fn set_state(...) { ... }
    pub fn get_state(...) -> WorkerStates { ... }
    pub fn set_failure(...) { ... }
    pub fn get_failure(...) -> Option<String> { ... }
}
```

## 4. Worker Task (`worker_task.rs`)

### Extract from prototype (lines ~332-350):
- `WorkerTask` trait

### Key points:
- Depends on `worker.rs` for `Worker`
- Simple trait definition
- Async trait requires `#[async_trait::async_trait]`

### Module structure:
```rust
use super::worker::Worker;

#[async_trait::async_trait]
pub trait WorkerTask: Send + Sync {
    fn get_worker(&self) -> &Worker;
    async fn run(&self) -> Result<(), anyhow::Error>;
}
```

## 5. Worker Job (`worker_job.rs`)

### Extract from prototype (lines ~352-410):
- `WorkerJob` struct
- Job lifecycle methods

### Key points:
- Depends on `worker.rs`, `worker_task.rs`, `worker_job_continuations.rs`
- Uses `tokio_util::sync::CancellationToken`
- Uses `tokio::task::JoinHandle` for async task management
- Integrates continuation system

### Module structure:
```rust
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use super::worker::Worker;
use super::worker_task::WorkerTask;
use super::worker_job_continuations::Continuations;

pub struct WorkerJob {
    pub worker: Arc<Worker>,
    pub worker_task: Arc<dyn WorkerTask>,
    pub cancellation_token: CancellationToken,
    pub task_handle: Option<tokio::task::JoinHandle<Result<(), anyhow::Error>>>,
    pub worker_job_continuations: Arc<Continuations>,
}

impl WorkerJob {
    pub fn new(...) -> Self { ... }
    pub fn launch(&mut self) { ... }
    pub fn cancel(&self) { ... }
    pub async fn wait(self) -> Result<(), anyhow::Error> { ... }
}
```

## 6. Worker Interface (`worker_interface.rs`)

### Extract from prototype (lines ~545-560):
- `WorkerToHostInterface` trait

### Key points:
- Depends on `worker.rs` for `WorkerStates`
- Depends on `model_provider.rs` for `ModelResponseChunk`
- Uses `tokio_util::sync::CancellationToken`
- Async trait requires `#[async_trait::async_trait]`

### Module structure:
```rust
use uuid::Uuid;
use tokio_util::sync::CancellationToken;
use super::worker::WorkerStates;
use super::model_provider::ModelResponseChunk;

#[async_trait::async_trait]
pub trait WorkerToHostInterface: Send + Sync {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates);
    fn response_chunk_received(&self, worker_id: Uuid, chunk: ModelResponseChunk);
    async fn get_tool_confirmation(
        &self,
        worker_id: Uuid,
        request: String,
        cancellation_token: CancellationToken,
    ) -> Result<String, anyhow::Error>;
}
```

## 7. Session (`session.rs`)

### Extract from prototype (lines ~562-620):
- `Session` struct
- Worker factory methods
- Job launching methods

### Key points:
- Depends on `worker.rs`, `worker_job.rs`, `worker_task.rs`, `model_provider_impls`
- Uses `Arc<Mutex<Vec<T>>>` for concurrent collections
- Provides factory methods for workers and jobs

### Module structure:
```rust
use std::sync::{Arc, Mutex};
use super::worker::Worker;
use super::worker_job::WorkerJob;
use super::worker_task::WorkerTask;
use super::model_provider_impls::BedrockConverseStreamModelProvider;
use tokio_util::sync::CancellationToken;

pub struct Session {
    model_providers: Vec<BedrockConverseStreamModelProvider>,
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<Arc<WorkerJob>>>>,
}

impl Session {
    pub fn new(...) -> Self { ... }
    pub fn build_worker(&self) -> Arc<Worker> { ... }
    pub fn run_demo_loop(...) -> Result<Arc<WorkerJob>, anyhow::Error> { ... }
    fn run(...) -> Result<Arc<WorkerJob>, anyhow::Error> { ... }
    pub fn cancel_all_jobs(&self) { ... }
}
```

## 8. Demo: Proto Loop (`demo/proto_loop.rs`)

### Extract from prototype (lines ~412-543):
- `WorkerInput` struct
- `WorkerProtoLoop` struct
- WorkerTask implementation

### Key points:
- Depends on all core modules
- Demonstrates complete agent execution flow
- Temporary demo code

### Module structure:
```rust
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use crate::agent_env::{
    Worker, WorkerTask, WorkerStates, WorkerToHostInterface,
    ModelRequest, ModelResponse, ModelResponseChunk,
};

#[derive(Debug, Clone)]
pub struct WorkerInput { ... }

pub struct WorkerProtoLoop { ... }

impl WorkerProtoLoop {
    pub fn new(...) -> Self { ... }
    // Helper methods
}

#[async_trait::async_trait]
impl WorkerTask for WorkerProtoLoop { ... }
```

## 9. Demo: CLI Interface (`demo/cli_interface.rs`)

### Extract from prototype (lines ~622-700):
- `CliInterface` struct
- `AnsiColor` enum
- `CliUi` struct
- WorkerToHostInterface implementation

### Key points:
- Depends on core worker and model_provider modules
- Demonstrates console-based UI
- Temporary demo code

### Module structure:
```rust
use std::sync::Arc;
use std::future::Future;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use crate::agent_env::{
    Worker, WorkerStates, WorkerToHostInterface,
    ModelResponseChunk, WorkerJobCompletionType,
};

pub struct CliInterface { ... }

impl CliInterface {
    pub fn new(...) -> Self { ... }
}

#[async_trait::async_trait]
impl WorkerToHostInterface for CliInterface { ... }

#[derive(Debug, Clone)]
pub enum AnsiColor { ... }

impl AnsiColor {
    fn to_ansi_code(&self) -> &'static str { ... }
}

#[derive(Clone)]
pub struct CliUi;

impl CliUi {
    pub fn new() -> Self { ... }
    pub fn interface(&self, color: AnsiColor) -> CliInterface { ... }
    pub fn report_job_completion(...) -> impl Future<Output = ()> + Send { ... }
}
```

## 10. Demo: Initialization (`demo/init.rs`)

### Extract from prototype (lines ~702-730):
- `build_session()` function
- `build_ui()` function

### Key points:
- Depends on session and cli_interface modules
- AWS configuration and client setup
- Temporary demo code

### Module structure:
```rust
use aws_config::{BehaviorVersion, Region};
use aws_sdk_bedrockruntime::Client as BedrockClient;
use crate::agent_env::{Session, model_provider_impls::BedrockConverseStreamModelProvider};
use super::cli_interface::CliUi;

pub async fn build_session() -> Result<Session, anyhow::Error> { ... }

pub fn build_ui() -> CliUi { ... }
```

## Main Module (`mod.rs`)

### Structure:
```rust
// Core modules
pub mod worker_job_continuations;
pub mod model_provider;
pub mod model_provider_impls;
pub mod worker;
pub mod worker_task;
pub mod worker_job;
pub mod worker_interface;
pub mod session;

// Demo module
pub mod demo;

// Re-exports for convenience
pub use worker_job_continuations::{Continuations, JobState, WorkerJobCompletionType};
pub use model_provider::{
    ModelProvider, ModelRequest, ModelResponse, ModelResponseChunk, ToolRequest,
};
pub use model_provider_impls::BedrockConverseStreamModelProvider;
pub use worker::{Worker, WorkerStates};
pub use worker_task::WorkerTask;
pub use worker_job::WorkerJob;
pub use worker_interface::WorkerToHostInterface;
pub use session::Session;
```

## Demo Module (`demo/mod.rs`)

### Structure:
```rust
pub mod proto_loop;
pub mod cli_interface;
pub mod init;

pub use proto_loop::{WorkerProtoLoop, WorkerInput};
pub use cli_interface::{CliInterface, CliUi, AnsiColor};
pub use init::{build_session, build_ui};
```

## Extraction Order

Follow this order to minimize compilation errors:

1. Create directory structure
2. Create `mod.rs` with module declarations (empty modules)
3. Extract `worker_job_continuations.rs` (no dependencies)
4. Extract `model_provider.rs` (trait and data structures only)
5. Create `model_provider_impls/` subdirectory
6. Extract `model_provider_impls/bedrock_converse_stream.rs` (Bedrock implementation)
7. Create `model_provider_impls/mod.rs`
8. Extract `worker_interface.rs` (depends on worker, model_provider - use forward declarations)
9. Extract `worker.rs` (depends on model_provider_impls, worker_interface)
10. Extract `worker_task.rs` (depends on worker)
11. Extract `worker_job.rs` (depends on worker, worker_task, worker_job_continuations)
12. Extract `session.rs` (depends on worker, worker_job, worker_task, model_provider_impls)
13. Create `demo/mod.rs`
14. Extract `demo/cli_interface.rs` (depends on core modules)
15. Extract `demo/init.rs` (depends on session, cli_interface)
16. Extract `demo/proto_loop.rs` (depends on all core modules)

## Common Issues and Solutions

### Issue: Circular dependencies
**Solution**: Use forward declarations or restructure to break cycles

### Issue: Missing trait bounds
**Solution**: Add `Send + Sync + 'static` where needed for async/threading

### Issue: Lifetime issues with Arc
**Solution**: Clone Arc references instead of borrowing

### Issue: Async trait compilation errors
**Solution**: Ensure `#[async_trait::async_trait]` is on both trait and impl

### Issue: Mutex deadlocks
**Solution**: Keep lock scopes minimal, never hold multiple locks
