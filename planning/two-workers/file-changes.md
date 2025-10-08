# Two Workers Demo - File Modification Checklist

## Files to Modify

### 1. text_ui_worker_to_host_interface.rs
**Path**: `crates/chat-cli/src/cli/chat/agent_env_ui/text_ui_worker_to_host_interface.rs`

**Changes**:
- [ ] Add `color_code: Option<&'static str>` field to struct
- [ ] Update `new()` to accept `color_code` parameter
- [ ] Modify `response_chunk_received()` to wrap text output with color codes
- [ ] Ensure `flush()` is called after colored output

**Lines to Change**: ~15-20 lines
**Complexity**: Low
**Risk**: Low

---

### 2. mod.rs (AgentEnvTextUi)
**Path**: `crates/chat-cli/src/cli/chat/agent_env_ui/mod.rs`

**Changes**:
- [ ] Add imports: `HashMap`, `Mutex`, `Uuid`
- [ ] Add `worker_interfaces: Arc<Mutex<HashMap<Uuid, Arc<dyn WorkerToHostInterface>>>>` field
- [ ] Initialize field in `new()`
- [ ] Rename `create_ui_interface()` to `get_worker_interface()`
- [ ] Implement lookup-first pattern: check map, create and store if not found
- [ ] Update `run()` to use `get_worker_interface()`

**Lines to Change**: ~30-40 lines
**Complexity**: Medium
**Risk**: Low-Medium (HashMap management)

---

### 3. mod.rs (ChatArgs)
**Path**: `crates/chat-cli/src/cli/chat/mod.rs`

**Changes**:
- [ ] Update startup message to indicate two workers
- [ ] Create two workers instead of one
- [ ] Pre-register colored interfaces using `get_worker_interface()`
- [ ] Launch first worker with input (if provided)
- [ ] Launch second worker with same input (if provided)
- [ ] Queue both workers for prompts (if no input)

**Lines to Change**: ~40-50 lines (mostly in execute() method)
**Complexity**: Medium
**Risk**: Medium (entry point changes)

---

## Detailed Change Breakdown

### File 1: text_ui_worker_to_host_interface.rs

#### Current Structure
```rust
pub struct TextUiWorkerToHostInterface {}

impl TextUiWorkerToHostInterface {
    pub fn new() -> Self {
        Self {}
    }
}
```

#### New Structure
```rust
pub struct TextUiWorkerToHostInterface {
    color_code: Option<&'static str>,
}

impl TextUiWorkerToHostInterface {
    pub fn new(color_code: Option<&'static str>) -> Self {
        Self { color_code }
    }
}
```

#### Method Changes

**response_chunk_received()** - Current:
```rust
ModelResponseChunk::AssistantMessage(text) => {
    print!("{}", text);
    io::stdout().flush().unwrap();
}
```

**response_chunk_received()** - New:
```rust
ModelResponseChunk::AssistantMessage(text) => {
    if let Some(color) = self.color_code {
        print!("{}{}\x1b[0m", color, text);
    } else {
        print!("{}", text);
    }
    io::stdout().flush().unwrap();
}
```

---

### File 2: mod.rs (AgentEnvTextUi)

#### New Imports
```rust
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;
```

#### Struct Changes

**Current**:
```rust
pub struct AgentEnvTextUi {
    session: Arc<Session>,
    input_handler: InputHandler,
    prompt_queue: Arc<PromptQueue>,
    shutdown_signal: Arc<Notify>,
}
```

**New**:
```rust
pub struct AgentEnvTextUi {
    session: Arc<Session>,
    input_handler: InputHandler,
    prompt_queue: Arc<PromptQueue>,
    shutdown_signal: Arc<Notify>,
    worker_interfaces: Arc<Mutex<HashMap<Uuid, Arc<dyn WorkerToHostInterface>>>>,
}
```

#### Constructor Changes

