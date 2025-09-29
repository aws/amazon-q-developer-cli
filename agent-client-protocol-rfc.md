# RFC: Agent Client Protocol Integration for Amazon Q CLI

- **Feature Name**: `acp`
- **Start Date**: 2025-09-14
- **RFC PR**: (TBD)
- **Amazon Q Issue**: (TBD)

## Summary

Add Agent Client Protocol (ACP) server capability to Amazon Q CLI, allowing editors like Zed and Neovim to use Q as an AI coding assistant through a standardized JSON-RPC interface.

**What is ACP?** Agent Client Protocol is a JSON-RPC standard that lets editors communicate with AI agents over stdio. Instead of building custom integrations for each editor, agents implement ACP once and work with any ACP-compatible editor.

**What this adds:** Users run `q acp` to start Q in server mode, then configure their editor to connect to this process. The editor handles the UI while Q provides the AI capabilities - same models, tools, and features as `q chat`.

## Motivation

**Problem:** Currently Q CLI provides two options to users: an interactive, CLI-based chat interface and a non-interactive mode. But some use-cases demand interaction in a programmatic or scripted fashion. This includes custom GUIs in editors, automation tools, IDEs, web interfaces, and other applications. Right now each application must adapt to each agent independently, meaning applications are likely only to build on the most widely used alternatives (e.g., the Claude Code SDK, which provides programmatic access to the Claude Code agent).

**Solution:** ACP provides an alternative, using a JSON-RPC protocol inspired by MCP to let any application integrate with any agent, sending user input and receiving the agent's responses in a streaming fashion.

**Immediate Benefits:** This provides immediate value to Q CLI users by allowing them to access Q from editors that support ACP (Zed, Neovim) with native integration - same models, tools, and MCP servers, but in their preferred editor instead of switching to terminal.

**Strategic Benefits:** Supporting ACP also helps boost the protocol itself. The more editors and agents use ACP, the more likely it will succeed as a standard. This avoids the problem of tooling being built atop proprietary options like the Claude Code SDK, which would lock Q CLI out of future editor integrations.

## Guide-level Explanation

### Setup

```bash
# Start Q in ACP server mode
q acp --agent my-profile

# Configure editor to connect to this process
# (Editor-specific configuration)
```

### User Experience

Once connected, users interact with Q through their editor's AI interface:

- **Chat with Q** in editor panels/sidebars
- **File operations** - Q can read/write files through the editor (sees unsaved changes)
- **Tool execution** - Editor-native permission prompts for tool use
- **Same capabilities** - Same models, agents, and MCP servers as terminal Q CLI

### Feature Gating

Following Q CLI's established pattern, ACP functionality is feature-gated:

```bash
# Enable ACP
q settings acp.enabled true

# Start ACP server
q acp --agent my-profile
```

This allows:
- **Controlled rollout** during development and testing
- **User opt-in** for ACP functionality
- **Consistent patterns** with other Q CLI features (tangent mode, todo list, etc.)

## Reference-level Explanation

### What is ACP?

Agent Client Protocol is a JSON-RPC protocol for editor-agent communication over stdio. Key concepts:

**Sessions** - Long-lived conversations between editor and agent. Each session has a unique ID and maintains conversation history.

**Prompts** - User messages sent to the agent via `session/prompt` method. The agent processes these and streams responses back.

**Streaming** - Responses stream incrementally via `session/update` notifications, allowing editors to display content as it's generated.

**Tools** - Agents can execute tools (file operations, shell commands, etc.) with permission checks via `session/request_permission`.

**Example message flow:**

```json
// Editor → Q: User sends a message
{"method": "session/prompt", "params": {"sessionId": "123", "messages": [...]}}

// Q → Editor: Streaming response
{"method": "session/update", "params": {"sessionId": "123", "update": {"AgentMessageChunk": {...}}}}

// Q → Editor: Tool execution request
{"method": "session/request_permission", "params": {"sessionId": "123", "tool": "fs_write", ...}}
```

