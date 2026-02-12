---
doc_meta:
  validated: 2026-02-03
  commit: 1e26fbcf
  status: validated
  testable_headless: false
  category: command
  title: kiro-cli acp
  description: Start the Agent Client Protocol (ACP) agent for programmatic client integration
  keywords: [acp, agent-client-protocol, protocol, integration, api, headless]
  related: [chat, mcp]
---

# kiro-cli acp

Start the Agent Client Protocol (ACP) agent for programmatic client integration.

## Overview

The `kiro-cli acp` command starts Kiro as an ACP-compliant agent that communicates over stdin/stdout using JSON-RPC. This enables programmatic integration with ACP clients like the Kiro TUI or custom tooling.

ACP (Agent Client Protocol) is an open protocol that standardizes how clients communicate with AI agents. See [agentclientprotocol.com](https://agentclientprotocol.com/get-started/introduction) for the full specification.

This command is used by the Kiro 2.0 TUI but can be invoked directly by any integration that supports ACP.

## Usage

```bash
kiro-cli acp
```

The agent reads JSON-RPC messages from stdin and writes responses to stdout. Communication follows the ACP specification.

## Supported ACP Methods

### Core Protocol

| Method | Description |
|--------|-------------|
| `initialize` | Initialize the connection and exchange capabilities |
| `session/new` | Create a new chat session |
| `session/load` | Load an existing session by ID |
| `session/prompt` | Send a prompt to the agent |
| `session/cancel` | Cancel the current operation |
| `session/set_mode` | Switch agent mode (e.g., different agent configs) |
| `session/set_model` | Change the model for the session |

### Agent Capabilities

The Kiro ACP agent advertises these capabilities during initialization:

- `loadSession: true` - Supports loading existing sessions
- `promptCapabilities.image: true` - Supports image content in prompts

### Session Updates

The agent sends these session update types via `session/notification`:

| Update Type | Description |
|-------------|-------------|
| `AgentMessageChunk` | Streaming text/content from the agent |
| `ToolCall` | Tool invocation with name, parameters, status |
| `ToolCallUpdate` | Progress updates for running tools |
| `TurnEnd` | Signals the agent turn has completed |

### Kiro Extensions

Custom extension methods (prefixed with `_kiro.dev/` per ACP spec). These are currently used by the new Kiro TUI under development:

| Method | Type | Description |
|--------|------|-------------|
| `_kiro.dev/commands/execute` | Request | Execute a slash command |
| `_kiro.dev/commands/options` | Request | Get autocomplete options for a command |
| `_kiro.dev/commands/available` | Notification | Lists available commands after session creation |
| `_kiro.dev/mcp/oauth_request` | Notification | OAuth URL for MCP server authentication |
| `_kiro.dev/mcp/server_initialized` | Notification | MCP server finished initializing |
| `_kiro.dev/compaction/status` | Notification | Context compaction progress |
| `_kiro.dev/clear/status` | Notification | Session clear status |
| `_session/terminate` | Notification | Terminate a subagent session |

## Session Storage

ACP sessions are persisted to disk at:

```
~/.kiro/sessions/cli/
```

Each session creates two files:
- `<session-id>.json` - Session metadata and state
- `<session-id>.jsonl` - Event log (conversation history)

## Logging

ACP agent logs are written to the standard Kiro log location:

- macOS: `$TMPDIR/kiro-log/kiro-chat.log`
- Linux: `/tmp/kiro-log/logs/kiro-chat.log`

Control log verbosity with environment variables:

```bash
KIRO_LOG_LEVEL=debug kiro-cli acp
KIRO_CHAT_LOG_FILE=/path/to/custom.log kiro-cli acp
```

## Examples

### Example 1: Start ACP Agent

```bash
kiro-cli acp
```

The agent waits for JSON-RPC messages on stdin.

## Related

- [kiro-cli chat](chat.md) - Interactive chat (uses ACP internally)
- [kiro-cli mcp](mcp.md) - MCP server management (MCP servers can be passed to ACP sessions)

## Technical Details

- Protocol: JSON-RPC 2.0 over stdio
- Transport: stdin (client→agent), stdout (agent→client)
- Specification: ACP v2025-01-01
- Implementation: `sacp` Rust crate
