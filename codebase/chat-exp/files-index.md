# Some info about the initialization process

## Startup Call Chain
- [main()](../../crates/chat-cli/src/main.rs) - parses arguments, creates tokio runtime, passes to Cli.execute in...
- [Cli.execute()](../../crates/chat-cli/src/cli/mod.rs#L217) - sets up logger, creates `Os`, executes subcommand (below), closes telemetry
- [RootSubcommand.execute()](../../crates/chat-cli/src/cli/mod.rs#L139) - telemetry, passes to the actuall subcommand execution
  - subcommands are defined as a [enum RootSubcommand](../../crates/chat-cli/src/cli/mod.rs#L93)
  - We are intersted in `Chat(ChatArgs)`
  - `ChatArgs` are defined in "chat" folder:  [ChatArgs](../../crates/chat-cli/src/cli/chat/mod.rs#L210)
- **Chat entry point is** [`ChatArgs.execute()](../../crates/chat-cli/src/cli/chat/mod.rs#L238)
    - it makes a lot of checks and validations, creates some data
    - Mainly it kicks off [`ChatSession::new`] - [link](../../crates/chat-cli/src/cli/chat/mod.rs#L422)
    - TODO: review and list what kind of information is obtained and configured at this stage
- `ChatSession::new` - [link](../../crates/chat-cli/src/cli/chat/mod.rs#L604)



## Agent Environment Architecture

New parallel agent execution architecture. See [Agent Environment Documentation](../agent-environment/README.md) for complete details.

### Core Components
- [Worker](../agent-environment/worker.md) - Agent configuration and state management
  - Implementation: [worker.rs](../../crates/chat-cli/src/agent_env/worker.rs)
  - States: Inactive, Working, Requesting, Receiving, Waiting, UsingTool, InactiveFailed
- [Session](../agent-environment/session.md) - Central orchestrator for workers and jobs
  - Implementation: [session.rs](../../crates/chat-cli/src/agent_env/session.rs)
  - Manages worker creation, job launching, resource sharing
- [WorkerJob](../agent-environment/job.md) - Running task instance with lifecycle management
  - Implementation: [worker_job.rs](../../crates/chat-cli/src/agent_env/worker_job.rs)
  - Continuations: [worker_job_continuations.rs](../../crates/chat-cli/src/agent_env/worker_job_continuations.rs)

### Task System
- [WorkerTask Trait](../agent-environment/tasks.md) - Interface for executable work units
  - Trait definition: [worker_task.rs](../../crates/chat-cli/src/agent_env/worker_task.rs)
  - AgentLoop implementation: [agent_loop.rs](../../crates/chat-cli/src/agent_env/worker_tasks/agent_loop.rs)
  - Demo ProtoLoop: [proto_loop.rs](../../crates/chat-cli/src/agent_env/demo/proto_loop.rs)

### Communication
- [WorkerToHostInterface](../agent-environment/interface.md) - Worker-to-UI communication contract
  - Trait definition: [worker_interface.rs](../../crates/chat-cli/src/agent_env/worker_interface.rs)
  - CLI implementation: [cli_interface.rs](../../crates/chat-cli/src/agent_env/demo/cli_interface.rs)

### Model Providers
- [ModelProvider System](../agent-environment/model-provider.md) - LLM abstraction layer
  - Trait definition: [model_provider.rs](../../crates/chat-cli/src/agent_env/model_providers/model_provider.rs)
  - Bedrock implementation: [bedrock_converse_stream.rs](../../crates/chat-cli/src/agent_env/model_providers/bedrock_converse_stream.rs)

### Demo
- [Demo Implementation](../agent-environment/demo.md) - Working example with concurrent agents
  - Entry point: [init.rs](../../crates/chat-cli/src/agent_env/demo/init.rs)
  - Module exports: [mod.rs](../../crates/chat-cli/src/agent_env/mod.rs)