### Mapping ACP to Q CLI

**Core Concept Mappings:**
- **ACP Sessions** → **Q Conversation IDs** (same UUID)
- **ACP Prompts** → **Q Chat pipeline** (reuse existing processing)
- **ACP Streaming** → **Q Response streaming** (protocol translation)
- **ACP Tool calls** → **Q Tool system** (existing infrastructure)
- **ACP Permissions** → **Q Agent permissions** (existing flow)

**Implementation Scope:** This initial implementation focuses on core chat functionality. The following ACP features are deferred to future iterations:
- Authentication (Q CLI currently has no authentication requirement in ACP mode)
- Session modes (ask/architect/code)
- Terminal operations (`terminal/create`, `terminal/output`, etc.)
- Agent plans (`session/update` with plan)
- Advanced tool call reporting (diff content, location tracking, terminal embedding)
- Slash commands
- Rich prompt content (images, audio - currently only text and resource links supported)

#### Session Lifecycle

The ACP session lifecycle has four key points:

**`initialize` message** - ACP connection setup
- Q CLI reports back with capabilities (protocol version, supported features).
- **Authentication:** Q CLI checks if the user is logged in (via `q login`):
  - If logged in → returns `authMethods: []` (no authentication required, sessions can be created immediately)
  - If NOT logged in → returns `authMethods: [{"id": "cli", "name": "CLI Login", "description": "Run 'q login' to authenticate"}]`
- If `authenticate` is called with `methodId: "cli"`, Q CLI re-checks login status and returns success if logged in, error otherwise.
- The actual login flow (`q login`) happens outside ACP - users must authenticate in a terminal before the editor can create sessions.

**`session/new`** - Create a new conversation
- Q CLI creates a new `SessionId` (a UUID) and a fresh `ConversationState` and adds it into the SQLite database.
- This uses the agent configured with `q acp --agent XXX` (or default agent).
- The session is configured with ACP-specific versions of built-in file system tools (`fs_read`, `fs_write`, etc.) that route through the ACP protocol instead of accessing the filesystem directly.
- ACP allows the caller to provide a list of MCP servers, these are **added to** the agent's base MCP configuration (additive approach).
- The working directory is specified in the ACP message and stored per session.
- Each session gets its own `ToolManager` instance with session-specific MCP servers.

**`session/load_session`** - Resume a session
- Q CLI fetches the `ConversationState` from the database and recreates the session context.

#### Prompt Handling

**`session/prompt`** - Incoming user message
- A new Q CLI `UserMessage` is created and added into the conversation state.
- The chat request is sent to the backend server and responses are streamed back via ACP `session/update` notifications.
- **Content types:** Q CLI advertises support for text (baseline), images, resource links (baseline), and embedded resources. Audio is not supported.
  - Text content is included directly in the prompt
  - Images are mapped to Q CLI's `ImageBlock` type
  - Resource links are converted to XML representation: `<resource uri="..."/>`
  - Embedded resources are converted to XML: `<resource uri="..." mime-type="...">content</resource>`

#### File Operations

File operations using ACP do not go directly to the file system. Instead they are directed over the protocol so that the editor can provide a simulated file system (including unsaved buffer contents).

When a session is created in ACP mode, Q CLI configures it with ACP-specific versions of built-in file system tools (`fs_read`, `fs_write`, etc.). These tool implementations route their operations through the ACP protocol (calling methods like `fs/read_text_file` on the client) rather than accessing the operating system directly. This allows the agent to see unsaved editor changes and lets the client track all file modifications.

#### Tool Execution

When using ACP, Q CLI requests permission to use a tool by sending a `session/request_permission` to the client and awaiting the response. Trusted tools will not trigger this flow.

When tool use occurs, Q CLI reports that tool use using ACP `ToolCall` messages. We begin by mapping all tool use output to plain text. Later PRs can explore how to provide custom output formats that ACP supports (e.g., diffs for `fs_write`, structured terminal output for `bash`).

