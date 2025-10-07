# Two Workers Demo - Implementation Plan

## Goal
Demonstrate the core feature of the Agent Environment Architecture: running multiple AI agents in parallel. This demo will show two workers producing output simultaneously with color-coded output to distinguish them.

## Scope
This is a minimal proof-of-concept focused on visual demonstration. UI will be "shaky" (interleaved output) - that's expected and will be addressed in future iterations.

## Architecture Changes

### 1. TextUiWorkerToHostInterface - Add Color Support
**File**: `crates/chat-cli/src/cli/chat/agent_env_ui/text_ui_worker_to_host_interface.rs`

**Changes**:
- Add optional `color_code: Option<&'static str>` field to struct
- Update constructor to accept optional color parameter
- Modify `response_chunk_received()` to wrap output with color codes when color is set

**Implementation**:
```rust
pub struct TextUiWorkerToHostInterface {
    color_code: Option<&'static str>,
}

impl TextUiWorkerToHostInterface {
    pub fn new(color_code: Option<&'static str>) -> Self {
        Self { color_code }
    }
}

// In response_chunk_received:
ModelResponseChunk::AssistantMessage(text) => {
    if let Some(color) = self.color_code {
        print!("{}{}\x1b[0m", color, text);
    } else {
        print!("{}", text);
    }
    io::stdout().flush().unwrap();
}
```

**Rationale**: Reuses existing pattern from `CliInterface` in demo code. Minimal change to existing structure.

### 2. AgentEnvTextUi - Worker-Specific UI Interfaces
**File**: `crates/chat-cli/src/cli/chat/agent_env_ui/mod.rs`

**Changes**:
- Add `worker_interfaces: Arc<Mutex<HashMap<Uuid, Arc<dyn WorkerToHostInterface>>>>` field
- Rename `create_ui_interface()` to `get_worker_interface()`
- Update signature to accept optional worker_id and color
- Check map first, create and store if not present
- Update `run()` to use `get_worker_interface()`

**Implementation**:
```rust
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

pub struct AgentEnvTextUi {
    session: Arc<Session>,
    input_handler: InputHandler,
    prompt_queue: Arc<PromptQueue>,
    shutdown_signal: Arc<Notify>,
    worker_interfaces: Arc<Mutex<HashMap<Uuid, Arc<dyn WorkerToHostInterface>>>>,
}

impl AgentEnvTextUi {
    pub fn new(...) -> Result<Self, eyre::Error> {
        Ok(Self {
            // ... existing fields
            worker_interfaces: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    pub fn get_worker_interface(
        &self,
        worker_id: Option<Uuid>,
        color: Option<&'static str>,
    ) -> Arc<dyn WorkerToHostInterface> {
        // If worker_id provided, check map first
        if let Some(id) = worker_id {
            let mut interfaces = self.worker_interfaces.lock().unwrap();
            
            if let Some(interface) = interfaces.get(&id) {
                return interface.clone();
            }
            
            // Not found, create and store
            let interface = Arc::new(TextUiWorkerToHostInterface::new(color));
            interfaces.insert(id, interface.clone());
            interface
        } else {
            // No worker_id, create without storing
            Arc::new(TextUiWorkerToHostInterface::new(color))
        }
    }
    
    pub async fn run(mut self) -> Result<(), eyre::Error> {
        // ... existing code until job launch
        
        // Get interface (will reuse if exists)
        let ui_interface = self.get_worker_interface(Some(request.worker.id), None);
        
        let job = self.session.run_agent_loop(
            request.worker.clone(),
            agent_input,
            ui_interface,
        )?;
        
        // ... rest of code
    }
}
```

**Rationale**: Single method handles both lookup and creation. Simpler API, no need to store references in ChatArgs.

### 3. ChatArgs Entry Point - Launch Two Workers
**File**: `crates/chat-cli/src/cli/chat/mod.rs`

**Changes**:
- Create two workers instead of one
- Pre-register UI interfaces with colors using `get_worker_interface()`
- Launch both workers with initial input (or queue both for prompts)

**Implementation**:
```rust
impl ChatArgs {
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
        
        // Launch both workers
        if let Some(input) = self.input {
            // Worker 1
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
            
            // Worker 2
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
            // Queue both for prompts
            let continuation = ui.create_agent_completion_continuation();
            continuation(worker1, crate::agent_env::WorkerJobCompletionType::Normal, None).await;
            continuation(worker2, crate::agent_env::WorkerJobCompletionType::Normal, None).await;
        }
        
        tracing::info!("Launching UI loop");
        ui.run().await?;
        
        println!("Goodbye!");
        Ok(ExitCode::SUCCESS)
    }
}
```

