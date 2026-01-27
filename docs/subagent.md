# Subagent
**Tool name**: `use_subagent`
**Description:** Enables delegating complex tasks to specialized subagents that run in parallel with isolated context

**Features:**
- Spawn up to 4 subagents simultaneously for parallel task execution
- Each subagent operates with its own isolated context to prevent main conversation bloat
- Control which agents are available and trusted via toolsSettings
- Real-time visual indicator showing status of all running subagents
- Support for different agent configurations per subagent
- Interactive controls for monitoring and managing subagents
- Automatic execution summary with tool usage and duration metrics

**Configuration:**
Control subagent access in your agent configuration:
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

- **availableAgents**: Which agents can be used as subagents (supports glob patterns). If not set, all agents are available.
- **trustedAgents**: Which agents are auto-approved without confirmation (supports glob patterns). Alias: `allowedAgents` for backwards compatibility.

**How it works:**
When enabled, the main agent can delegate tasks to subagents using the `use_subagent` tool. Each subagent:
1. Receives a specific query/task and optional context
2. Must be in the availableAgents list (if configured)
3. Runs independently with its own agent configuration
4. Uses the `summary` tool to report findings back to the main agent
5. Operates in isolation to keep the main conversation focused

**Visual Indicator:**
The subagent indicator displays:
- Real-time status for each subagent (starting up, thinking, calling tools, summarizing)
- Animated spinner showing active subagents
- Current activity and progress messages
- Tool approval requests (press 'y' to approve, 'n' to deny)
- MCP server loading status and OAuth requirements
- Execution summary upon completion (tool uses, duration)

**Controls:**
```
j/↓         Navigate down through subagents
k/↑         Navigate up through subagents
y           Approve tool use for selected subagent
n           Deny tool use for selected subagent
Enter       Copy OAuth URL to clipboard (when applicable)
Ctrl+C      Interrupt all subagents
Esc         Deselect current subagent
```

**Use cases:**
- Breaking down complex multi-step tasks into parallel subtasks
- Preventing context window bloat in long conversations
- Running independent research or analysis tasks simultaneously
- Delegating specialized tasks to different agent configurations
- Maintaining focus in the main conversation while handling auxiliary tasks

**Example workflow:**
```
User: "Research the top 3 JavaScript frameworks and compare their performance"

Main agent uses subagent tool to spawn 3 subagents:
- Subagent 1: Research React performance metrics
- Subagent 2: Research Vue.js performance metrics
- Subagent 3: Research Angular performance metrics

Each subagent:
- Conducts independent research
- Gathers relevant data
- Calls summary tool with findings

Main agent receives all summaries and synthesizes comparison
```

