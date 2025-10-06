# Agent Environment Architecture

## Overview

The Agent Environment architecture enables **multiple AI agents to run in parallel**, each working independently on different tasks while sharing common infrastructure. This design supports having multiple specialized agents that can execute different tasks simultaneously without blocking each other.

## Key Design Goals

1. **Parallel Execution**: Multiple agents can run concurrently with independent state management
2. **Flexible Configuration**: Each agent can be customized with different parameters, tools, and behaviors
3. **Resource Sharing**: Agents share common resources (LLM providers, thread pools) efficiently
4. **Task Abstraction**: Different task types (agent loops, commands, orchestration) use the same execution framework
5. **Clean Lifecycle**: Proper cancellation, error handling, and resource cleanup

## Architecture Components

The architecture consists of several key components:

- **[Worker](./worker.md)**: Complete AI agent configuration (model provider, state, error tracking)
- **[WorkerTask](./tasks.md)**: Interface for executable work units (agent loops, commands, etc.)
- **[WorkerJob](./job.md)**: Running instance combining Worker + Task + execution infrastructure
- **[Session](./session.md)**: Central orchestrator managing all Workers and Jobs
- **[WorkerToHostInterface](./interface.md)**: Communication contract between Workers and UI layer
- **[ModelProvider](./model-provider.md)**: Abstraction for LLM communication

## Code Location

All implementation is in: `crates/chat-cli/src/agent_env/`

```
agent_env/
├── mod.rs                          # Module exports
├── worker.rs                       # Worker implementation
├── worker_task.rs                  # WorkerTask trait
├── worker_job.rs                   # WorkerJob implementation
├── worker_job_continuations.rs    # Job completion callbacks
├── worker_interface.rs             # WorkerToHostInterface trait
├── session.rs                      # Session orchestrator
├── model_providers/                # LLM provider abstractions
│   ├── model_provider.rs          # ModelProvider trait
│   └── bedrock_converse_stream.rs # AWS Bedrock implementation
├── worker_tasks/                   # Task implementations
│   ├── agent_loop.rs              # Main agent loop task
│   └── mod.rs
└── demo/                           # Demo implementations
    ├── proto_loop.rs              # Prototype task
    ├── cli_interface.rs           # CLI UI implementation
    └── init.rs                    # Demo initialization
```

## Execution Flow

1. **Session Creation**: Initialize with model providers
2. **Worker Creation**: Build workers with specific configurations
3. **Task Launch**: Create task (e.g., AgentLoop) and launch via Session
4. **Execution**: Task runs asynchronously, communicating via WorkerToHostInterface
5. **State Management**: Worker transitions through states (Working → Requesting → Receiving → Inactive)
6. **Completion**: Job completes normally, is cancelled, or fails with error

## State Machine

Workers transition through these states:

```
Inactive → Working → Requesting → Receiving → [Waiting/UsingTool]* → Inactive
                                                                    ↓
                                                              InactiveFailed
```

- **Inactive**: Worker idle, ready for new task
- **Working**: Preparing request
- **Requesting**: Sending request to LLM
- **Receiving**: Streaming response from LLM
- **Waiting**: Waiting for user input
- **UsingTool**: Executing tool
- **InactiveFailed**: Task failed with error

## Example Usage

```rust
// Create session with model provider
let session = Session::new(vec![model_provider]);

// Build worker
let worker = session.build_worker();

// Create task input
let input = AgentLoopInput {
    prompt: "Hello, world!".to_string(),
};

// Launch agent loop
let job = session.run_agent_loop(
    worker,
    input,
    ui_interface,
)?;

// Wait for completion
job.wait().await?;
```

## Related Documentation

- [Worker Details](./worker.md)
- [Task System](./tasks.md)
- [Job Management](./job.md)
- [Session Orchestration](./session.md)
- [UI Interface](./interface.md)
- [Model Providers](./model-provider.md)
- [Demo Implementation](./demo.md)
