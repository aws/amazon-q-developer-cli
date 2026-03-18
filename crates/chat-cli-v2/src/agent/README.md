# ACP Agent Implementation

This module implements the ACP server — the Rust side of the `TUI ↔ Agent` communication.

## Actors and Channels
This module uses the actor pattern. An actor is an independent unit that runs its own loop, waiting for messages and processing them one at a time. Actors communicate through channels — one-way message queues where one side sends and the other side receives. Actors never call each other directly; they drop a message into the other actor's channel and move on. The entry point is `AgentToClient` from the `sacp` SDK. The SDK runs the loop internally — it reads ACP messages from stdin, dispatches each to our registered handlers, and writes request responses to stdout. The handlers don't do the real work — they route each request into the right actor's channel.

There are three actors:
1. **SessionManager** [created for ACP Process] Coordinates session lifecycle, owns agent configs, and routes requests to the right session.
1. **AcpSession** [created for each session, including subagent sessions]: — bridges the ACP protocol to a single `Agent`. Translates agent events into ACP notifications, dispatches slash commands, and persists session state via `SessionDb`. Also writes streaming notifications (text chunks, tool calls) to stdout via `connection_cx`.
1. **Agent** — (from the `agent` crate) runs the LLM conversation, executes tools, evaluates permissions. See agent crate README for its internals.


```
        stdin                        stdout
          │                            ▲
          ▼                            │ (request responses)
┌─────────────────┐                    │
│  SACP Server    │────────────────────┘
│  (AgentToClient │
│   from sacp SDK)│      ┌──────────────────────┐
│                 ├─────►│   SessionManager     │
│ • Reads stdin   │ Ch 1 │                      │
│ • Dispatches to │      │ • Owns all sessions  │
│   handlers      │      │ • Owns agent configs │
│ • Writes        │      └──────────────────────┘
│   responses to  │
│   stdout        │      ┌──────────────────────┐        ┌──────────────────┐
│                 ├─────►│   AcpSession         │ Ch 3   │   Agent          │
└─────────────────┘ Ch 2 │   (per session)      ├───────►│   (per session)  │
                         │                      │◄───────┤                  │
                         │ • Owns AgentHandle   │ Ch 4   │ • LLM calls      │
                         │ • Owns SessionDb     │        │ • Tool execution │
                         │ • Slash commands     │        │ • Permissions    │
                         └──────────┬───────────┘        └──────────────────┘
                                    │
                                    │ stdout (streaming notifications)
                                    ▼
```

Four channels connect the actors:

- **Channel 1** (AgentToClient → SessionManager): Session lifecycle — create, load, switch mode.
- **Channel 2** (AgentToClient → AcpSession): Per-session requests — prompts, cancel, slash commands.
- **Channel 3** (AcpSession → Agent): Requests to the agent — send prompt, cancel, swap agent, approval results.
- **Channel 4** (Agent → AcpSession): Events from the agent — streaming text, tool calls, approval requests, end-of-turn.

### Where things are defined

| Concept | Type | Location |
|---------|------|----------|
| AgentToClient (SDK loop) | `sacp::AgentToClient` | `sacp` crate (external) |
| Handlers (registered on AgentToClient) | closures | `acp_agent.rs` → `execute()` |
| SessionManager actor | `SessionManager` | `session_manager.rs` |
| AcpSession actor | `AcpSession` | `acp_agent.rs` |
| Agent actor | `Agent` | `crates/agent/src/agent/mod.rs` |
| SessionManager channel (ch 1) | `SessionManagerHandle` wraps `mpsc::Sender` | `session_manager.rs` |
| AcpSession channel (ch 2) | `AcpSessionHandle` wraps `mpsc::Sender` | `acp_agent.rs` |
| Agent channels (ch 3 + 4) | `AgentHandle` wraps `RequestSender` + `broadcast::Receiver` | `crates/agent/src/agent/mod.rs` |


## How It Starts

When the TUI spawns `kiro-cli chat acp`, the `execute()` function in `acp_agent.rs` sets everything up: it creates the `SessionManager`, registers a handler for each ACP method, and starts `AgentToClient` (the SDK loop). Each handler is a small function that routes the request to the right actor's channel. 

```
execute()
    │
    ├─ Build SessionManager and start AgentToClient (SDK loop)
    │
    │  AgentToClient reads ACP messages from stdin and routes
    │  each one to the actor responsible for handling it:
    │
    │  Standard ACP methods:
    │    initialize                 ──► SessionManager  (channel 1)
    │    session/new                ──► SessionManager  (channel 1)
    │    session/load               ──► SessionManager  (channel 1)
    │    session/set_mode           ──► SessionManager  (channel 1)
    │    session/prompt             ──► AcpSession      (channel 2)
    │
    │  Custom extensions (kiro.dev/):
    │    kiro.dev/commands/execute  ──► AcpSession      (channel 2)
    │    kiro.dev/commands/options  ──► AcpSession      (channel 2)
    │
    │  AcpSession also sends these notifications back via AgentToClient:
    │    kiro.dev/commands/available       Advertise slash commands, prompts, tools, MCP servers
    │    kiro.dev/metadata                 Context usage percentage
    │    kiro.dev/compaction/status        Compaction started/completed/failed
    │    kiro.dev/mcp/server_init_failure  MCP server failed to initialize
    │    kiro.dev/error/rate_limit         Rate limit hit
    │    kiro.dev/agent/switched           Agent persona changed
    │
    └─ Responses written back to stdout via shared connection handle
```