#### Response Streaming

Q CLI converts its existing streaming responses to ACP `session/update` notifications as the model generates content. Text responses and code blocks are sent as `AgentMessageChunk` updates, allowing the editor to display them incrementally. Tool execution is reported through `ToolCall` messages when tools start and `ToolCallUpdate` messages as they progress and complete. This preserves Q CLI's existing streaming behavior while adapting it to ACP's notification model.

**Stop reasons:** When a prompt turn completes, Q CLI maps its completion state to ACP stop reasons:
- `ResponseEvent::EndStream` (normal completion) → `end_turn`
- Stream errors → `refusal`
- User cancellation → `cancelled` (when cancellation is implemented)
- Q CLI's backend does not currently expose `max_tokens` or `max_turn_requests` limits, so these stop reasons are not used.

#### Agent Plans

ACP supports agent plans - structured task breakdowns sent via `session/update` notifications with `SessionUpdate::Plan`. Q CLI has a todo system that could potentially map to this feature.

**Q CLI Todo System:**
- **Structure**: `TodoListState` contains tasks with `task_description` and `completed` boolean
- **Operations**: Create, Complete, Add, Remove, Load, Lookup via `TodoList` enum commands
- **Storage**: Persisted as JSON files in `.amazonq/cli-todo-lists/`
- **Feature gating**: Controlled by `chat.enableTodoList` setting

**ACP Plan Structure:**
- **Plan**: Contains a list of `PlanEntry` objects
- **PlanEntry**: Has `content` (description), `priority` (high/medium/low), and `status` (pending/in_progress/completed)
- **Updates**: Full plan replacement via `SessionUpdate::Plan` - agent sends complete entry list on each update

**Mapping Considerations:**

*Semantic differences:*
- Q CLI todos track *actual work being done* - they're a project management tool
- ACP plans communicate *agent's intended strategy* - they're a visibility/transparency mechanism
- Q CLI todos persist across sessions and can be resumed later
- ACP plans are session-scoped and describe the current turn's execution strategy

*Structural mapping:*
- Q CLI `Task.task_description` → ACP `PlanEntry.content` ✓
- Q CLI `Task.completed` → ACP `PlanEntry.status` (completed vs pending) - but Q CLI doesn't track `in_progress`
- Q CLI has no priority concept → ACP `PlanEntry.priority` would need default value
- Q CLI's `context` and `modified_files` have no ACP equivalent

*Implementation challenges:*
- Q CLI todo system is a tool that the model explicitly invokes, not automatic planning
- ACP plans are sent automatically as part of the agent's response streaming
- Would need to decide when to generate and send plan updates (after each TodoList tool call? on every prompt?)
- Current ACP session code ignores `SessionUpdate::Plan` entirely

**Decision:** Explicitly deferred for future implementation - requires feedback from Q CLI team.

The semantic mismatch between Q CLI's project management todos and ACP's strategy communication plans suggests they may serve different purposes. However, there are reasonable arguments for bridging them:

*Potential approaches:*
1. Automatically generate ACP plans from Q CLI todo state when the model uses the TodoList tool
2. Implement separate agent planning capability specifically for ACP visibility
3. Leave unimplemented and document that Q CLI doesn't expose execution plans through ACP

*Open questions for Q CLI team:*
- Should Q CLI's todo system be exposed to ACP clients as plans?
- If so, how should we handle the semantic differences (project management vs. visibility)?
- Should we implement automatic plan generation, or require explicit model support?
- What's the desired user experience when using Q through an ACP client like Zed?

#### Session Modes

ACP defines session modes as a general concept for dynamic agent behavior changes within a session. Q CLI has agent configurations but these serve a different purpose.

**Q CLI Agent Configurations:**
- Declarative JSON files specifying: name, prompt, tools, MCP servers, resources, hooks, model
- Selected when starting ACP server: `q acp --agent my-profile`
- Session-scoped - the agent is fixed for the lifetime of the session
- No concept of switching "modes" within a session

