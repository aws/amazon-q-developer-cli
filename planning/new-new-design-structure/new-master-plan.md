# New design approach #2

## What are we doing today

We are trying to rebuild crates/chat-cli with new better architecture. 
We work in crates/chat-exp2.

There's some documentation in codebase/ folder. Look through its structure, read files you could need.


## Useful Files in codebase/ folder

### Core Architecture
- `codebase/chat-cli/llm-api.md` - LLM API initialization and usage patterns
- `codebase/ChatSession-state-machine.md` - Current state machine implementation
- `codebase/ChatSession-handle_response.md` - Response handling patterns
- `codebase/ChatSession-conversation-data.md` - Conversation data structures

### AWS CodeWhisperer Integration
- `codebase/aws-codewhisperer-clients.md` - Client initialization patterns
- `codebase/aws-codewhisperer-calls.md` - API call patterns and streaming
- `codebase/aws-codewhisperer-calls-example-request.json` - Request format examples
- `codebase/aws-codewhisperer-calls-example-response.json` - Response format examples

### Output and UI
- `codebase/chat-cli/ChatSession-output-audit.md` - Output handling audit
- `codebase/chat-cli/output-extraction-progress.md` - Progress extraction patterns
- `codebase/ChatArgs-summary.md` - CLI arguments structure

## Terminology
- **Session** - The main orchestrator containing model providers, workers, jobs, and thread pool. Manages the lifecycle of all components and provides worker creation and job execution capabilities.
- **Worker** - A stateful entity with unique ID, name, model provider, and execution state. Contains conversation context, tools configs, and trust state. Approximately equal to `SessionAgent`.
- **WorkerJob** - A running job that combines a worker, cancellation token, thread job, and worker task. Represents an active execution unit.
- **WorkerTask** - Interface for executable work units. Implemented by `WorkerProtoLoop` for the main worker execution logic.
- **WorkerProtoLoop** - The main execution loop that handles model requests, state transitions, and UI interactions. Runs asynchronously with cancellation support.
- **WorkerStates** - Enum defining worker execution states: INACTIVE, WORKING, REQUESTING, RECEIVING, WAITING, USING_TOOL, INACTIVE_FAILED.
- **WorkerToHostInterface** - Interface for worker-to-UI communication, handling state changes, response chunks, and tool confirmations.
- **ModelProvider** - Interface for LLM communication with streaming support and cancellation tokens.
- **Agent** - A set of configurations that define instructions and available tools (future concept).

## Work hints

**IMPORTANT** `cargo check` output can become extremely large after that, ONLY use temporary file + sub-q to analyze the output for any `cargo check` call
**IMPORTANT** Use command template like `cd /path/to/package && echo "Build started at: $(date)" && echo "Output file: /tmp/build_output_$(date +%s).txt" && cargo check > /tmp/build_output_$(date +%s).txt 2>&1 && echo "Build completed"`