**Rationale**: Pre-register interfaces with colors, then subsequent calls to `get_worker_interface()` return the same instances. No need to store references.

## Color Codes
- **Worker 1**: Green (`\x1b[32m`)
- **Worker 2**: Cyan (`\x1b[36m`)
- **Reset**: `\x1b[0m` (already used in existing code)

## Expected Behavior

### With Input Provided
```bash
q chat "Tell me a joke"
```
- Both workers receive same input
- Both start processing simultaneously
- Output appears interleaved with green and cyan colors
- Both workers complete and return to prompt queue

### Without Input (Interactive)
```bash
q chat
```
- Both workers queued for prompts
- First prompt goes to worker 1 (green)
- After completion, worker 1 re-queued
- Second prompt goes to worker 2 (cyan)
- Alternating pattern continues

## Known Issues & Limitations

### 1. Interleaved Output (Expected)
**Issue**: Output from both workers will be mixed together character-by-character.

**Example**:
```
Why did the chicken cross the road?
To get to the other side!
```
Might appear as:
```
Why did the chicken cross the road?
To get to the other side!
```
(with colors interleaved)

**Status**: This is EXPECTED for this iteration. Will be addressed in future TUI work.

### 2. Prompt Queue Behavior
**Issue**: Only one worker can be prompted at a time due to single input handler.

**Status**: This is correct behavior. The demo shows parallel execution, not parallel input.

### 3. State Change Messages
**Issue**: `worker_state_change()` in TextUiWorkerToHostInterface only logs to tracing, not visible to user.

**Status**: Acceptable for demo. Could add colored state messages if needed.

### 4. Tool Confirmation
**Issue**: `get_tool_confirmation()` auto-approves all tools. With two workers, this could cause confusion.

**Status**: Acceptable for demo. Real implementation will need proper tool approval UI.

### 5. Worker Identification
**Issue**: No visual indication of which worker is which beyond color.

**Improvement**: Could add worker name/ID prefix to output:
```rust
print!("{}[W1] {}\x1b[0m", color, text);
```

**Decision**: Keep minimal for now. Add if needed during testing.

## Testing Strategy

### Manual Test Cases

1. **Single Input, Two Workers**
   ```bash
   q chat "Count to 10"
   ```
   Expected: Both workers count simultaneously, output interleaved in green/cyan

2. **Interactive Mode**
   ```bash
   q chat
   > Hello
   > What's 2+2?
   ```
   Expected: First prompt to worker 1 (green), second to worker 2 (cyan)

3. **Long Running Tasks**
   ```bash
   q chat "Write a long story about a robot"
   ```
   Expected: Both workers stream output simultaneously, colors help distinguish

4. **Ctrl+C Handling**
   ```bash
   q chat "Count to 100"
   ^C
   ```
   Expected: Both workers cancelled cleanly

## Implementation Order

1. **TextUiWorkerToHostInterface** - Add color support (smallest change)
2. **AgentEnvTextUi** - Add worker interface map and lookup
3. **ChatArgs** - Launch two workers
4. **Test** - Verify parallel execution with colors
5. **Document** - Update demo documentation

## Future Improvements (Out of Scope)

- Proper TUI with separate output regions per worker
- Worker name/ID display
- Interactive worker selection for prompts
- Tool approval UI for multiple workers
- Worker-specific command handling (e.g., `/cancel worker1`)

## Success Criteria

- [ ] Two workers launch successfully
- [ ] Both workers process same input simultaneously
- [ ] Output is color-coded (green and cyan)
- [ ] Both workers complete and return to prompt queue
- [ ] Interactive mode alternates between workers
- [ ] Ctrl+C cancels both workers cleanly
- [ ] No crashes or deadlocks

## Risk Assessment

### Low Risk
- Color code addition (proven pattern from demo code)
- Worker interface map (standard HashMap usage)

### Medium Risk
- Interleaved output readability (expected, but might be worse than anticipated)
- Prompt queue behavior with two workers (might need adjustment)

### Mitigation
- Keep changes minimal and reversible
- Test thoroughly with different input types
- Document known issues clearly
- Plan for TUI improvements in next iteration
