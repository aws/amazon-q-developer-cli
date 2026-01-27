---
doc_meta:
  validated: 2026-01-23
  commit: f4ef478f
  status: validated
  testable_headless: true
  category: tool
  title: use_subagent
  description: Delegate tasks to specialized subagents running in parallel with isolated context
  keywords: [use_subagent, subagent, delegate, parallel, multi-agent, availableAgents, trustedAgents]
  related: [delegate, slash-agent]
---

# use_subagent

Delegate tasks to specialized subagents running in parallel with isolated context.

## Overview

The use_subagent tool enables spawning up to 4 specialized subagents simultaneously to handle independent tasks in parallel. Each subagent operates with isolated context, preventing main conversation bloat. Subagents can use different agent configurations and report findings back via summary tool.

## How It Works

Main agent invokes use_subagent with queries for each subagent. Subagents start in parallel, each with own context and agent config. They execute independently, use tools as needed, and summarize findings. Main agent receives all summaries and synthesizes results.

## Usage

### Basic Usage

```json
{
  "command": "InvokeSubagents",
  "content": {
    "subagents": [{
      "query": "Research React performance metrics"
    }]
  }
}
```

### Common Use Cases

#### Use Case 1: Parallel Research

```json
{
  "command": "InvokeSubagents",
  "content": {
    "subagents": [
      {"query": "Research React performance", "agent_name": "research-agent"},
      {"query": "Research Vue.js performance", "agent_name": "research-agent"},
      {"query": "Research Angular performance", "agent_name": "research-agent"}
    ]
  }
}
```

**What this does**: Spawns 3 subagents to research different frameworks in parallel. Each returns summary to main agent.

#### Use Case 2: Task with Context

```json
{
  "command": "InvokeSubagents",
  "content": {
    "subagents": [{
      "query": "Analyze the authentication flow",
      "relevant_context": "Focus on OAuth2 implementation in auth.ts"
    }]
  }
}
```

**What this does**: Provides additional context to help subagent understand task scope.

#### Use Case 3: List Available Agents

```json
{
  "command": "ListAgents"
}
```

**What this does**: Returns all available agents with descriptions for delegation.

#### Use Case 4: Trusted Tool Execution

```json
{
  "command": "InvokeSubagents",
  "content": {
    "subagents": [{
      "query": "Run tests and analyze results",
      "dangerously_trust_all_tools": true
    }]
  }
}
```

**What this does**: Subagent executes all tools without approval prompts. Use with caution.

## Configuration

Control which agents can be used as subagents via toolsSettings in your agent configuration:

```json
{
  "toolsSettings": {
    "subagent": {
      "availableAgents": ["research-agent", "code-agent", "test-*"],
      "trustedAgents": ["research-agent"]
    }
  }
}
```

**availableAgents** (array, optional): Controls which agents appear in ListAgents and can be invoked. Supports exact names and glob patterns (e.g., `"test-*"`). If not set, all agents are available.

**trustedAgents** (array, optional): Controls which available agents are auto-approved without user confirmation. Supports exact names and glob patterns. Alias: `allowedAgents` for backwards compatibility. If not set, all invocations require approval.

**Permission Flow**:
1. Check if agent is in `availableAgents` → If not, deny with error
2. Check if agent is in `trustedAgents` → If yes, auto-approve; otherwise, ask for confirmation

Enable subagent feature:

```bash
kiro-cli settings chat.enableSubagent true
```

## Commands

### ListAgents

Query available agents for delegation.

**Parameters**: None

**Returns**: Map of agent names to descriptions.

### InvokeSubagents

Spawn subagents to handle tasks.

**Parameters**:
- `subagents` (array, required): List of subagent invocations (max 4)
  - `query` (string, required): Task for subagent
  - `agent_name` (string, optional): Specific agent to use (default: default agent)
  - `relevant_context` (string, optional): Additional context for task
  - `dangerously_trust_all_tools` (boolean, optional): Auto-approve all tools (default: false)
  - `is_interactive` (boolean, optional): Allow user interaction (default: false)
- `convo_id` (string, optional): Conversation ID for tracking

