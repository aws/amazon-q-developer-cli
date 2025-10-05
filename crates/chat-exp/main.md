# AI Agent CLI Architecture Prototype

## The Goals

The primary goal of this architecture is to enable **multiple AI agents to run in parallel**, each working independently on different tasks while sharing common infrastructure. This design supports the core vision of having multiple specialized agents that can:

- Execute different tasks simultaneously without blocking each other
- Be configured independently with different parameters, tools, and behaviors
- Share common resources (LLM providers, thread pools) efficiently
- Be modified, added, or removed dynamically without affecting other agents

The secondary goal is **extreme flexibility in agent configuration**. Each "Worker" (agent) can be customized with:
- Different conversation contexts and histories
- Unique tool sets and permissions
- Specialized request builders and response handlers
- Independent state management and error handling
- Custom UI interfaces and interaction patterns

More importantly, each "Worker" (agent) can run completely different tasks ("WorkerTask"), that implement different logic and behavior
- General "Agent loop": LLM -> response -> Tools uses -> repeat
- "/compact" and its variations
- Orchestrator loop: (sub-agents states + available tasks) -> LLM -> (commands for sub-agents) -> repeat

This architecture enables scenarios like having a code analysis agent running alongside a documentation agent, while an orchestrator agent coordinating their work - all operating concurrently with their own specialized configurations.

## Core Elements

### Worker
The **Worker** represents a complete AI agent configuration - essentially "agent config JSON + conversation history + LLM access + tools" bundled into a single unit. Each Worker contains:
- Unique ID and name for identification
- Model provider for LLM communication
- Thread-safe state management (Inactive, Working, Requesting, Receiving, Waiting, UsingTool, InactiveFailed)
- Error tracking and failure handling
- Not in this demo: conversation history, context resources, tools provider

Workers are designed to be independent units that can be created, configured, and managed separately while sharing common infrastructure.

### WorkerTask
The **WorkerTask** trait defines the interface for executable work units. It abstracts different types of tasks that can be performed by a Worker:
- Main agent conversation loops
- Specialized commands (like `/compact`)
- Custom sub-agent orchestration
- Any finite agent operation

Tasks are designed to be self-contained and cancellable, implementing a specific piece of work for a given Worker without handling user interaction directly.

This is basically an interface for "Agent Loop", "Compact", and other kinds of tasks implementations.

### WorkerJob
The **WorkerJob** combines a Worker, a WorkerTask, and execution infrastructure into an active running unit. It manages:
- The Worker performing the task
- The specific WorkerTask being executed
- Cancellation token for clean shutdown
- Async task handle for the running operation

This is Worker + Task + Running thread (tokio task handler) combination

### Session
The **Session** serves as the central orchestrator managing all Workers and Jobs. It provides:
- Worker factory methods with shared model providers
- Job launching and lifecycle management
- Centralized cancellation for all running operations
- Thread pool management for concurrent execution
- Future: tools hosting, configuration management, conversation storage

The Session ensures proper resource sharing while maintaining isolation between different agents.

### WorkerToHostInterface
The **WorkerToHostInterface** defines the communication contract between Workers and the UI layer. It handles:
- State change notifications (worker transitions between states)
- Response chunk streaming (real-time LLM output)
- Tool confirmation requests (user approval for tool usage)
- Cancellable user interactions

This interface enables different UI implementations (CLI, web, API) to work with the same core Worker logic.

This interface will be expanded to provide more communication ways, but the input must be business-centric, and not depend on choosen UI platform or framework.

## Demo Elements

### WorkerProtoLoop (Demo Task)
The **WorkerProtoLoop** implements a demonstration WorkerTask that showcases the complete agent execution flow:
1. **Working**: Prepares and builds the model request
2. **Requesting**: Sends request to LLM provider
3. **Receiving**: Streams response chunks to UI
4. **Waiting**: Requests tool confirmation from user and then prints back user input
5. **Inactive**: Completes successfully or transitions to InactiveFailed

This demo task illustrates how real agent tasks would interact with the LLM, handle streaming responses, and coordinate with the UI layer.

### CliInterface (Demo UI)
The **CliInterface** provides a console-based implementation of WorkerToHostInterface:
- Colored state change notifications for visual distinction
- Real-time text streaming to stdout
- Interactive user input with cancellation support
- Color-coded output for multiple concurrent agents

The CLI interface demonstrates how different UI implementations can provide distinct user experiences while using the same core architecture.

### main() (Demo Application)
The **main()** function orchestrates a complete demonstration of the architecture:
1. **Initialization**: Creates Session with AWS Bedrock connectivity
2. **Multi-Agent Setup**: Creates two Workers with different prompts and colors
3. **Concurrent Execution**: Launches both agents simultaneously
4. **Cancellation Demo**: Shows graceful shutdown after a timeout
5. **State Inspection**: Displays final worker states for verification

This demo proves the architecture's ability to handle multiple concurrent agents with independent configurations, shared resources, and coordinated lifecycle management.

The demonstration showcases the key architectural benefits: agents running in parallel with different colors (Cyan and Green), different prompts ("lorem ipsum please, twice" vs "introduce yourself"), and independent state management while sharing the same Session infrastructure.
