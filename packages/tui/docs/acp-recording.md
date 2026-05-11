# ACP wire recording

The TUI can record every JSON-RPC message exchanged with its ACP agent
(KAS or V2) to a JSONL file. Useful for debugging protocol issues,
reproducing production bugs, and capturing traces to hand-author test
scenarios from.

## Enabling

Set `KIRO_ACP_RECORD_PATH` to a file path before launching the TUI:

```bash
KIRO_ACP_RECORD_PATH=/tmp/kiro-acp-trace.jsonl kiro-cli
```

- The recorder appends to the file; it does not truncate.
- Missing parent directories are not created; the recorder disables
  itself and logs an error if the file cannot be opened.
- Unset (or empty) = no recording and zero overhead.

## Format

Each line is a JSON object with three fields:

```
{"ts":1715187000000,"dir":"in","msg":{"jsonrpc":"2.0","id":1,"result":{...}}}
{"ts":1715187000001,"dir":"out","msg":{"jsonrpc":"2.0","id":2,"method":"session/new","params":{...}}}
{"ts":1715187000050,"dir":"in","msg":{"jsonrpc":"2.0","method":"session/update","params":{...}}}
```

- `ts`: wall-clock milliseconds since epoch (`Date.now()`)
- `dir`: `in` = message from agent to TUI, `out` = message from TUI to agent
- `msg`: the full JSON-RPC 2.0 message as it appeared on the wire, verbatim

## Sensitive data

> **Traces contain full user input, tool inputs and outputs, and file
> contents.** Treat them as confidential.

Specifically, the following typically land in traces unredacted:

- Prompts the user types
- File paths and file contents the agent reads or writes
- Tool invocation parameters and tool results
- System prompts, session history, MCP server config
- Working directory paths, environment context

If you are sharing a trace for debugging, review it first and redact any
sensitive values by hand.

## Use cases

### Reproducing a bug

1. Enable the recorder locally and reproduce the bug.
2. Note the timestamp when the bug happened.
3. Attach the trace to the bug report; filter to messages near that
   timestamp when analyzing.

### Writing a regression test

1. Capture a trace exhibiting the bug.
2. Inspect the trace to understand the agent-to-client message sequence
   that triggered it.
3. Hand-author a mock scenario (using `acp_integ_tests/` infrastructure)
   that reproduces the same sequence. Do not attempt to replay the trace
   verbatim - the TUI's outbound traffic depends on user input,
   filesystem state, and timing that the trace does not capture.

### Inspecting protocol behavior

`jq` and similar tools work well on JSONL:

```bash
# Count messages by method
jq -r 'select(.msg.method) | .msg.method' /tmp/trace.jsonl | sort | uniq -c

# Show all requests sent by the TUI
jq 'select(.dir == "out" and .msg.method)' /tmp/trace.jsonl

# Find everything in a 100ms window around a known timestamp
jq 'select(.ts > 1715187000000 and .ts < 1715187000100)' /tmp/trace.jsonl
```

## Performance

When `KIRO_ACP_RECORD_PATH` is unset, the recorder is never instantiated
and the ACP stream runs with no wrapper.

When enabled, every message passes through a `TransformStream` and is
serialized with `JSON.stringify` before async file write. Overhead is
small but measurable under heavy traffic; prefer to enable only when
actively debugging, not as a default.

## Limitations

- No automatic redaction. Future work.
- No log rotation or size cap. Long sessions can produce large trace
  files; rotate or clean up manually.
- Best-effort flush on signals (`SIGINT`, `SIGTERM`, `beforeExit`). The
  last few messages may be lost if the process is killed abruptly.
- Not intended as an audit log or telemetry channel; use the proper
  observability pipelines for those.
