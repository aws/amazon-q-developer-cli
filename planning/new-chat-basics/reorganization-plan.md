# Agent Environment Reorganization Plan

## Overview

This document outlines the plan to reorganize the prototype from `crates/chat-exp/main.rs` into a maintainable structure under `crates/chat-cli/src/agent_env/`, and integrate it with `ChatArgs.execute()`.

## Goals

1. **Logical Component Breakdown**: Separate concerns into focused modules
2. **Production-Ready Core**: Keep core abstractions clean and extensible
3. **Demo Isolation**: Separate demo code from production components
4. **Gradual Evolution**: Enable incremental replacement with production code
5. **Clear Integration**: Simple entry point from ChatArgs.execute()

## Component Analysis

### Core Components (Production-Ready)

These components form the foundation and should be production-quality:

1. **Worker Job Continuations System** (`worker_job_continuations.rs`)
   - `JobState` enum
   - `WorkerJobCompletionType` enum
   - `WorkerJobContinuationFn` type alias
   - `Continuations` struct with latched state pattern
   - Purpose: Enable dynamic callback registration for job completion

2. **Model Provider** (`model_provider.rs` + `model_provider_impls/`)
   - `ModelRequest` struct
   - `ModelResponse` struct
   - `ModelResponseChunk` enum
   - `ToolRequest` struct
   - `ModelProvider` trait (async)
   - `model_provider_impls/bedrock_converse_stream.rs`: BedrockConverseStreamModelProvider implementation
   - Purpose: Abstract LLM communication with streaming support

3. **Worker** (`worker.rs`)
   - `WorkerStates` enum
   - `Worker` struct
   - State management methods
   - Purpose: Represent an agent configuration with state

4. **Worker Task** (`worker_task.rs`)
   - `WorkerTask` trait
   - Purpose: Define interface for executable work units

5. **Worker Job** (`worker_job.rs`)
   - `WorkerJob` struct
   - Job lifecycle methods (launch, cancel, wait)
   - Purpose: Combine worker, task, and execution infrastructure

6. **Session** (`session.rs`)
   - `Session` struct
   - Worker factory methods
   - Job launching and management
   - Purpose: Central orchestrator for workers and jobs

7. **Worker-to-Host Interface** (`worker_interface.rs`)
   - `WorkerToHostInterface` trait
   - Purpose: Define communication contract between workers and UI

### Demo Components (Temporary)

These components demonstrate the architecture but will be replaced:

1. **WorkerProtoLoop** (`demo/proto_loop.rs`)
   - `WorkerInput` struct
   - `WorkerProtoLoop` implementation of WorkerTask
   - Purpose: Demonstrate complete agent execution flow

2. **CLI Interface** (`demo/cli_interface.rs`)
   - `CliInterface` implementation of WorkerToHostInterface
   - `CliUi` factory
   - `AnsiColor` enum
   - Purpose: Demonstrate console-based UI implementation

3. **Initialization** (`demo/init.rs`)
   - `build_session()` function
   - `build_ui()` function
   - Purpose: Demo-specific initialization logic

## Directory Structure

```
crates/chat-cli/src/agent_env/
├── mod.rs                      # Main module with re-exports
├── worker_job_continuations.rs # Job continuation system
├── model_providers/            # ModelProvider trait and implementations
│   ├── mod.rs
│   ├── model_provider.rs       # ModelProvider trait
│   └── bedrock_converse_stream.rs # BedrockConverseStreamModelProvider
├── worker.rs                   # Worker struct and state management
├── worker_task.rs              # WorkerTask trait
├── worker_job.rs               # WorkerJob struct
├── worker_interface.rs         # WorkerToHostInterface trait
├── session.rs                  # Session orchestrator
└── demo/                       # Demo implementations (temporary)
    ├── mod.rs                  # Demo module exports
    ├── proto_loop.rs           # WorkerProtoLoop demo task
    ├── cli_interface.rs        # CLI UI implementation
    └── init.rs                 # Demo initialization functions
```

## Module Dependencies

```
mod.rs
  ├─> worker_job_continuations.rs (no deps)
  ├─> model_provider.rs (no deps)
  ├─> model_provider_impls/
  │     └─> bedrock_converse_stream.rs
  │           └─> model_provider.rs
  ├─> worker.rs
  │     └─> model_provider.rs
  │     └─> worker_interface.rs
  ├─> worker_task.rs
  │     └─> worker.rs
  ├─> worker_job.rs
  │     └─> worker.rs
  │     └─> worker_task.rs
  │     └─> worker_job_continuations.rs
  ├─> worker_interface.rs
  │     └─> worker.rs
  │     └─> model_provider.rs
  ├─> session.rs
  │     └─> worker.rs
  │     └─> worker_job.rs
  │     └─> worker_task.rs
  │     └─> model_provider.rs
  └─> demo/
        ├─> proto_loop.rs
        │     └─> worker.rs
        │     └─> worker_task.rs
        │     └─> worker_interface.rs
        │     └─> model_provider.rs
        ├─> cli_interface.rs
        │     └─> worker.rs
        │     └─> worker_interface.rs
        │     └─> model_provider.rs
        └─> init.rs
              └─> session.rs
              └─> cli_interface.rs
              └─> model_provider_impls/bedrock_converse_stream.rs
```

## Integration with ChatArgs.execute()

### Current State
```rust
impl ChatArgs {
    pub async fn execute(mut self, os: &mut Os) -> Result<ExitCode> {
        // TODO: This is where we plug in new entry point
        Ok(ExitCode::SUCCESS)
    }
}
```

