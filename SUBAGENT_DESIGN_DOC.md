# Kiro CLI Subagent Design Doc

## Introduction

### Overview
The Subagent feature enables Kiro CLI's main agent to delegate specialized tasks to focused sub-agents. Each subagent operates with its own context, agent configuration, and tool access, allowing for parallel execution of complex multi-step workflows.

### Goals
- Enable task delegation from the main agent to specialized subagents
- Support concurrent execution of multiple subagents (up to 4 simultaneously)
- Provide isolated execution contexts for each subagent
- Maintain telemetry and usage tracking for subagent invocations
- Ensure proper error handling and result aggregation

### Non-Goals
- Nested subagent spawning (subagents cannot spawn other subagents)
- Persistent subagent sessions across main agent conversations
- Direct user interaction with subagents during execution

## Architecture

### High-Level Design

The subagent system consists of three main components:

1. **UseSubagent Tool** (`use_subagent.rs`): LLM-facing tool interface
2. **Subagent Runtime** (`agent/mod.rs`): Execution engine for subagent queries
3. **SubagentIndicator UI**: Visual feedback component for concurrent subagent execution

```
┌─────────────────┐
│   Main Agent    │
│   (LLM Loop)    │
└────────┬────────┘
         │ invokes
         ▼
┌─────────────────┐
│  UseSubagent    │
│     Tool        │
└────────┬────────┘
         │ spawns
         ▼
┌─────────────────────────────────┐
│  Subagent Runtime (1-4 agents)  │
│  ┌──────┐ ┌──────┐ ┌──────┐    │
│  │ Sub1 │ │ Sub2 │ │ Sub3 │    │
│  └──────┘ └──────┘ └──────┘    │
└─────────────────────────────────┘
         │ returns
         ▼
┌─────────────────┐
│  Summary Tool   │
│    Results      │
└─────────────────┘
```

### Component Details

#### 1. UseSubagent Tool

**Location**: `crates/chat-cli/src/cli/chat/tools/use_subagent.rs`

**Commands**:
- `ListAgents`: Returns available agent configurations with descriptions
- `InvokeSubagents`: Spawns 1-4 subagents with specified queries

**Input Schema**:
```rust
struct InvokeSubagent {
    query: String,                    // Task for the subagent
    agent_name: Option<String>,       // Agent config to use
    relevant_context: Option<String>, // Additional context
}
```

**Validation**:
- Maximum 4 concurrent subagents
- Enabled via `UseSubagent` experiment flag

#### 2. Subagent Runtime

**Location**: `crates/chat-cli/src/agent/mod.rs`

**Key Features**:
- Independent conversation ID per subagent
- Embedded system messages for task context
- Summary tool enforcement via failsafe mechanism
- Telemetry tracking (token count, tool calls, duration)

**Execution Flow**:
1. Initialize agent with specified configuration
2. Inject embedded user messages:
   - Standard subagent context message
   - Optional relevant context from parent
3. Send query to agent
4. Monitor for summary tool invocation
5. If turn ends without summary, inject failsafe prompt
6. Aggregate results and telemetry

#### 3. Communication Protocol

**Event Flow**:
```
Input Events (from UI):
- Text: User input (ignored by subagents)
- Interrupt: Cancel subagent execution
- ToolApproval/ToolRejection: User consent for tool use

Output Events (to UI):
- ToolCallStart/ToolCallEnd: Tool execution lifecycle
- TextMessageContent: Agent response deltas
- MetaEvent: Turn completion with telemetry
- McpEvent: MCP server initialization status
```

### Data Flow

```
Main Agent
    │
    ├─> ListAgents
    │       └─> Returns: Map<agent_name, description>
    │
    └─> InvokeSubagents
            │
            ├─> Spawn Subagent 1 ──┐
            ├─> Spawn Subagent 2 ──┤
            ├─> Spawn Subagent 3 ──┼─> Concurrent Execution
            └─> Spawn Subagent 4 ──┘
                    │
                    ├─> Initialize Agent
                    ├─> Send Query
                    ├─> Execute Tools
                    ├─> Call Summary Tool
                    └─> Return Summary
                            │
                            ▼
            Aggregate Results
                    │
                    └─> {
                          "successes": [Summary],
                          "failures": [String]
                        }
```

## Implementation Details

### Subagent Lifecycle

1. **Initialization**
   - Create new `AgentSnapshot` with default settings
   - Generate unique conversation ID for RTS
   - Load agent configuration if specified
   - Initialize MCP manager
   - Inject embedded system messages

2. **Execution**
   - Wait for agent initialization (MCP servers, etc.)
   - Send user query
   - Process agent events in main loop
   - Handle tool approval requests
   - Monitor for summary tool invocation

3. **Termination**
   - Collect `UserTurnMetadata` (tokens, tool calls, duration)
   - Send telemetry to parent conversation
   - Return `Summary` result or error