**ACP Session Modes:**
- General concept for dynamic behavior changes within a session
- Agents can define their own custom modes (ask/architect/code are just examples)
- Can be changed via `session/set_mode` method
- Mode changes can be triggered by user or agent
- Advertised in `session/new` and `session/load` responses via `modes` field

**Current Implementation:**
- Q CLI returns `modes: None` in `NewSessionResponse` and `LoadSessionResponse`
- `session/set_mode` returns `method_not_found` error
- This is valid per the ACP protocol - modes are optional

**Rationale:**
Q CLI agents are architectural configurations, not runtime behavioral modes. While Q CLI agents *could* theoretically be exposed as switchable ACP modes, this would require:
1. Loading multiple agent configurations per session
2. Implementing agent-switching logic mid-session
3. Deciding how to handle state transitions (conversation history, tool state, etc.)

**Decision:** Initial implementation does not support session modes. The current behavior (modes: None, set_mode returns error) is the intended design. Future work could explore exposing Q CLI agents as switchable modes if user demand warrants the complexity.

#### Terminal Operations

ACP supports terminal operations (`terminal/create`, `terminal/output`, `terminal/release`, etc.) for executing and managing shell commands with live output streaming. This could enhance Q CLI's bash tool integration.

**Q CLI's Current Bash Tool:**
- Executes shell commands and returns text output
- Supports background execution via `run_in_background` parameter
- Output is returned as plain text in tool call results

**ACP Terminal Operations:**
- `terminal/create` - Create a new terminal session
- `terminal/output` - Stream live output from terminal
- `terminal/release` - Clean up terminal resources
- Tool calls can reference terminal IDs for "follow-along" execution
- Enables rich terminal display in editors

**Future Enhancement Opportunity:**
When Q CLI runs the bash tool in ACP mode, it could:
1. Create an ACP terminal via `terminal/create` 
2. Execute commands in that terminal
3. Stream live output via `terminal/output` notifications
4. Reference the terminal ID in `ToolCall` messages
5. Provide much richer terminal experience in editors like Zed

**Decision:** Deferred for future implementation. The current text-based approach works for initial ACP support. Terminal operations would be a valuable enhancement for live command execution visibility in editors.

#### Advanced Tool Call Features

ACP tool calls support several advanced features that could enhance the editor experience but are not required for basic functionality.

**Available Features:**
- **Diff content**: Tool calls can report file modifications as diffs (old text vs new text). Useful for `fs_write` operations to show exactly what changed.
- **Location tracking**: Tool calls can report file paths and line numbers they're working with, enabling "follow-along" features in editors.
- **Terminal embedding**: Tool calls can embed live terminal output by referencing a terminal ID (requires terminal operations).
- **Detailed stop reasons**: Fine-grained reporting of why tool execution completed or failed.

**Current Implementation:**
Q CLI uses simple text content for all tool call reporting - tool results are converted to plain text and sent in `ToolCallUpdate` messages. This provides basic functionality without additional complexity.

**Decision:** All advanced tool call features are deferred for future implementation. The current text-based approach meets the goal of enabling minimal ACP support. These features can be added incrementally to improve the editor experience.

#### Cancellation

ACP defines detailed cancellation semantics that provide responsive user experience in editors. This should be implemented for the initial release.

**ACP Cancellation Requirements:**
- Client sends `session/cancel` notification
- Agent must abort LLM requests and tool executions
- Agent must respond to pending permission requests with `cancelled` outcome  
- Agent must send final `session/prompt` response with `cancelled` stop reason
- Updates may still arrive after cancellation but before final response

**Current State:**
Q CLI's current ACP implementation has cancellation as a no-op - `handle_cancel` exists but doesn't actually abort ongoing operations.