### Proposed Integration

This is a direct production integration - the demo components will be replaced in the next iteration.

```rust
use crate::cli::chat::agent_env;
use crate::cli::chat::agent_env::demo::{build_session, build_ui, AnsiColor, WorkerInput};

impl ChatArgs {
    pub async fn execute(mut self, os: &mut Os) -> Result<ExitCode> {
        println!("Starting Agent Environment...");

        // Initialize session and UI
        let session = build_session().await?;
        let ui = build_ui();

        // Create workers
        let worker1 = session.build_worker();
        let worker2 = session.build_worker();

        // Launch jobs
        let job1 = session.run_demo_loop(
            worker1.clone(),
            WorkerInput {
                prompt: "lorem ipsum please, twice".to_string(),
            },
            Arc::new(ui.interface(AnsiColor::Cyan)),
        )?;

        let job2 = session.run_demo_loop(
            worker2.clone(),
            WorkerInput {
                prompt: "introduce yourself".to_string(),
            },
            Arc::new(ui.interface(AnsiColor::Green)),
        )?;

        // Add completion continuations
        let ui_clone = ui.clone();
        job1.worker_job_continuations.add_or_run_now(
            "completion_report",
            agent_env::Continuations::boxed(move |worker, completion_type, _error_msg| {
                ui_clone.report_job_completion(worker, completion_type)
            }),
            job1.worker.clone(),
        ).await;

        let ui_clone = ui.clone();
        job2.worker_job_continuations.add_or_run_now(
            "completion_report",
            agent_env::Continuations::boxed(move |worker, completion_type, _error_msg| {
                ui_clone.report_job_completion(worker, completion_type)
            }),
            job2.worker.clone(),
        ).await;

        // Run for a period then cancel
        tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
        session.cancel_all_jobs();

        // Wait for cleanup
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        println!("Completed");
        Ok(ExitCode::SUCCESS)
    }
}
```

## Migration Steps

### Phase 1: Create Core Module Structure
1. Create `agent_env/` directory
2. Create `mod.rs` with module declarations
3. Create empty files for each core module

### Phase 2: Extract Core Components
1. Extract continuations system → `worker_job_continuations.rs`
2. Extract model provider trait → `model_provider.rs`
3. Create `model_provider_impls/` subdirectory
4. Extract Bedrock implementation → `model_provider_impls/bedrock_converse_stream.rs`
5. Extract worker → `worker.rs`
6. Extract worker task → `worker_task.rs`
7. Extract worker job → `worker_job.rs`
8. Extract worker interface → `worker_interface.rs`
9. Extract session → `session.rs`

### Phase 3: Create Demo Module
1. Create `demo/` subdirectory
2. Create `demo/mod.rs`
3. Extract WorkerProtoLoop → `demo/proto_loop.rs`
4. Extract CLI interface → `demo/cli_interface.rs`
5. Extract initialization → `demo/init.rs`

### Phase 4: Integration
1. Add `pub mod agent_env;` to `chat/mod.rs`
2. Implement direct integration in `ChatArgs.execute()`
3. Test with `cargo run --bin chat_cli`

### Phase 5: Verification
1. Verify all components compile
2. Test demo execution
3. Verify cancellation behavior
4. Check continuation callbacks

## Key Design Principles

### 1. Separation of Concerns
- Core abstractions are independent of demo code
- Demo code depends on core, not vice versa
- UI interface is abstract, implementations are concrete

### 2. Async-First Design
- All I/O operations are async
- Cancellation tokens throughout
- Proper error propagation

### 3. Type Safety
- Strong typing for states and events
- Trait-based abstractions
- Arc/Mutex for shared state

### 4. Extensibility
- Easy to add new WorkerTask implementations
- Easy to add new ModelProvider implementations
- Easy to add new UI implementations

### 5. Testability
- Core components can be unit tested
- Mock implementations possible for all traits
- Demo code serves as integration test

## Future Evolution Path

### Short Term (Demo → Production)
1. Replace `WorkerProtoLoop` with `MainAgentLoop`
2. Replace `CliInterface` with production UI
3. Add conversation history to Worker
4. Add tools provider to Worker

### Medium Term (Feature Expansion)
1. Add context resources to Worker
2. Add request builder to Worker
3. Implement `/compact` as WorkerTask
4. Add orchestrator loop as WorkerTask

### Long Term (Full Production)
1. Replace demo initialization with production config
2. Add persistence for conversations
3. Add telemetry integration
4. Add error recovery mechanisms

## Testing Strategy

### Unit Tests
- Test continuations latching behavior
- Test worker state transitions
- Test job lifecycle
- Test cancellation propagation

### Integration Tests
- Test complete demo flow
- Test multi-worker scenarios
- Test cancellation during different states
- Test continuation callbacks

### Manual Testing
- Run demo with different prompts
- Test cancellation at different times
- Verify UI output correctness
- Check error handling

## Success Criteria

1. ✅ All core components compile without errors
2. ✅ Demo runs successfully with `cargo run --bin chat_cli`
3. ✅ Multiple workers run concurrently
4. ✅ Cancellation works correctly
5. ✅ Continuations fire on completion
6. ✅ Code is well-organized and documented
7. ✅ Clear path for production evolution

## Notes

- Keep the prototype in `chat-exp/` for reference during migration
- Document any deviations from the prototype
- Add inline comments explaining complex patterns
- This is a production step - demo components will be replaced in next iteration
