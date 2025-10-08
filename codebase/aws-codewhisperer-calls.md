# AWS CodeWhisperer Client Usage in chat-cli

## Overview

Analysis of how AWS CodeWhisperer clients are used in the chat-cli crate, focusing on request setup, data packing, and response preservation between calls.

## Client Selection and Initialization

### Client Types Used
From `/crates/chat-cli/src/api_client/mod.rs:100-200`:

1. **CodewhispererClient** (non-streaming) - Line 14
2. **CodewhispererStreamingClient** - Line 23  
3. **QDeveloperStreamingClient** - Line 24

### Client Selection Logic
From `/crates/chat-cli/src/api_client/mod.rs:140-180`:

- **Bearer Token Mode** (default): Uses `CodewhispererStreamingClient` for streaming operations
- **SigV4 Mode**: Uses `QDeveloperStreamingClient` when `AMAZON_Q_SIGV4` environment variable is set
- Selection happens at initialization based on environment variable check (line 140)

### Authentication Setup
From `/crates/chat-cli/src/api_client/mod.rs:110-130`:

- **Bearer Token**: Uses dummy credentials (`"xxx"`) with `BearerResolver` (line 110)
- **SigV4**: Uses `CredentialsChain` for AWS credentials (line 145)
- Both configured with same endpoint URL from `endpoint.url()` (lines 125, 175)

## Request Construction and Data Packing

### Primary Request Method
From `/crates/chat-cli/src/api_client/mod.rs:378-420`:

The main entry point is `ApiClient::send_message()` which takes a `ConversationState` and returns `SendMessageOutput`.

### ConversationState Structure
From `/crates/chat-cli/src/api_client/model.rs:96-100`:

```rust
pub struct ConversationState {
    pub conversation_id: Option<String>,
    pub user_input_message: UserInputMessage,
    pub history: Option<Vec<ChatMessage>>,
}
```

### UserInputMessage Data Packing
From `/crates/chat-cli/src/api_client/model.rs:857-863`:

```rust
pub struct UserInputMessage {
    pub content: String,
    pub user_input_message_context: Option<UserInputMessageContext>,
    pub user_intent: Option<UserIntent>,
    pub images: Option<Vec<ImageBlock>>,
    pub model_id: Option<String>,
}
```

### UserInputMessageContext Details
From `/crates/chat-cli/src/api_client/model.rs:894-900`:

```rust
pub struct UserInputMessageContext {
    pub env_state: Option<EnvState>,
    pub git_state: Option<GitState>,
    pub tool_results: Option<Vec<ToolResult>>,
    pub tools: Option<Vec<Tool>>,
}
```

### Request Building Process
From `/crates/chat-cli/src/cli/chat/conversation.rs:958-971`:

1. **Conversation State Construction**: `as_sendable_conversation_state()` method (line 492)
2. **History Flattening**: `flatten_history()` combines context messages and history (line 959)
3. **User Message Conversion**: `into_user_input_message()` packs tools and context (line 963)
4. **Final Assembly**: Creates `FigConversationState` with conversation_id, user_input_message, and history (line 966)

### Context Data Inclusion
From `/crates/chat-cli/src/cli/chat/message.rs:200-225`:

The `into_user_input_message()` method packs:
- **Tool Results**: From previous tool executions (line 210-215)
- **Available Tools**: Current tool specifications (line 216-220)
- **Environment State**: OS, git state, etc. (line 209)
- **Images**: Attached image blocks
- **Model ID**: Target model identifier

## API Call Execution

### Streaming Client Calls
From `/crates/chat-cli/src/api_client/mod.rs:390-420`:

**CodewhispererStreamingClient**:
- Uses `generate_assistant_response()` operation (line 400)
- Includes `profile_arn` if available (line 403)
- Returns `SendMessageOutput::Codewhisperer(response)` (line 406)

**QDeveloperStreamingClient**:
- Uses `send_message()` operation (line 499)
- Includes `source` set to "CLI" (line 501)
- Returns `SendMessageOutput::QDeveloper(response)` (line 505)

### Request Metadata Tracking
From `/crates/chat-cli/src/cli/chat/parser.rs:200-240`:

Each request generates:
- **Message ID**: UUID for tracking (line 200)
- **Request Start Time**: For latency measurement (line 210)
- **User Prompt Length**: For telemetry (line 202)
- **Model ID**: Target model (line 203)
- **Message Meta Tags**: Additional metadata (line 204)

## Response Processing and Preservation

### Response Stream Handling
From `/crates/chat-cli/src/cli/chat/parser.rs:160-180`:

`SendMessageStream` structure:
- **Request ID**: Preserved from response headers (line 161)
- **Event Receiver**: Streaming response channel (line 162)
- **Cancel Token**: For graceful cancellation (line 164)

### Response Event Types
From `/crates/chat-cli/src/api_client/model.rs:545-550`:

```rust
pub enum ChatResponseStream {
    AssistantResponseEvent { content: String },
    // Other streaming events...
}
```

### AssistantResponseMessage Preservation
From `/crates/chat-cli/src/api_client/model.rs:511-518`:

```rust
pub struct AssistantResponseMessage {
    pub message_id: Option<String>,    // Preserved for conversation continuity
    pub content: String,               // Response content
    pub tool_uses: Option<Vec<ToolUse>>, // Tool calls to execute
}
```

### Conversation History Management
From `/crates/chat-cli/src/cli/chat/conversation.rs:95-120`:

**Preserved Between Calls**:
1. **Conversation ID**: UUID maintained throughout session (line 97)
2. **Message History**: `VecDeque<HistoryEntry>` with user/assistant pairs (line 99)
3. **Tool Results**: Previous tool execution results included in next request
4. **Context Files**: Managed by `ContextManager` (line 105)
5. **Model Information**: Current model metadata (line 110)

**History Entry Structure**:
- **User Message**: Original user input with tool results
- **Assistant Message**: Response with message_id and tool_uses
- **Metadata**: Timestamps, character counts, etc.

### State Persistence
From `/crates/chat-cli/src/cli/chat/conversation.rs:400-410`:

- **Database Storage**: Conversation saved by working directory path (line 405)
- **Message ID Tracking**: Last assistant message_id preserved for utterance tracking (line 417)
- **Tool State**: Tool manager maintains tool availability and updates (line 530)

## Key Data Flow Summary

1. **Request Setup**: 
   - Conversation ID generated once per session
   - User input combined with context, tools, and history
   - Previous assistant responses and tool results included

2. **API Call**:
   - Client selected based on authentication mode
   - Request sent with full conversation state
   - Streaming response initiated

3. **Response Processing**:
   - Message ID extracted and preserved
   - Content streamed and accumulated
   - Tool uses extracted for execution

4. **State Preservation**:
   - Assistant response added to history
   - Tool results prepared for next request
   - Conversation state persisted to database

This architecture ensures conversation continuity by preserving message IDs, maintaining complete history, and including tool results in subsequent requests.
