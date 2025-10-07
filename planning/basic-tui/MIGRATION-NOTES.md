# Migration from PromptTask to AgentEnvTextUi Design

## Summary of Changes

The original design in `02-user-input.md` (and referenced in `03-ctrl-c-handling.md` and `04-graceful-shutdown.md`) used **PromptTask as a WorkerTask**. This has been changed to **AgentEnvTextUi managing prompts directly**.

## Key Architectural Changes

### Before (PromptTask Design)
- **PromptTask**: A WorkerTask that runs in the job system
- **Continuation chain**: AgentLoop → PromptTask → AgentLoop → PromptTask...
- **Session**: Manages both AgentLoop jobs and PromptTask jobs
- **Prompting**: Treated as an autonomous task in the job system

### After (AgentEnvTextUi Design)
- **No PromptTask**: Prompting is not a task/job
- **AgentEnvTextUi main loop**: Manages prompt queue and user interaction
- **Continuation chain**: AgentLoop completes → continuation re-queues prompt → UI processes prompt → UI launches AgentLoop
- **Session**: Only manages AgentLoop jobs (and other real tasks)
- **Prompting**: UI concern, not a job concern

## Rationale

1. **Prompting is not autonomous**: It requires user interaction, doesn't fit the "autonomous process" model
2. **Web API compatibility**: Web API won't have a "prompt task" - requests are async by nature
3. **Cleaner separation**: Session = job management, AgentEnvTextUi = UI management
4. **Multi-worker support**: Prompt queue naturally handles multiple workers waiting for input
5. **Simpler state**: No need to track prompt tasks in job system

## Impact on Other Design Docs

### 03-ctrl-c-handling.md
- **Status**: ✅ Updated
- **Changes**: 
  - Ctrl+C during prompt handled by InputHandler returning error (not PromptTask cancellation)
  - Continuations don't need shutdown checks - main loop handles it
  - Simplified shutdown detection

### 04-graceful-shutdown.md
- **Status**: ⚠️ Needs review (contains many PromptTask references)
- **Key changes needed**:
  - Remove `launch_initial_prompt()` method (main loop starts directly)
  - Remove PromptTask → AgentLoop continuation (only AgentLoop → prompt re-queue)
  - Shutdown happens by breaking main loop, not stopping continuation chain
  - History saving happens in main loop cleanup, not separate shutdown method

### Other files
- `00-overview.md`: ✅ No changes needed (high-level overview)
- `01-job-cleanup.md`: ✅ No changes needed (Session job management unchanged)
- `05-complete-flow-example.md`: ⚠️ May need review if it shows PromptTask
- `06-diagrams.md`: ⚠️ May need review if diagrams show PromptTask

## New Components

### PromptQueue
- **Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/prompt_queue.rs`
- **Purpose**: Queue prompt requests from multiple workers (FIFO)
- **Methods**: `enqueue()`, `dequeue()`, `is_empty()`, `len()`

### AgentEnvTextUi Main Loop
- **Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/mod.rs`
- **Purpose**: Main UI loop that processes prompt queue
- **Flow**:
  1. Dequeue next prompt request
  2. Read user input via InputHandler
  3. Handle commands (/quit)
  4. Push input to ConversationHistory
  5. Launch AgentLoop with continuation
  6. Continuation re-queues prompt when job completes

### InputHandler (Simplified)
- **Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/input_handler.rs`
- **Changes**: 
  - No `last_input` tracking (not needed)
  - Takes worker name as parameter to `read_line()`
  - Simpler interface

## Migration Checklist for 04-graceful-shutdown.md

- [ ] Remove `launch_initial_prompt()` method
- [ ] Update `run()` to start main loop directly
- [ ] Remove PromptTask → AgentLoop continuation
- [ ] Update AgentLoop → prompt continuation to just re-queue
- [ ] Update shutdown flow to break main loop
- [ ] Move history saving to main loop cleanup
- [ ] Update all code examples
- [ ] Update flow diagrams

## Migration Checklist for 05-complete-flow-example.md

- [ ] Review for PromptTask references
- [ ] Update flow to show AgentEnvTextUi main loop
- [ ] Update continuation examples

## Migration Checklist for 06-diagrams.md

- [ ] Review all diagrams for PromptTask
- [ ] Update sequence diagrams to show main loop
- [ ] Update state diagrams to remove PromptTask state

## Testing Impact

Tests that mock PromptTask will need to be rewritten to:
- Mock InputHandler instead
- Test AgentEnvTextUi main loop directly
- Test prompt queue behavior
- Test continuation re-queueing

## Implementation Order (Updated)

1. ✅ Create `PromptQueue` with enqueue/dequeue
2. ✅ Create `InputHandler` with readline
3. ✅ Create `AgentEnvTextUi` with main loop
4. ✅ Wire up continuation from AgentLoop to re-queue prompt
5. Test single iteration (prompt → agent → prompt)
6. Add /quit command handling
7. Add Ctrl+C handling during prompt
8. Add history persistence on shutdown
9. Test with multiple workers (future iteration)
