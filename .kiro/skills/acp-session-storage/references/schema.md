# Schema Reference

## LogEntry (v1)

```typescript
// Tagged union: {"version":"v1","kind":"<Kind>","data":{...}}

// Prompt
{
  "version": "v1",
  "kind": "Prompt",
  "data": {
    "message_id": string,
    "content": ContentBlock[]
  }
}

// AssistantMessage
{
  "version": "v1",
  "kind": "AssistantMessage",
  "data": {
    "message_id": string,
    "content": ContentBlock[]
  }
}

// ToolResults
{
  "version": "v1",
  "kind": "ToolResults",
  "data": {
    "message_id": string,
    "content": ContentBlock[],
    "results": { [tool_use_id: string]: ToolResult }
  }
}

// Compaction
{
  "version": "v1",
  "kind": "Compaction",
  "data": {
    "summary": string,
    "strategy": CompactStrategy,
    "messages_snapshot": Message[]
  }
}

// ResetTo
{
  "version": "v1",
  "kind": "ResetTo",
  "data": {
    "target_index": number
  }
}

// CancelledPrompt, Clear - no data field
```

## ContentBlock

```typescript
// Tagged union: {"kind":"<kind>","data":...}
type ContentBlock =
  | { kind: "text", data: string }
  | { kind: "toolUse", data: ToolUseBlock }
  | { kind: "toolResult", data: ToolResultBlock }
  | { kind: "image", data: ImageBlock }
```

## ToolUseBlock

```typescript
{
  "toolUseId": string,
  "name": string,      // Tool name (e.g., "read", "write", "execute_bash")
  "input": object      // Tool-specific parameters
}
```

## ToolResultBlock

```typescript
{
  "toolUseId": string,
  "content": ToolResultContentBlock[],
  "status": "success" | "error"
}

type ToolResultContentBlock =
  | { kind: "text", data: string }
  | { kind: "json", data: object }
  | { kind: "image", data: ImageBlock }
```

## ToolResult (in ToolResults event)

```typescript
{
  "tool": Tool | null,  // Parsed tool, null if parsing failed
  "result": {
    "Success": { "items": [...] } |
    "Error": { ... } |
    "Cancelled"
  }
}
```

## CompactStrategy

```typescript
{
  "message_pairs_to_exclude": number,
  "context_window_percent_to_exclude": number,
  "truncate_large_messages": boolean,
  "max_message_length": number
}
```

## Message (in Compaction snapshot)

```typescript
{
  "id": string | null,
  "role": "user" | "assistant",
  "content": ContentBlock[],
  "timestamp": number | null  // Unix timestamp in seconds
}
```

## Source Code References

- Event log types: `crates/agent/src/agent/event_log.rs`
- Content block types: `crates/agent/src/agent/agent_loop/types.rs`
- Session persistence: `crates/chat-cli-v2/src/agent/session/mod.rs`
- Compaction logic: `crates/agent/src/agent/compact/mod.rs`
- Tool result types: `crates/agent/src/agent/protocol.rs`