### Summary Tool Enforcement

Subagents must call the `summary` tool to communicate results back to the main agent. This is enforced through:

1. **Embedded Message**: Instructs subagent to call summary tool
   ```
   "You are a subagent executing a task delegated to you by the main agent.
   After what is asked of you has concluded, call the summary tool to convey 
   your findings to the main agent."
   ```

2. **Failsafe Mechanism**: If turn ends without summary, inject reminder prompt
   ```
   "You have not called the summary tool yet. Please call the summary tool 
   now to provide your findings to the main agent before ending your task."
   ```

3. **Result Validation**: Only complete when summary is received

### Concurrency Model

- Up to 4 subagents execute concurrently using `futures::join_all`
- Each subagent has independent:
  - Agent instance
  - Conversation ID
  - Event channels
  - Tool execution context
- Results are partitioned into successes and failures

### Error Handling

**Failure Scenarios**:
- Agent initialization failure
- Channel closure (input/agent)
- User interruption
- Agent error during execution
- Missing summary result

**Error Propagation**:
- Individual subagent failures don't block others
- Failures collected as error strings in result
- Main agent receives both successes and failures

## Security Considerations

### Assumptions

| ID | Assumption | Comments |
|----|------------|----------|
| A-01 | Subagents operate within the same security boundary as the main agent | Inherits main agent's authentication and authorization |
| A-02 | Tool execution requires user approval unless explicitly bypassed | Critical for preventing unauthorized actions |
| A-03 | Each subagent has isolated conversation context | Prevents information leakage between concurrent subagents |
| A-04 | Maximum 4 concurrent subagents is sufficient for resource management | Prevents resource exhaustion attacks |

### Assets

| Asset Number | Asset Name | Asset Usage | Data Type | Comments |
|--------------|------------|-------------|-----------|----------|
| A-001 | Subagent Query | User-provided task description sent to subagent | Customer content | Stored temporarily in memory during execution |
| A-002 | Subagent Context | Additional context provided to subagent | Customer content | Optional, stored temporarily in memory |
| A-003 | Summary Results | Output from subagent execution | Customer content | Returned to main agent as JSON |
| A-004 | Conversation ID | Unique identifier for subagent RTS session | System metadata | Generated per subagent, tracked for telemetry |
| A-005 | Telemetry Data | Token count, tool calls, duration metrics | System metadata | Sent to telemetry service with parent conversation ID |

### Threat Actors

- A threat actor from the internet
- A threat actor acting with AWS customer permissions
- A malicious or compromised LLM attempting to abuse subagent capabilities

### Threats

| Threat Number | Priority | Threat | STRIDE | Mitigations | Status |
|---------------|----------|--------|--------|-------------|--------|
| T-01 | High | A malicious LLM can spawn excessive subagents to exhaust system resources which leads to denial of service for the user | Denial of Service | M-001 | Resolved |
| T-02 | Medium | A compromised subagent can access tools without user approval which leads to unauthorized actions on the user's system | Elevation of Privilege | M-002 | Resolved |
| T-03 | Medium | A malicious actor with access to the user's machine can intercept subagent communication channels which leads to information disclosure of sensitive query data | Information Disclosure | M-003 | Accepted Risk |
| T-04 | Medium | A subagent can fail to call the summary tool which leads to incomplete results being returned to the main agent | Tampering | M-004 | Resolved |
| T-05 | Low | Concurrent subagents can share state through global variables which leads to race conditions and data corruption | Tampering | M-005 | Resolved |
| T-06 | Medium | A malicious LLM can provide misleading context to subagents which leads to subagents performing unintended actions | Spoofing | M-006 | Accepted Risk |

### Mitigations

#### System Specific Mitigations

| Mitigation Number | Mitigation | Threats Mitigating | Status | Related BSC | Ticket/Artifact/CR/Tests | Comments |
|-------------------|------------|-------------------|--------|-------------|--------------------------|----------|
| M-001 | Hard limit of 4 concurrent subagents enforced in validation | T-01 | Resolved | N/A | Validation in `use_subagent.rs` | Prevents resource exhaustion |
| M-002 | Tool approval required by default for all subagent tool executions | T-02 | Resolved | N/A | Approval flow in `agent/mod.rs` | User must explicitly approve each tool call |
| M-003 | All communication uses in-memory channels, no network exposure | T-03 | Accepted Risk | N/A | N/A | Local machine compromise is out of scope |
| M-004 | Failsafe mechanism injects reminder prompt if summary tool not called | T-04 | Resolved | N/A | `SUMMARY_FAILSAFE_MSG` in `agent/mod.rs` | Ensures subagents always return results |
| M-005 | Each subagent has isolated agent instance and conversation ID | T-05 | Resolved | N/A | Independent `AgentSnapshot` per subagent | No shared mutable state |
| M-006 | Subagent context is user-controlled and visible in tool invocation | T-06 | Accepted Risk | N/A | N/A | User can review context in tool approval UI |