**Current**:
```rust
pub fn new(
    session: Arc<Session>,
    history_path: Option<PathBuf>,
) -> Result<Self, eyre::Error> {
    Ok(Self {
        session,
        input_handler: InputHandler::new(history_path)?,
        prompt_queue: Arc::new(PromptQueue::new()),
        shutdown_signal: Arc::new(Notify::new()),
    })
}
```

**New**:
```rust
pub fn new(
    session: Arc<Session>,
    history_path: Option<PathBuf>,
) -> Result<Self, eyre::Error> {
    Ok(Self {
        session,
        input_handler: InputHandler::new(history_path)?,
        prompt_queue: Arc::new(PromptQueue::new()),
        shutdown_signal: Arc::new(Notify::new()),
        worker_interfaces: Arc::new(Mutex::new(HashMap::new())),
    })
}
```

#### Method Changes

**Current**:
```rust
pub fn create_ui_interface(&self) -> Arc<dyn WorkerToHostInterface> {
    Arc::new(TextUiWorkerToHostInterface::new())
}
```

**New**:
```rust
pub fn get_worker_interface(
    &self,
    worker_id: Option<Uuid>,
    color: Option<&'static str>,
) -> Arc<dyn WorkerToHostInterface> {
    if let Some(id) = worker_id {
        let mut interfaces = self.worker_interfaces.lock().unwrap();
        
        // Check if interface already exists
        if let Some(interface) = interfaces.get(&id) {
            return interface.clone();
        }
        
        // Create and store new interface
        let interface = Arc::new(TextUiWorkerToHostInterface::new(color));
        interfaces.insert(id, interface.clone());
        interface
    } else {
        // No worker_id, create without storing
        Arc::new(TextUiWorkerToHostInterface::new(color))
    }
}
```

#### run() Method Changes

**Current** (relevant section):
```rust
let ui_interface = self.create_ui_interface();

let job = match self.session.run_agent_loop(
    request.worker.clone(),
    agent_input,
    ui_interface,
) {
    // ...
}
```

**New**:
```rust
// Get interface (will reuse if pre-registered)
let ui_interface = self.get_worker_interface(Some(request.worker.id), None);

let job = match self.session.run_agent_loop(
    request.worker.clone(),
    agent_input,
    ui_interface,
) {
    // ...
}
```

---

### File 3: mod.rs (ChatArgs)

#### Add Constants (at top of impl block or module level)
```rust
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_CYAN: &str = "\x1b[36m";
```

#### execute() Method - Complete Replacement

**Current**:
```rust
pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
    println!("Starting Agent Environment...");

    let session = Arc::new(crate::agent_env::demo::build_session().await?);
    let history_path = crate::util::directories::chat_cli_bash_history_path(os).ok();
    let ui = agent_env_ui::AgentEnvTextUi::new(session.clone(), history_path)?;
    
    let worker = session.build_worker();
    
    if let Some(input) = self.input {
        worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message(input);
        
        let worker_host_ui = ui.create_ui_interface();
        let job = session.run_agent_loop(
            worker.clone(),
            crate::agent_env::worker_tasks::AgentLoopInput {},
            worker_host_ui,
        )?;
        
        let continuation = ui.create_agent_completion_continuation();
        job.worker_job_continuations.add_or_run_now(
            "agent_to_prompt",
            continuation,
            worker.clone(),
        ).await;
    } else {
        let continuation = ui.create_agent_completion_continuation();
        continuation(worker, crate::agent_env::WorkerJobCompletionType::Normal, None).await;
    }
    
    tracing::info!("Launching UI loop");
    ui.run().await?;
    
    println!("Goodbye!");
    Ok(ExitCode::SUCCESS)
}
```

