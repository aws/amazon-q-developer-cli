# Agent-Client-Protocol (ACP)

This is an implementation of ACP agent interface using Amazon Q CLI agent

## Features

### Supported
- New Session setup ("session/new")
- Basic chat ("session/prompt" & "session/update")
- Using built-in tool like fs_read and fs_write ("tool_call")
- Request tool call permission ("session/request_permission")

### Not Supported
- Tool call update ("tool_call_update" as part of "session/update")
- Auth
- Slash commands
- MCP
- Cancel
- Session reload

## Usage

### Run ACP Agent (Standalone)
```bash
cargo run -p agent -- acp

# Test it with:
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1}}
```

### Run ACP Client (for testing)
```bash
# Build agent first
cargo build -p agent

# Run interactive test client
cargo run -p agent -- acp-client ./target/debug/agent
```

The test client automatically launch the ACP Agent. It provides a REPL interface for sending prompts to the agent and automatically approves tool permissions.