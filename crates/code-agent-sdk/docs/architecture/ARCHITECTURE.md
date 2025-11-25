# Code Agent SDK - Architecture Documentation

## 🏗️ System Architecture

### High-Level Overview

The Code Agent SDK is designed as a **language-agnostic semantic code analysis system** that bridges LLM tools with Language Server Protocol (LSP) servers. It provides a unified API for code understanding across multiple programming languages.

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────────┐
│   LLM Tools     │    │  Code Agent SDK  │    │  Language Servers   │
│                 │    │                  │    │                     │
│ • Q CLI         │◄──►│                  │◄──►│ • typescript-ls     │
│ • AI Agents     │    │ • Unified API     │    │ • rust-analyzer     │
│ • Code Bots     │    │ • Multi-language  │    │ • pylsp             │
│                 │    │ • LSP Protocol    │    │ • ...               │
└─────────────────┘    └──────────────────┘    └─────────────────────┘
```

### Core Design Principles

1. **Language Agnostic**: Single API works across all supported languages
2. **LSP Compliant**: Uses standard LSP protocol for maximum compatibility
3. **Async First**: Non-blocking operations for better performance
4. **Extensible**: Easy to add new language servers
5. **Type Safe**: Leverages Rust's type system and LSP types
6. **Error Resilient**: Comprehensive error handling and graceful degradation

## 📦 Module Architecture

### Core Modules

```
src/
├── lib.rs              # Public API exports
├── sdk/
│   ├── client.rs       # Main CodeIntelligence struct
│   ├── services/       # Service implementations
│   └── workspace_manager.rs # Workspace management
├── model/
│   ├── types.rs        # Request/response types
│   └── entities.rs     # Core data structures
├── lsp/                # LSP implementation
│   ├── client.rs       # LSP client implementation
│   ├── protocol.rs     # LSP message handling
│   └── config.rs       # LSP configuration
├── config/             # Language server configurations
├── utils/              # Utility functions
├── mcp/                # Model Context Protocol server
└── cli/                # CLI demonstration
    └── cli.rs          # Command-line interface
```

### Module Responsibilities

#### `sdk/client.rs` - Main API Layer
- **Purpose**: High-level API that LLM tools interact with
- **Key Components**:
  - `CodeIntelligence` struct - Main entry point
  - Language server management
  - Request routing and response processing
  - File lifecycle management

#### `lsp/client.rs` - LSP Client Layer
- **Purpose**: Language-agnostic LSP communication
- **Key Components**:
  - `LspClient` struct - Manages individual language server
  - Async message handling
  - Request/response correlation
  - Language server process management

#### `lsp/protocol.rs` - Protocol Layer
- **Purpose**: LSP message parsing and serialization
- **Key Components**:
  - Message reading/writing with proper headers
  - JSON-RPC protocol handling
  - Error parsing and handling

#### `types.rs` - Type System
- **Purpose**: Type definitions for requests and responses
- **Key Components**:
  - Request types (`FindSymbolsRequest`, `FindReferencesRequest`, etc.)
  - Configuration types (`LanguageServerConfig`)
  - Uses LSP types from `lsp-types` crate

## 🔄 Data Flow Architecture

### Request Processing Flow

```
1. LLM Tool Request
   ↓
2. CodeIntelligence API
   ↓
3. Language Detection (by file extension)
   ↓
4. LSP Client Selection
   ↓
5. LSP Request Formation
   ↓
6. Language Server Communication
   ↓
7. LSP Response Processing
   ↓
8. Type Conversion
   ↓
9. Response to LLM Tool
```

### Detailed Flow Example: `find_symbols`

```rust
// 1. LLM Tool calls API
let symbols = code_intel.find_symbols(request).await?;

// 2. Core.rs processes request
pub async fn find_symbols(&self, request: FindSymbolsRequest) -> Result<Vec<WorkspaceSymbol>> {
    // 3. Route to appropriate client
    let client = self.get_client_for_file(&file_path)?;
    
    // 4. Convert to LSP request
    let params = WorkspaceSymbolParams { query: request.symbol_name, ... };
    
    // 5. Send LSP request
    let response = client.workspace_symbols(params).await?;
    
    // 6. Process and return
    Ok(response.unwrap_or_default())
}
```

## 🌐 Language Server Integration

### Language Server Lifecycle

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌──────────────┐
│   Spawn     │───►│ Initialize   │───►│   Ready     │───►│  Shutdown    │
│  Process    │    │   (LSP)      │    │ (Serving)   │    │              │
└─────────────┘    └──────────────┘    └─────────────┘    └──────────────┘
```

### Configuration System

Each language server is configured via `LanguageServerConfig`:

```rust
pub struct LanguageServerConfig {
    pub name: String,                    // Unique identifier
    pub command: String,                 // Executable name
    pub args: Vec<String>,              // Command arguments
    pub file_extensions: Vec<String>,    // Supported file types
    pub initialization_options: Option<Value>, // LSP init options
}
```

### Built-in Configurations