**Implementation Needs:**
1. **Session actor cancellation**: Add cancellation token/mechanism to `AcpSessionActor`
2. **LLM request abortion**: Cancel ongoing streaming requests to Q CLI's backend
3. **Tool execution abortion**: Stop running tools (especially long-running bash commands)
4. **Permission request cleanup**: Respond to pending `session/request_permission` calls with cancelled status
5. **Final response**: Send `PromptResponse` with `cancelled` stop reason

**Rationale:** Cancellation is important for editor responsiveness - users expect to be able to stop long-running AI operations. The implementation should be straightforward given Q CLI's actor-based architecture.

**Decision:** Implement proper cancellation semantics. This is essential for good editor UX and shouldn't require major architectural changes.

### Architecture

The ACP implementation uses an actor-based design for clean encapsulation of mutable state and canonical message ordering. Each actor is defined in its own module under `src/acp`.

**Why actors?** Actors own any piece of mutable state and serve as a "canonical ordering point" where needed. This eliminates shared mutable state (no RwLocks), provides natural backpressure through bounded channels, and enables clean testing through message injection.

#### Component Overview

```
┌─────────────┐    JSON-RPC      ┌─────────────┐    Actor Messages   ┌───────────────┐
│   Editor    │ ◄──────────────► │AcpAgentForward│ ◄─────────────────► │ AcpServerActor│
│ (Zed, etc.) │     (stdio)      │             │                     │               │
└─────────────┘                  └─────────────┘                     └───────────────┘
                                                                             │
                                                                    Actor Messages
                                                                             │
                                                                             ▼
                                                                   ┌───────────────┐
                                                                   │AcpSessionActor│
                                                                   │ (per session) │
                                                                   └───────────────┘
```

**AcpAgentForward** - Thin forwarding layer implementing `acp::Agent` trait
- Receives JSON-RPC requests from editor over stdio
- Forwards requests as messages to server actor via bounded channels
- Returns responses back to editor

**AcpServerActor** - Top-level coordinator managing sessions and routing messages
- Maintains `HashMap<SessionId, AcpSessionHandle>` for session routing
- Handles `initialize`, `new_session`, `load_session` methods
- Routes `session/prompt` and other session methods to appropriate session actor

**AcpSessionActor** - Per-session actors that own `ConversationState` and process prompts
- Each session actor owns its `ConversationState` and `ToolManager` instance
- Processes prompts with Q CLI's existing chat pipeline
- Streams responses back through the actor hierarchy

#### Message Flow

When an ACP client sends a prompt:

1. `AcpAgentForward` receives JSON-RPC request over stdio
2. Forwards as `ServerMethod::Prompt` message to server actor via channel
3. Server actor routes as `SessionMethod::Prompt` to appropriate session actor
4. Session actor processes with `ConversationState` and streams responses back
5. Response flows back through the same channel hierarchy to ACP client

#### Key Design Decisions

**Bounded channels** - Uses `mpsc::channel(32)` for message passing between actors
- Provides natural backpressure when actors can't keep up
- Prevents unbounded memory growth under load

**Response channels** - Uses `oneshot::channel()` for request-response patterns
- Each request carries a oneshot sender for the response
- Enables async/await style without shared state

**Error handling** - Internal code uses `eyre::Result`, protocol boundary uses `acp::Error`
- Clean separation between internal errors and protocol errors
- Conversion at the boundary in `AcpAgentForward`
- Errors unrelated to the protocol (e.g., internal failures, unexpected states) are converted to JSON-RPC "internal error" responses

**LocalSet** - Uses `tokio::task::LocalSet` for !Send ACP futures
- ACP library's futures are not Send
- LocalSet allows running them in a single-threaded context

**Session ownership** - Each session actor owns its mutable state
- No RwLocks or shared mutable state
- Actors provide canonical ordering point for session operations

**Cancellation** - Actor design enables clean cancellation
- Session actors can be sent cancel messages
- Can abort ongoing LLM requests and tool executions
- Natural place to implement proper cancellation semantics (currently no-op)