**New**:
```rust
pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
    println!("Starting Agent Environment with TWO WORKERS...");

    let session = Arc::new(crate::agent_env::demo::build_session().await?);
    let history_path = crate::util::directories::chat_cli_bash_history_path(os).ok();
    let ui = agent_env_ui::AgentEnvTextUi::new(session.clone(), history_path)?;
    
    // Create two workers
    let worker1 = session.build_worker();
    let worker2 = session.build_worker();
    
    // Pre-register colored interfaces (creates and stores in map)
    let green_code = "\x1b[32m";
    let cyan_code = "\x1b[36m";
    ui.get_worker_interface(Some(worker1.id), Some(green_code));
    ui.get_worker_interface(Some(worker2.id), Some(cyan_code));
    
    if let Some(input) = self.input {
        // Launch worker 1
        worker1.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message(input.clone());
        
        let job1 = session.run_agent_loop(
            worker1.clone(),
            crate::agent_env::worker_tasks::AgentLoopInput {},
            ui.get_worker_interface(Some(worker1.id), None),
        )?;
        
        let continuation1 = ui.create_agent_completion_continuation();
        job1.worker_job_continuations.add_or_run_now(
            "agent_to_prompt",
            continuation1,
            worker1.clone(),
        ).await;
        
        // Launch worker 2
        worker2.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message(input);
        
        let job2 = session.run_agent_loop(
            worker2.clone(),
            crate::agent_env::worker_tasks::AgentLoopInput {},
            ui.get_worker_interface(Some(worker2.id), None),
        )?;
        
        let continuation2 = ui.create_agent_completion_continuation();
        job2.worker_job_continuations.add_or_run_now(
            "agent_to_prompt",
            continuation2,
            worker2.clone(),
        ).await;
    } else {
        // Queue both workers for prompts
        let continuation = ui.create_agent_completion_continuation();
        continuation(worker1, crate::agent_env::WorkerJobCompletionType::Normal, None).await;
        continuation(worker2, crate::agent_env::WorkerJobCompletionType::Normal, None).await;
    }
    
    tracing::info!("Launching UI loop");
    ui.run().await?;
    
    println!("Goodbye!");
    Ok(ExitCode::SUCCESS)
}
```

---

## Implementation Order

1. **First**: text_ui_worker_to_host_interface.rs (smallest, independent change)
2. **Second**: mod.rs (AgentEnvTextUi) (depends on #1)
3. **Third**: mod.rs (ChatArgs) (depends on #2)
4. **Fourth**: Test and verify

---

## Compilation Checkpoints

### After File 1
```bash
cargo check --bin chat_cli
```
Expected: Should compile, no breaking changes to interface yet

### After File 2
```bash
cargo check --bin chat_cli
```
Expected: Will fail - ChatArgs still calls old create_ui_interface() signature

### After File 3
```bash
cargo check --bin chat_cli
```
Expected: Should compile successfully

---

## Testing Checklist

### Compilation
- [ ] `cargo check` passes
- [ ] No warnings about unused code
- [ ] No warnings about unused imports

### Functionality
- [ ] Single input mode: `q chat "test"`
- [ ] Interactive mode: `q chat`
- [ ] Colors appear correctly (green and cyan)
- [ ] Both workers produce output
- [ ] Ctrl+C cancels both workers
- [ ] No crashes or panics

### Edge Cases
- [ ] Empty input handling
- [ ] Very long input
- [ ] Rapid Ctrl+C
- [ ] Multiple sequential runs

---

## Rollback Plan

If issues arise, revert in reverse order:

1. Revert ChatArgs changes (restore single worker)
2. Revert AgentEnvTextUi changes (remove HashMap)
3. Revert TextUiWorkerToHostInterface changes (remove color)

Each step should leave code in compilable state.

---

## Estimated Time

- File 1: 15 minutes
- File 2: 30 minutes
- File 3: 45 minutes
- Testing: 30 minutes
- **Total**: ~2 hours

---

## Dependencies

- No new crate dependencies required
- Uses existing: `std::collections::HashMap`, `std::sync::Mutex`, `uuid::Uuid`
- All already in use elsewhere in codebase