### Security Tests

- **Unit Tests**: Validation of max 4 subagents limit
- **Integration Tests**: Tool approval flow for subagent tool executions
- **Manual Testing**: Verify failsafe mechanism triggers when summary tool not called
- **Concurrency Tests**: Verify no shared state between concurrent subagents

### Tool Approval

- By default, subagents require user approval for tool execution
- `dangerously_trust_all_tools` flag can bypass approval (not recommended)
- Approval requests routed through UI with agent ID

### Context Isolation

- Each subagent has isolated conversation context
- No shared state between concurrent subagents
- Parent conversation ID tracked for telemetry only

### Resource Limits

- Maximum 4 concurrent subagents prevents resource exhaustion
- MCP initialization timeout: 24 hours (configurable)
- No nested subagent spawning

## Telemetry

**Metrics Tracked**:
- Token count per subagent
- Tool call count per subagent
- Execution duration per subagent
- Parent conversation ID for attribution

**Aggregation**:
```rust
SubagentExecutionSummary {
    tool_call_count: Option<u32>,
    token_count: u32,
    duration: Option<Duration>,
}
```

**Telemetry Event**:
```rust
telemetry_thread.send_subagent_invocation(
    parent_conversation_id,
    token_count,
    tool_call_count
)
```

## UI/UX

### SubagentIndicator

Visual component showing concurrent subagent execution:
- Agent name and query for each subagent
- Real-time status updates
- Tool execution indicators
- Completion summary with metrics

### User Interaction

- Users approve/reject tool calls per subagent
- Interrupt signal cancels all running subagents
- Results displayed after all subagents complete

## Testing

### Unit Tests
- Deserialization of `UseSubagent` commands
- Validation logic (max 4 subagents)

### Integration Tests
- `subagent_widget_demo`: Standalone test for UI component
- `test_sub_agent_routine`: Concurrent execution testing

### Test Scenarios
- Single subagent execution
- Multiple concurrent subagents
- Subagent failure handling
- Summary tool enforcement
- User interruption

## Configuration

### Experiment Flag
```rust
ExperimentManager::is_enabled(os, ExperimentName::UseSubagent)
```

### Agent Settings
```rust
AgentSettings {
    mcp_init_timeout: Duration::from_secs(86400), // 24 hours
}
```

### Tool Info
```rust
ToolInfo {
    spec_name: "use_subagent",
    preferred_alias: "subagent",
    aliases: &["use_subagent", "subagent"],
}
```

## Future Enhancements

### Potential Improvements
1. **Nested Subagents**: Allow subagents to spawn their own subagents
2. **Streaming Results**: Return partial results as subagents complete
3. **Priority Scheduling**: Execute high-priority subagents first
4. **Resource Quotas**: Per-subagent token/time limits
5. **Persistent Sessions**: Resume subagent execution across main agent turns
6. **Dynamic Concurrency**: Adjust max subagents based on system resources

### Known Limitations
- No direct user messaging to subagents during execution
- Fixed maximum of 4 concurrent subagents
- Summary tool must be called explicitly (no automatic extraction)
- Cannot inherit `dangerously_trust_all_tools` from parent session

## API Reference

### UseSubagent Tool

#### ListAgents Command
```json
{
  "command": "ListAgents"
}
```

**Response**:
```json
{
  "agent_name_1": "Description of agent 1",
  "agent_name_2": "Description of agent 2"
}
```

#### InvokeSubagents Command
```json
{
  "command": "InvokeSubagents",
  "content": {
    "subagents": [
      {
        "query": "Task description",
        "agent_name": "optional_agent_name",
        "relevant_context": "optional context"
      }
    ],
    "convo_id": "optional_parent_conversation_id"
  }
}
```

**Response**:
```json
{
  "successes": [
    {
      "summary": "Result from subagent",
      "metadata": {}
    }
  ],
  "failures": [
    "Error message from failed subagent"
  ]
}
```

## References

### Code Locations
- Entry Point: `crates/chat-cli/src/cli/chat/tools/use_subagent.rs`
- Runtime: `crates/chat-cli/src/agent/mod.rs`
- UI Component: `chat_cli_ui::subagent_indicator`
- Experiment Flag: `ExperimentName::UseSubagent`

### Related Documentation
- [Q CLI Rebrand Threat Model](https://quip-amazon.com/FYMbAmpbHDtk/Q-CLI-rebrand-to-Kiro-CLI-Threat-Model)
- Agent Configuration: `agent::agent_config::load_agents`
- Summary Tool: `agent::tools::summary::Summary`

### Dependencies
- `agent`: Core agent runtime
- `chat_cli_ui`: UI components and event protocol
- `rts`: Runtime Service client
- `tokio`: Async runtime
- `futures`: Concurrent execution utilities
