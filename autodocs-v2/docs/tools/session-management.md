---
doc_meta:
  title: session-management
  description: Agent-to-agent orchestration tool for spawning sessions, managing groups, and controlling lifecycle
  category: tool
  keywords: [session, orchestration, spawn, group, agent, interrupt, revive, persistent]
  related: [subagent, summary]
  validated: 2026-07-22
  commit: 9256b96f2
  status: validated
  testable_headless: true
---

## Overview

The session management tool provides agent-to-agent orchestration capabilities within the ACP (Agent Communication Protocol) layer. It allows agents to spawn persistent sessions, manage session groups, control session lifecycle, and coordinate multi-agent work.

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

This tool is used internally by the agent system for multi-session coordination. It is not included in the default tool set for regular agents.

Sessions are long-lived agents, unlike subagents which are ephemeral. Worker results are consolidated and returned when the group completes.

## Usage

The tool uses a `command` field to select the operation. Available commands:

### spawn_session

Spawn a new persistent session with an agent.

- `agent_name` (required) — Agent config name to use
- `task` (required) — Initial task/prompt for the session
- `model` (optional) — Model override for the session (normal agent/default model resolution when omitted)
- `name` (optional) — Friendly name (auto-assigned if omitted)
- `role` (optional) — Role description
- `group` (optional) — Group to add the session to
- `persistent` (optional) — If true, session stays alive after task; if false (default), terminates after task

### list_sessions

List sessions with optional filtering.

- `filter` (optional) — One of: `active`, `idle`, `busy`, `terminated`, `all`

### get_session_status

Get detailed status of a specific session.

- `target` (required) — Session ID or name
- `verbose` (optional) — Show full details including live activity (default: false)

### interrupt

Interrupt a session and redirect it with a new message.

- `target` (required) — Target session ID or name
- `message` (required) — New direction/message

### inject_context

Silently inject context into a session without triggering a turn.

- `target` (required) — Target session ID or name
- `context` (required) — Context content to inject

### manage_group

Manage session groups for coordinated work.

- `action` (required) — One of: `create`, `add`, `remove`, `list`
- `group` (optional) — Group name
- `target` (optional) — Session ID or name (for add/remove)
- `role` (optional) — Role within group (for add)

### revive_session

Revive a terminated session with a new task, keeping the same name and group.

- `target` (required) — Session name to revive
- `task` (required) — New task/prompt for the revived session

## Examples

### Spawning a session

```json
{
  "command": "spawn_session",
  "agent_name": "code-reviewer",
  "task": "Review the changes in src/auth/ for security issues",
  "model": "claude-sonnet-4.6",
  "name": "auth-reviewer",
  "group": "review-team"
}
```

### Listing active sessions

```json
{
  "command": "list_sessions",
  "filter": "active"
}
```

### Managing a group

```json
{
  "command": "manage_group",
  "action": "create",
  "group": "review-team"
}
```

```json
{
  "command": "manage_group",
  "action": "add",
  "group": "review-team",
  "target": "auth-reviewer",
  "role": "security-specialist"
}
```

### Interrupting a session

```json
{
  "command": "interrupt",
  "target": "auth-reviewer",
  "message": "Stop current task, new priority: check for SQL injection in queries"
}
```

### Reviving a terminated session

```json
{
  "command": "revive_session",
  "target": "auth-reviewer",
  "task": "Review the new changes pushed to the auth module"
}
```

### Injecting context

```json
{
  "command": "inject_context",
  "target": "auth-reviewer",
  "context": "The team has decided to use JWT tokens instead of session cookies"
}
```

## Troubleshooting

### Session not found

If a target session cannot be found:
- Verify the session name or ID is correct using `list_sessions`
- The session may have already terminated — use `filter: "terminated"` to check
- Use `revive_session` to restart a terminated session

### Results not appearing

- Worker results are consolidated and returned when the group completes
- Use `get_session_status` to check on a specific worker's progress

### Tool not available

The session management tool is excluded from the default agent tool set. It is used internally by the orchestration layer.

## Related

- [subagent](subagent.md) — Higher-level tool for delegating tasks to subagents
- [summary](summary.md) — Tool subagents use to report results back