## Visual Indicator

Real-time status display shows:
- Subagent status (starting up, thinking, calling tools, summarizing)
- Animated spinner for active subagents
- Current activity messages
- Tool approval requests
- MCP server loading status
- Execution summary (tool uses, duration)

**Controls**:
- `j/↓` - Navigate down
- `k/↑` - Navigate up
- `y` - Approve tool use
- `n` - Deny tool use
- `Enter` - Copy OAuth URL
- `Ctrl+C` - Interrupt all
- `Esc` - Deselect

## Examples

### Example 1: Multi-Framework Comparison

```json
{
  "command": "InvokeSubagents",
  "content": {
    "subagents": [
      {"query": "Analyze React bundle size and performance"},
      {"query": "Analyze Vue.js bundle size and performance"},
      {"query": "Analyze Svelte bundle size and performance"}
    ]
  }
}
```

### Example 2: Code Analysis Tasks

```json
{
  "command": "InvokeSubagents",
  "content": {
    "subagents": [
      {"query": "Find all TODO comments in codebase", "agent_name": "code-agent"},
      {"query": "Analyze test coverage", "agent_name": "test-agent"}
    ]
  }
}
```

### Example 3: Documentation Generation

```json
{
  "command": "InvokeSubagents",
  "content": {
    "subagents": [
      {"query": "Document API endpoints", "relevant_context": "Focus on REST API in routes/"},
      {"query": "Document database schema", "relevant_context": "Focus on models/ directory"}
    ]
  }
}
```

## Troubleshooting

### Issue: "Agent 'X' is not available to be used as SubAgent"

**Symptom**: Error when trying to invoke a specific agent  
**Cause**: Agent not in `availableAgents` list in toolsSettings  
**Solution**: Add the agent to `availableAgents` in your agent's toolsSettings, or use a glob pattern that matches it.

### Issue: "You can only spawn 4 or fewer subagents"

**Symptom**: Error when trying to spawn >4 subagents  
**Cause**: Hard limit of 4 concurrent subagents  
**Solution**: Split tasks into batches of 4 or fewer.

### Issue: Subagent Hangs

**Symptom**: Subagent stuck in "thinking" state  
**Cause**: Waiting for tool approval or infinite loop  
**Solution**: Press `Ctrl+C` to interrupt. Check if tool approval needed.

### Issue: Context Not Passed

**Symptom**: Subagent doesn't understand task  
**Cause**: Missing relevant_context  
**Solution**: Provide relevant_context with specific details about task scope.

### Issue: Wrong Agent Used

**Symptom**: Subagent uses default agent instead of specified  
**Cause**: agent_name not found  
**Solution**: Use ListAgents to verify agent name exists.

### Issue: Tool Approval Prompts

**Symptom**: Subagent waiting for tool approval  
**Cause**: Tools not trusted  
**Solution**: Press `y` to approve, or set `dangerously_trust_all_tools: true` (use with caution).

## Related Features

- [delegate](delegate.md) - Background async agent execution
- [/agent](../slash-commands/agent-switch.md) - Switch agents in main conversation
- [Agent Configuration](../agent-config/overview.md) - Create specialized agents

## Limitations

- Max 4 concurrent subagents
- Each subagent has isolated context (can't share state)
- Subagents can't communicate with each other
- Tool approvals required unless dangerously_trust_all_tools enabled
- Subagents spawned together can't be dependent on each other's results
- No real-time streaming of subagent output to main agent

## Technical Details

**Aliases**: `use_subagent`, `subagent`

**Execution**: Subagents run synchronously (blocking) - main agent waits for all to complete.

**Context Isolation**: Each subagent has own conversation context, separate from main agent.

**Agent Selection**: If agent_name not specified, uses default agent. Local agents (`.kiro/agents/`) take precedence over global (`~/.kiro/agents/`).

**Summary Tool**: Subagents use built-in summary tool to report findings back to main agent.

**Permissions**: Trusted by default. Requires `chat.enableSubagent` setting enabled.

**Interactive Mode**: Set `is_interactive: true` to allow subagent to prompt user for input.
