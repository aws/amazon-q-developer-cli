# Rust Implementation Approaches

## Key Challenges and Solutions

### 1. Worker State Management and Communication

**Challenge**: Worker state needs to be mutated by WorkerProtoLoop while being accessible to Session and UI.

**Solution**: Use `Arc<Mutex<WorkerState>>` pattern:
```rust
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub state: Arc<Mutex<WorkerStates>>,
    pub last_failure: Arc<Mutex<Option<anyhow::Error>>>,
    pub model_provider: Arc<dyn ModelProvider>,
}

impl Worker {
    fn set_state(&self, new_state: WorkerStates, interface: &dyn WorkerToHostInterface) {
        {
            let mut state = self.state.lock().unwrap();
            *state = new_state;
        }
        interface.worker_state_change(self.id, new_state);
    }
}
```

### 2. Cancellation Token Implementation

**Challenge**: Need cancellation that works across async boundaries and can be checked efficiently.

**Solution**: Use `tokio::CancellationToken`:
```rust
use tokio_util::sync::CancellationToken;

// In WorkerProtoLoop
async fn run(&self) -> Result<(), anyhow::Error> {
    tokio::select! {
        result = self.do_work() => result,
        _ = self.cancellation_token.cancelled() => {
            Err(anyhow::anyhow!("Operation cancelled"))
        }
    }
}
```

**Question**: In this shape it will terminate the loop run, but what about underlying processes? For example, if ModelProvider is receiving a response stream - will it break automatically? Or should it also handle cancellation token on its own?

### 3. Session Collections Management

**Challenge**: Concurrent access to workers and jobs collections.

**Solution**: Use `Arc<Mutex<Vec<T>>>` or `Arc<RwLock<Vec<T>>>`:
```rust
pub struct Session {
    model_providers: Vec<Arc<dyn ModelProvider>>, // Read-only after init
    workers: Arc<Mutex<Vec<Arc<Worker>>>>,
    jobs: Arc<Mutex<Vec<WorkerJob>>>,
    runtime: tokio::runtime::Handle,
}
```

### 4. WorkerToHostInterface Trait

**Challenge**: Async trait methods with proper lifetime management.

**Solution**: Use `async-trait` crate:
```rust
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

### 5. WorkerJob Ownership

**Challenge**: WorkerJob needs to own/reference multiple components with different lifetimes.

**Solution**: Use Arc for shared ownership:
```rust
pub struct WorkerJob {
    worker: Arc<Worker>,
    cancellation_token: CancellationToken,
    task_handle: tokio::task::JoinHandle<Result<(), anyhow::Error>>,
}

impl WorkerJob {
    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }
    
    pub async fn wait(self) -> Result<(), anyhow::Error> {
        match self.task_handle.await {
            Ok(result) => result,
            Err(join_error) => Err(anyhow::anyhow!("Task panicked: {}", join_error)),
        }
    }
}
```

### 6. ModelProvider Streaming

**Challenge**: Streaming responses with callbacks and cancellation.

**Solution**: Use channels and tokio::select:
```rust
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    async fn request(
        &self,
        request: ModelRequest,
        response_sender: tokio::sync::mpsc::UnboundedSender<ModelResponseChunk>,
        cancellation_token: CancellationToken,
    ) -> Result<ModelResponse, anyhow::Error>;
}
```

### 7. Communication Patterns

**Recommended Approaches**:

1. **State Updates**: Direct method calls with Arc<Mutex<T>>
2. **Streaming Data**: `tokio::sync::mpsc` channels
3. **UI Interaction**: Async trait methods with proper error handling
4. **Cancellation**: `tokio::CancellationToken` throughout

### 8. Error Handling Strategy

**Pattern**: Use `anyhow::Error` for application errors, `Result<T, E>` everywhere:
```rust
pub type AppResult<T> = Result<T, anyhow::Error>;

impl WorkerProtoLoop {
    async fn run(&self) -> AppResult<()> {
        self.worker.set_state(WorkerStates::Working, &*self.host_interface)?;
        // ... rest of implementation
        Ok(())
    }
}
```

### 9. Threading and Runtime

**Approach**: Use tokio runtime with proper task spawning:
```rust
impl Session {
    pub fn run(
        &self,
        worker: Arc<Worker>,
        input: WorkerInput,
        ui_interface: Arc<dyn WorkerToHostInterface>,
    ) -> AppResult<WorkerJob> {
        let cancellation_token = CancellationToken::new();
        let worker_loop = WorkerProtoLoop::new(
            worker.clone(),
            input,
            ui_interface,
            cancellation_token.clone(),
        );
        
        let task_handle = tokio::spawn(async move {
            worker_loop.run().await
        });
        
        let job = WorkerJob {
            worker,
            cancellation_token,
            task_handle,
        };
        
        self.jobs.lock().unwrap().push(job.clone());
        Ok(job)
    }
}
```

## Critical Implementation Notes

1. **Avoid Deadlocks**: Always acquire locks in consistent order
2. **Use Arc Sparingly**: Only for truly shared data
3. **Prefer Channels**: For producer-consumer patterns
4. **Handle Panics**: Use `tokio::task::JoinHandle` properly
5. **Cancellation Checks**: Add `cancellation_token.is_cancelled()` checks in long operations
6. **Error Propagation**: Use `?` operator consistently

## Potential Problematic Areas

1. **WorkerProtoLoop**: Needs careful lifetime management between worker, interface, and cancellation
2. **Session cleanup**: Jobs collection needs proper cleanup on completion
3. **ModelProvider callbacks**: Streaming with proper backpressure handling
4. **UI blocking operations**: Ensure cancellation works during user input
