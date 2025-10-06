# ModelProvider System

## Overview

The **ModelProvider** trait abstracts LLM communication, enabling different LLM backends (AWS Bedrock, OpenAI, local models) to work with the same agent architecture. It provides streaming response support and tool request handling.

## ModelProvider Trait

**File**: `crates/chat-cli/src/agent_env/model_providers/model_provider.rs`

### Definition

**Location**: `model_provider.rs` lines 24-32

```rust
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    async fn request(
        &self,
        request: ModelRequest,
        when_receiving_begin: impl Fn() + Send,
        when_received: impl Fn(ModelResponseChunk) + Send,
        cancellation_token: CancellationToken,
    ) -> Result<ModelResponse, eyre::Error>;
}
```

### Method

**request()**: Sends request to LLM and streams response

**Parameters**:
- **request**: The prompt and context
- **when_receiving_begin**: Callback when first chunk arrives
- **when_received**: Callback for each response chunk
- **cancellation_token**: For cancelling the request

**Returns**: Complete response with text and tool requests

## Data Types

### ModelRequest

**Location**: `model_provider.rs` lines 3-6

```rust
#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub prompt: String,
}
```

Currently simple prompt string. Future: conversation history, system prompt, parameters.

### ModelResponseChunk

**Location**: `model_provider.rs` lines 8-12

```rust
#[derive(Debug, Clone)]
pub enum ModelResponseChunk {
    AssistantMessage(String),
    ToolUseRequest { tool_name: String, parameters: String },
}
```

Streaming chunks can be:
- **AssistantMessage**: Text content from LLM
- **ToolUseRequest**: Request to execute a tool

### ModelResponse

**Location**: `model_provider.rs` lines 14-18

```rust
#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub content: String,
    pub tool_requests: Vec<ToolRequest>,
}
```

Complete response after streaming:
- **content**: Full text response
- **tool_requests**: All tool requests from response

### ToolRequest

**Location**: `model_provider.rs` lines 20-24

```rust
#[derive(Debug, Clone)]
pub struct ToolRequest {
    pub tool_name: String,
    pub parameters: String,
}
```

Tool execution request:
- **tool_name**: Name of tool to execute
- **parameters**: JSON parameters for tool

## Bedrock Implementation

**File**: `crates/chat-cli/src/agent_env/model_providers/bedrock_converse_stream.rs`

### Structure

**Location**: `bedrock_converse_stream.rs` lines 13-16

```rust
#[derive(Clone)]
pub struct BedrockConverseStreamModelProvider {
    pub client: Client,
}
```

Uses AWS SDK Bedrock client.

### Constructor

**Location**: `bedrock_converse_stream.rs` lines 18-24

```rust
pub fn new(client: Client) -> Self {
    Self { client }
}
```

### Request Implementation

**Location**: `bedrock_converse_stream.rs` lines 29-125

```rust
async fn request(
    &self,
    request: ModelRequest,
    when_receiving_begin: impl Fn() + Send,
    when_received: impl Fn(ModelResponseChunk) + Send,
    cancellation_token: CancellationToken,
) -> Result<ModelResponse, eyre::Error>
```

Implementation flow:

1. **Build Request** (lines 35-44)
   ```rust
   let request = self.client
       .converse_stream()
       .model_id("us.anthropic.claude-3-5-sonnet-20241022-v2:0")
       .messages(
           Message::builder()
               .role(ConversationRole::User)
               .content(ContentBlock::Text(request.prompt))
               .build()
               .map_err(|e| eyre::eyre!("Failed to build message: {}", e))?,
       );
   ```

2. **Send Request** (lines 46-47)
   ```rust
   let mut stream = request.send().await?.stream;
   ```

3. **Process Stream** (lines 49-107)
   - Calls `when_receiving_begin()` on first chunk
   - Iterates through stream events
   - Handles ContentBlockDelta (text chunks)
   - Accumulates full response
   - Checks cancellation token
   - Handles errors

4. **Return Response** (lines 109-112)
   ```rust
   Ok(ModelResponse {
       content: full_response,
       tool_requests: vec![],
   })
   ```

### Error Handling

**Location**: `bedrock_converse_stream.rs` lines 95-106

Handles various error cases:
- Stream errors
- Cancellation
- Unexpected event types
- Missing content

## Usage Pattern

```rust
// Create provider
let client = aws_sdk_bedrockruntime::Client::new(&config);
let provider = BedrockConverseStreamModelProvider::new(client);

// Make request
let response = provider.request(
    ModelRequest {
        prompt: "Hello, world!".to_string(),
    },
    || {
        println!("Receiving started");
    },
    |chunk| {
        match chunk {
            ModelResponseChunk::AssistantMessage(text) => {
                print!("{}", text);
            }
            ModelResponseChunk::ToolUseRequest { tool_name, parameters } => {
                println!("Tool: {} ({})", tool_name, parameters);
            }
        }
    },
    cancellation_token,
).await?;

println!("Full response: {}", response.content);
```

## Design Notes

### Streaming First

Provider streams chunks as they arrive:
- Enables real-time UI updates
- Better user experience (no waiting for full response)
- Lower perceived latency

### Callback-Based

Uses callbacks instead of channels:
- Simpler API
- Less overhead
- Direct notification path

### Cancellation Support

Checks cancellation token during streaming:
- Clean shutdown mid-stream
- No wasted LLM tokens
- Responsive to user cancellation

### Provider Abstraction

Trait enables multiple backends:
- AWS Bedrock (current)
- OpenAI API (future)
- Local models (future)
- Mock provider (testing)

## Future Enhancements

### Request Features
- Conversation history
- System prompts
- Temperature/top_p parameters
- Max tokens limit
- Stop sequences

### Response Features
- Token usage metrics
- Finish reason
- Model metadata
- Latency tracking

### Provider Implementations
- OpenAI provider
- Anthropic direct API
- Local model provider (Ollama, llama.cpp)
- Mock provider for testing

### Advanced Features
- Request retries
- Rate limiting
- Connection pooling
- Caching
- Cost tracking
