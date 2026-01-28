# agent

Core LLM agent implementation for Kiro CLI. This crate provides the agent logic for handling
prompts, executing tools, managing conversation state, and coordinating with MCP servers.

## Agent Actor Architecture

The agent crate implements a tokio actor-based architecture. Each actor follows the
**handle/request/response pattern**: a `Handle` type provides an async API that sends typed
`Request` messages to the actor and awaits `Response` messages via oneshot channels.

### Actor Hierarchy

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                  Agent                                          │
│                                                                                 │
│  Core LLM agent that manages conversation state and coordinates subsystems.     │
│  Maintains a state machine tracking the current phase of execution.             │
│                                                                                 │
│  Key responsibilities:                                                          │
│  - Conversation state and history management                                    │
│  - Tool use parsing, permission evaluation, and execution coordination          │
│  - Hook execution (agent spawn, pre/post tool use, per-prompt)                  │
│  - Event broadcasting to consumers (e.g. AcpSession)                            │
└─────────────────────────────────────────────────────────────────────────────────┘
          │                    │                         │
          ▼                    ▼                         ▼
┌──────────────────┐  ┌─────────────────┐  ┌──────────────────────────────────────┐
│    AgentLoop     │  │  TaskExecutor   │  │            McpManager                │
│                  │  │                 │  │                                      │
│  Handles a       │  │  Executes tools │  │  Manages MCP server lifecycle.       │
│  single model    │  │  and hooks in   │  │  Routes tool calls to appropriate    │
│  request/response│  │  parallel on    │  │  servers.                            │
│  cycle. Parses   │  │  background     │  │                                      │
│  the streaming   │  │  tasks.         │  │  ┌────────────────────────────────┐  │
│  response and    │  │                 │  │  │      McpServerActor            │  │
│  extracts tool   │  │  Supports       │  │  │                                │  │
│  uses.           │  │  cancellation   │  │  │  Individual server actor that  │  │
│                  │  │  and caches     │  │  │  manages connection lifecycle  │  │
│  Created per     │  │  hook results.  │  │  │  and tool execution for a      │  │
│  user turn.      │  │                 │  │  │  single MCP server.            │  │
└──────────────────┘  └─────────────────┘  │  └────────────────────────────────┘  │
                                           └──────────────────────────────────────┘
```

### Agent State Machine

The `Agent` maintains an `ActiveState` that tracks the current phase of execution:

**State Descriptions:**
- `Idle` - Ready to receive a new prompt
- `ExecutingRequest` - Sending request to model and consuming response stream
- `ExecutingTools` - Tools are being executed in parallel via TaskExecutor
- `WaitingForApproval` - Blocked waiting for user to approve/deny tool execution
- `ExecutingHooks` - Running configured hooks (pre-tool, post-tool, etc.)
- `Compacting` - Context window overflow occurred; summarizing conversation history then retrying
- `Errored` - An error occurred; can recover by sending a new prompt

### User Turn Lifecycle

A "user turn" begins when a prompt is received and ends when the model returns a response
with no tool uses (or an error occurs). Within a turn, multiple request/response cycles
may occur as tools are executed and results sent back to the model.

```
User sends prompt
        │
        ▼
┌───────────────────┐
│ Run prompt hooks  │ (if configured)
└───────────────────┘
        │
        ▼
┌───────────────────────────────────────┐
│  Send request to model, parse stream  │◄─────────────────────────────────────┐
└───────────────────────────────────────┘                                      │
        │                                                                      │
        ▼                                                                      │
┌──────────────────┐                                                           │
│ Has tool uses?   │──No──► Turn ends                                          │
└──────────────────┘                                                           │
        │ Yes                                                                  │
        ▼                                                                      │
┌──────────────────┐                                                           │
│ Evaluate tool    │                                                           │
│ permissions      │                                                           │
└──────────────────┘                                                           │
        │                                                                      │
        ├───────────────┬───────────────┐                                      │
        ▼               ▼               ▼                                      │
    Allowed         Ask user        Denied ─────────────────────────────────────┤
        │               │                                                      │
        │               ▼                                                      │
        │       ┌──────────────┐                                               │
        │       │ Wait for     │                                               │
        │       │ approval     │                                               │
        │       └──────────────┘                                               │
        │          │         │                                                 │
        │      Approved   Denied ───────────────────────────────────────────────┤
        │          │                                                           │
        ▼          ▼                                                           │
┌────────────────────┐                                                         │
│ Run pre-tool hooks │                                                         │
└────────────────────┘                                                         │
        │         │                                                            │
     Allowed   Blocked ─────────────────────────────────────────────────────────┤
        │                                                                      │
        ▼                                                                      │
┌────────────────────┐                                                         │
│ Execute tools      │                                                         │
│ (TaskExecutor)     │                                                         │
└────────────────────┘                                                         │
        │                                                                      │
        ▼                                                                      │
┌────────────────────┐                                                         │
│ Run post-tool      │                                                         │
│ hooks              │                                                         │
└────────────────────┘                                                         │
        │                                                                      │
        └──────────────────────────────────────────────────────────────────────┘
```
