---
doc_meta:
  title: session-management
  description: Agent-to-agent orchestration tool for spawning sessions, messaging, and group management
  category: tool
  keywords: [session, orchestration, spawn, message, group, agent, inbox, escalation]
  related: [subagent, summary]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
---

## Overview

The session management tool provides agent-to-agent orchestration capabilities within the ACP (Agent Communication Protocol) layer. It allows agents to spawn persistent sessions, send messages between sessions, read their inbox, manage session groups, and control session lifecycle.

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

This tool is used internally by the agent system for multi-session coordination. It is not included in the default tool set for regular agents.

## Usage

The tool uses a `command` field to select the operation. Available commands:

### spawn_session

Spawn a new persistent session with an agent.

- `agent_name` (required) — Agent config name to use
- `task` (required) — Initial task/prompt for the session
- `name` (optional) — Friendly name (auto-assigned if omitted)
- `role` (optional) — Role description
- `group` (optional) — Group to add the session to
- `persistent` (optional) — If true, session stays alive after task; if false, terminates after task

### send_message

Send a message to another session's inbox.

- `target` (optional) — Target session ID or name. Omit for escalation auto-route to parent
- `message` (required) — Message content
- `priority` (optional) — `normal` (default) or `escalation`

### read_messages

Read messages from this session's inbox.

- `limit` (optional) — Max messages to return (default: 5)

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

- `action` (required) — One of: `create`, `add`, `remove`, `list`, `broadcast`
- `group` (optional) — Group name
- `target` (optional) — Session ID or name (for add/remove)
- `role` (optional) — Role within group (for add)
- `message` (optional) — Message content (for broadcast)

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
  "name": "auth-reviewer",
  "group": "review-team"
}
```

### Sending a message with escalation

```json
{
  "command": "send_message",
  "target": "auth-reviewer",
  "message": "Found a critical issue in token validation, please prioritize",
  "priority": "escalation"
}
```

### Listing active sessions

```json
{
  "command": "list_sessions",
  "filter": "active"
}
```

### Broadcasting to a group

```json
{
  "command": "manage_group",
  "action": "broadcast",
  "group": "review-team",
  "message": "All reviews complete, please submit your summaries"
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

## Troubleshooting

### Session not found

If a target session cannot be found:
- Verify the session name or ID is correct using `list_sessions`
- The session may have already terminated — use `filter: "terminated"` to check
- Use `revive_session` to restart a terminated session

### Messages not received

- Messages are delivered to the target session's inbox asynchronously
- Use `read_messages` to check the inbox
- Escalation priority messages are auto-routed to the parent session if no target is specified

### Tool not available

The session management tool is excluded from the default agent tool set. It is used internally by the orchestration layer.

## Related

- [subagent](subagent.md) — Higher-level tool for delegating tasks to subagents
- [summary](summary.md) — Tool subagents use to report results back