| Language | Command | Args | Extensions | Init Options |
|----------|---------|------|------------|--------------|
| TypeScript | `typescript-language-server` | `["--stdio"]` | `["ts", "js"]` | TypeScript preferences |
| Rust | `rust-analyzer` | `[]` | `["rs"]` | None |
| Python | `pylsp` | `[]` | `["py"]` | None |

## 🔧 LSP Protocol Implementation

### Message Format

All LSP communication follows the JSON-RPC 2.0 protocol:

```
Content-Length: 123\r\n
\r\n
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "textDocument/definition",
  "params": { ... }
}
```

### Supported LSP Methods

#### Core Methods
- `initialize` / `initialized` - Server initialization
- `textDocument/didOpen` - Open file for analysis
- `textDocument/didClose` - Close file

#### Query Methods
- `textDocument/definition` - Go to definition
- `textDocument/references` - Find references
- `textDocument/documentSymbol` - File symbols
- `workspace/symbol` - Workspace-wide symbol search
- `textDocument/rename` - Rename symbol

### Request/Response Correlation

The library maintains a correlation system for async requests:

```rust
// Each request gets a unique ID
let id = self.next_id.fetch_add(1, Ordering::SeqCst);

// Store callback for response
self.pending_requests.insert(id, callback);

// Send request with ID
let request = json!({
    "jsonrpc": "2.0",
    "id": id,
    "method": method,
    "params": params
});
```

## 🧪 Testing Architecture

### Test Structure

```
tests/
├── integration_tests.rs    # End-to-end LSP tests
└── samples/               # Test projects
    ├── test.ts            # TypeScript sample
    ├── test.rs            # Rust sample
    ├── test.py            # Python sample
    ├── package.json       # NPM config
    ├── tsconfig.json      # TS config
    └── Cargo.toml         # Rust config
```

### Test Categories

1. **Unit Tests**: Individual component testing
2. **Integration Tests**: Real LSP server communication
3. **CLI Tests**: End-to-end user experience
4. **Regression Tests**: Prevent functionality breakage

### Validation Pipeline

```bash
./validate.sh runs:
├── cargo check          # Compilation
├── cargo fmt --check    # Code formatting
├── cargo clippy         # Linting
├── cargo test --lib     # Unit tests
├── cargo test --test    # Integration tests
└── CLI functionality    # End-to-end test
```

## 🚀 Performance Considerations

### Async Architecture

- **Non-blocking I/O**: All LSP communication is async
- **Concurrent Requests**: Multiple requests can be in-flight
- **Efficient Message Parsing**: Streaming JSON-RPC parsing

### Memory Management

- **Process Isolation**: Each language server runs in separate process
- **Resource Cleanup**: Proper file closing and server shutdown
- **Caching**: Language servers cache analysis results

### Scalability

- **Multiple Clients**: Can manage multiple language servers simultaneously
- **Request Queuing**: Built-in request correlation and queuing
- **Error Recovery**: Graceful handling of server failures

## 🔒 Error Handling Strategy

### Error Types

1. **Configuration Errors**: Invalid language server setup
2. **Communication Errors**: LSP protocol failures
3. **Server Errors**: Language server crashes or errors
4. **File System Errors**: Invalid paths or permissions

### Error Propagation

```rust
// All public APIs return Result<T>
pub async fn find_symbols(&self, request: FindSymbolsRequest) -> Result<Vec<WorkspaceSymbol>>

// Internal error conversion
impl From<serde_json::Error> for CodeIntelligenceError
impl From<std::io::Error> for CodeIntelligenceError
```

### Graceful Degradation

- **Server Unavailable**: Skip tests if language server not installed
- **Partial Failures**: Return partial results when possible
- **Timeout Handling**: Reasonable timeouts for LSP requests

## 🔮 Extension Points

### Adding New Languages

1. **Create Configuration**:
```rust
code_intel.add_language_server(LanguageServerConfig {
    name: "go-language-server".to_string(),
    command: "gopls".to_string(),
    args: vec!["serve".to_string()],
    file_extensions: vec!["go".to_string()],
    initialization_options: None,
});
```

2. **Add Tests**: Create test samples and integration tests
3. **Update Documentation**: Add to supported languages list

### Adding New LSP Methods

1. **Add to LSP Client**:
```rust
pub async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    // Implementation
}
```

2. **Add to Core API**:
```rust
pub async fn get_hover(&self, file_path: &Path, line: u32, character: u32) -> Result<Option<Hover>> {
    // Route to appropriate client
}
```

3. **Add Request Type**: Define in `types.rs`
4. **Add Tests**: Integration and unit tests

## 📊 Metrics and Observability

### Logging Strategy

- **Error Logging**: All errors are logged with context
- **Debug Tracing**: Optional verbose LSP communication logging
- **Performance Metrics**: Request timing and success rates

### Debug Mode

Enable verbose LSP tracing:
```rust
trace: Some(TraceValue::Verbose)  // In client.rs
```

This architecture provides a solid foundation for semantic code understanding that can scale across multiple languages and integrate seamlessly with LLM tools.
