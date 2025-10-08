# LLM API Initialization in chat-cli

## Overview
The LLM API in chat-cli is initialized through the `ApiClient` struct in `src/api_client/mod.rs`. The client is created during OS initialization and provides access to both CodeWhisperer and Q Developer streaming clients.

## Key Components

### 1. ApiClient Structure
```rust
#[derive(Clone, Debug)]
pub struct ApiClient {
    client: CodewhispererClient,                    // Main client for non-streaming operations
    streaming_client: Option<CodewhispererStreamingClient>,  // Bearer token streaming
    sigv4_streaming_client: Option<QDeveloperStreamingClient>, // SigV4 auth streaming
    mock_client: Option<Arc<Mutex<std::vec::IntoIter<Vec<ChatResponseStream>>>>>, // Test mocking
    profile: Option<AuthProfile>,                   // User profile
    model_cache: ModelCache,                        // Cached model list
}
```

### 2. Initialization Flow
1. **OS Creation** (`src/os/mod.rs:45`):
   ```rust
   let client = ApiClient::new(&env, &fs, &mut database, None).await?;
   ```

2. **ApiClient::new()** (`src/api_client/mod.rs:88`):
   - Creates bearer token SDK config with dummy credentials
   - Initializes CodewhispererClient with bearer token resolver
   - Conditionally creates streaming clients based on `AMAZON_Q_SIGV4` env var:
     - If `AMAZON_Q_SIGV4` is set: Creates QDeveloperStreamingClient with SigV4 auth
     - Otherwise: Creates CodewhispererStreamingClient with bearer token auth

### 3. Client Configuration
Both clients are configured with:
- HTTP client from `crate::aws_common::http_client::client()`
- OptOutInterceptor for telemetry preferences
- UserAgentOverrideInterceptor for custom user agent
- DelayTrackingInterceptor for performance metrics
- Custom retry classifier
- Stalled stream protection
- 5-minute timeout (DEFAULT_TIMEOUT_DURATION)

### 4. Authentication Methods
- **Bearer Token**: Uses `BearerResolver` from `src/auth/builder_id.rs`
- **SigV4**: Uses `CredentialsChain` from `src/api_client/credentials.rs`

## Dependencies Required for Minimal Implementation

### Core Crates
```toml
amzn-codewhisperer-client = { path = "../amzn-codewhisperer-client" }
amzn-codewhisperer-streaming-client = { path = "../amzn-codewhisperer-streaming-client" }
amzn-qdeveloper-streaming-client = { path = "../amzn-qdeveloper-streaming-client" }
aws-config = "1.0"
aws-credential-types = "1.0"
aws-types = "1.0"
```

### Supporting Components
- Database for settings and auth profiles
- Environment and filesystem abstractions
- Bearer token resolver for authentication
- HTTP client configuration
- Interceptors for telemetry and user agent

## Minimal Integration Path
For the experimental crate, the simplest approach would be:
1. Create a minimal ApiClient wrapper that only initializes the streaming client
2. Use environment variables to configure endpoint and auth method
3. Skip database integration initially (use in-memory settings)
4. Use dummy implementations for interceptors
5. Focus on the streaming chat functionality first

## Key Files to Reference
- `src/api_client/mod.rs` - Main client implementation
- `src/os/mod.rs` - OS initialization including client creation
- `src/auth/builder_id.rs` - Bearer token authentication
- `src/api_client/credentials.rs` - SigV4 credentials chain
- `src/aws_common/http_client.rs` - HTTP client configuration
