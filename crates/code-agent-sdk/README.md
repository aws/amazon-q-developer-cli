# Code Agent SDK

A language-agnostic code intelligence library that provides semantic code understanding capabilities through Language Server Protocol (LSP) integration for LLM tools and applications.

## 🎯 Overview

This library enables LLM tools to access the same semantic code understanding that developers use in their IDEs. It provides a unified interface to multiple language servers, allowing AI agents to navigate codebases, find symbols, understand references, and perform code operations across different programming languages.

## ✨ Features

- **Multi-language Support**: TypeScript/JavaScript, Rust, Python (extensible architecture)
- **Symbol Management**: Find and locate symbols with fuzzy search capabilities
- **Reference Finding**: Locate all symbol usages across the codebase
- **Go-to-Definition**: Navigate to symbol definitions with precise location data
- **Rename Operations**: Rename symbols with workspace-wide updates (dry-run supported)
- **Language-Agnostic Design**: Easy to add support for new languages via configuration
- **LSP Protocol Compliance**: Uses standard LSP types and methods for maximum compatibility

## 🏗️ Architecture

### Core Components

```
code-agent-sdk/
├── src/
│   ├── lib.rs               # Library entry point
│   ├── sdk/
│   │   ├── client.rs        # Main CodeIntelligence API
│   │   ├── services/        # Service implementations
│   │   └── workspace_manager.rs # Workspace management
│   ├── model/
│   │   ├── types.rs         # Request/response types
│   │   └── entities.rs      # Core data structures
│   ├── lsp/
│   │   ├── client.rs        # LSP client implementation
│   │   ├── protocol.rs      # LSP message handling
│   │   └── config.rs        # LSP configuration
│   ├── config/              # Language server configurations
│   ├── utils/               # Utility functions
│   ├── mcp/                 # Model Context Protocol server
│   └── cli/                 # CLI tool
├── tests/
│   ├── e2e_integration.rs   # E2E integration tests
│   ├── e2e/                 # E2E test modules
│   └── samples/             # Test projects for each language
└── validate.sh              # Complete validation suite
```

### Architecture Principles

1. **Language-Agnostic Core**: The `CodeIntelligence` struct provides a unified API regardless of the underlying language server
2. **LSP Protocol Compliance**: All communication uses standard LSP types from the `lsp-types` crate
3. **Configurable Language Servers**: Easy to add new languages via `LanguageServerConfig`
4. **Async/Await Design**: Non-blocking operations for better performance
5. **Error Handling**: Comprehensive error handling with `anyhow::Result`

### Data Flow

```
LLM Tool Request → CodeIntelligence API → LSP Client → Language Server
                                                    ↓
LLM Tool Response ← Processed LSP Types ← LSP Response ← Language Server
```

## 📚 Documentation

For comprehensive documentation, see the [docs](docs/) directory:

- **[API Reference](docs/api/API_REFERENCE.md)** - Complete API documentation
- **[Architecture](docs/architecture/ARCHITECTURE.md)** - System design overview  
- **[Testing Guide](docs/testing/TEST_ANALYSIS_REPORT.md)** - Test strategy and coverage
- **[MCP Server](docs/guides/MCP_SERVER.md)** - Model Context Protocol integration
- **[Development Guide](docs/NEXT_PHASE_TASKS.md)** - Planned features and roadmap

## 🚀 Quick Start

### Prerequisites

Install the required language servers:

```bash
# TypeScript/JavaScript
npm install -g typescript-language-server typescript

# Rust
rustup component add rust-analyzer

# Python
pip install python-lsp-server
```

### Library Usage

```rust
use code_agent_sdk::{CodeIntelligence, FindSymbolsRequest};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a TypeScript-enabled code intelligence instance
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(std::env::current_dir()?)
        .add_language("typescript")
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    // Initialize language servers
    code_intel.initialize().await?;
    
    // Find symbols
    let request = FindSymbolsRequest {
        symbol_name: "function_name".to_string(),
        file_path: None,
        symbol_type: None,
        limit: Some(10),
        exact_match: false,
    };
    
    let symbols = code_intel.find_symbols(request).await?;
    println!("Found {} symbols", symbols.len());
    
    Ok(())
}
```

### CLI Usage

```bash
# Build the project
cargo build

# Analyze a file (shows symbols and workspace search)
cargo run --bin code-agent-cli test_file.ts

# Test go-to-definition and find-references at specific position
cargo run --bin code-agent-cli test_file.ts 6 20
```

## 📋 Core API Reference

For complete API documentation with all inputs, outputs, and lifecycle examples, see **[API_REFERENCE.md](API_REFERENCE.md)**.

### Quick API Overview

### CodeIntelligence Methods

#### `find_symbols(request: FindSymbolsRequest) -> Result<Vec<WorkspaceSymbol>>`
Find symbols using fuzzy search. Returns symbol name, location, and metadata.

**Input:**
- `symbol_name`: String to search for (empty string returns all symbols)
- `file_path`: Optional file to search within
- `symbol_type`: Optional LSP SymbolKind filter

**Output:** Array of `WorkspaceSymbol` with:
- Symbol name and type
- File location (URI)
- Start/end positions (line, character)

#### `get_symbols(request: GetSymbolsRequest) -> Result<Vec<WorkspaceSymbol>>`
Direct symbol retrieval for existence checking or code extraction.

#### `find_references(request: FindReferencesRequest) -> Result<Vec<Location>>`
Find all references to a symbol at a specific position.

**Input:**
- `file_path`: File containing the symbol
- `start_row`, `start_column`: Position of the symbol

**Output:** Array of `Location` with file URI and position ranges

#### `goto_definition(file_path, line, character) -> Result<Option<GotoDefinitionResponse>>`
Navigate to symbol definition.

#### `rename_symbol(request: RenameSymbolRequest) -> Result<Option<WorkspaceEdit>>`
Rename symbols with workspace-wide updates.

**Input:**
- `file_path`, `start_row`, `start_column`: Symbol position
- `new_name`: New symbol name
- `dry_run`: Preview changes without applying

#### `open_file(file_path, content) -> Result<()>`
Open a file in the language server for analysis.

#### `close_file(file_path) -> Result<()>`
Close a file in the language server.

## 🌐 Language Support

### Built-in Languages

| Language | Extensions | Server | Installation |
|----------|------------|--------|--------------|
| TypeScript/JavaScript | `.ts`, `.js` | `typescript-language-server` | `npm install -g typescript-language-server typescript` |
| Rust | `.rs` | `rust-analyzer` | `rustup component add rust-analyzer` |
| Python | `.py` | `pylsp` | `pip install python-lsp-server` |

### Adding New Languages

```rust
use code_agent_sdk::{CodeIntelligence, LanguageServerConfig};

let mut code_intel = CodeIntelligence::new(workspace_root);

// Add custom language server
code_intel.add_language_server(LanguageServerConfig {
    name: "my-language-server".to_string(),
    command: "my-lsp-server".to_string(),
    args: vec!["--stdio".to_string()],
    file_extensions: vec!["mylang".to_string()],
    initialization_options: Some(serde_json::json!({
        "custom": "options"
    })),
});
```

### Language Server Requirements

All language servers must:
1. Support LSP 3.16+ protocol
2. Accept `--stdio` communication mode
3. Implement required LSP methods:
   - `initialize` / `initialized`
   - `textDocument/didOpen` / `textDocument/didClose`
   - `textDocument/definition`
   - `textDocument/references`
   - `workspace/symbol`
   - `textDocument/documentSymbol`

## 🧪 Testing & Quality Assurance

### Running Tests

```bash
# Quick validation (recommended)
./validate.sh

# Individual test commands
cargo check                           # Compilation check
cargo fmt --check                     # Code formatting
cargo clippy -- -D warnings -A deprecated  # Linting
cargo test --lib                      # Unit tests
cargo test --test integration_tests   # Integration tests
```

### Integration Tests

The integration tests validate real LSP server functionality:

```bash
# Run all integration tests
cargo test --test integration_tests

# Run specific language test
cargo test --test integration_tests test_typescript_integration
cargo test --test integration_tests test_rust_integration
```

**Test Coverage:**
- ✅ Symbol finding in files and workspace
- ✅ Go-to-definition at specific positions  
- ✅ Find references for symbols
- ✅ Language server initialization and communication
- ✅ File open/close operations
- ✅ Error handling and edge cases

### Test Samples

Located in `tests/samples/`, each language has a complete project:

```
tests/samples/
├── test.ts          # TypeScript test file
├── package.json     # NPM project configuration
├── tsconfig.json    # TypeScript configuration
├── test.rs          # Rust test file
├── Cargo.toml       # Rust project configuration
└── test.py          # Python test file
```

### Regression Testing

The validation suite prevents regressions by testing:

1. **Compilation**: Code compiles without errors
2. **Formatting**: Code follows consistent style
3. **Linting**: No code quality issues
4. **Unit Tests**: Core functionality works
5. **Integration Tests**: Real LSP server communication
6. **CLI Functionality**: End-to-end user experience

### Continuous Integration

For CI/CD pipelines:

```yaml
# Example GitHub Actions
- name: Validate Code Intelligence
  run: |
    # Install language servers
    npm install -g typescript-language-server typescript
    rustup component add rust-analyzer
    pip install python-lsp-server
    
    # Run validation
    ./validate.sh
```

## 🔧 Development

### Project Structure

```
code-agent-sdk/
├── Cargo.toml                 # Rust project configuration
├── README.md                  # This documentation
├── validate.sh                # Validation script
├── test_file.ts              # CLI demo file
├── .gitignore                # Git exclusions
├── src/                      # Source code
│   ├── lib.rs                # Library entry point
│   ├── core.rs               # Main CodeIntelligence implementation
│   ├── types.rs              # Type definitions using LSP types
│   ├── cli/                  # CLI implementation
│   │   └── cli.rs            # Command-line interface
│   └── lsp/                  # LSP client implementation
│       ├── mod.rs            # Module exports
│       ├── client.rs         # LSP client with language server management
│       └── protocol.rs       # LSP message parsing and communication
└── tests/                    # Test suite
    ├── integration_tests.rs  # Integration tests
    └── samples/              # Test projects for each language
        ├── test.ts           # TypeScript sample
        ├── test.rs           # Rust sample
        ├── test.py           # Python sample
        ├── package.json      # NPM configuration
        ├── tsconfig.json     # TypeScript configuration
        └── Cargo.toml        # Rust configuration
```

### Dependencies

```toml
[dependencies]
tokio = { version = "1.32.0", features = ["full"] }  # Async runtime
serde_json = "1.0.107"                               # JSON serialization
serde = { version = "1.0.188", features = ["derive"] } # Serialization
lsp-types = "0.95.0"                                 # LSP type definitions
url = "2.5.0"                                        # URL handling
anyhow = "1.0"                                       # Error handling
thiserror = "1.0"                                    # Error types
uuid = { version = "1.0", features = ["v4"] }       # Unique identifiers
futures = "0.3.28"                                   # Future utilities
async-trait = "0.1"                                  # Async traits
```

### Code Style

- **Formatting**: Use `cargo fmt`
- **Linting**: Use `cargo clippy`
- **Error Handling**: Use `anyhow::Result` for public APIs
- **Async**: All I/O operations are async
- **Documentation**: Document public APIs with examples

### Adding Features

1. **Add LSP Method**: Implement in `lsp/client.rs`
2. **Add API Method**: Add to `core.rs` with proper error handling
3. **Add Types**: Define request/response types in `types.rs`
4. **Add Tests**: Create integration tests in `tests/integration_tests.rs`
5. **Update Documentation**: Update this README

## 🐛 Troubleshooting

### Common Issues

**Language Server Not Found**
```bash
# Check if language server is installed
which typescript-language-server
which rust-analyzer
which pylsp

# Install missing servers (see Language Support section)
```

**LSP Communication Errors**
```bash
# Check language server version compatibility
typescript-language-server --version
rust-analyzer --version
pylsp --version

# Enable LSP tracing (modify TraceValue::Off to TraceValue::Verbose in client.rs)
```

**File Path Issues**
```bash
# Ensure files exist and are readable
ls -la test_file.ts

# Use absolute paths in API calls
let absolute_path = file_path.canonicalize()?;
```

**Integration Test Failures**
```bash
# Run tests with output
cargo test --test integration_tests -- --nocapture

# Check if language servers are available
./validate.sh
```

### Debug Mode

Enable verbose LSP communication by changing in `src/lsp/client.rs`:
```rust
trace: Some(TraceValue::Verbose),  // Instead of TraceValue::Off
```

## 📈 Performance Considerations

- **Language Server Startup**: First request may be slower due to server initialization
- **File Watching**: Language servers may watch file system for changes
- **Memory Usage**: Each language server runs as a separate process
- **Concurrent Requests**: Library supports multiple concurrent operations
- **Caching**: Language servers cache analysis results for better performance

## 🤝 Contributing

1. **Fork the repository**
2. **Create a feature branch**: `git checkout -b feature/new-language`
3. **Make changes**: Follow code style and add tests
4. **Run validation**: `./validate.sh`
5. **Submit pull request**: Include test coverage and documentation

### Pull Request Checklist

- [ ] Code compiles without warnings
- [ ] All tests pass (`./validate.sh`)
- [ ] New features have integration tests
- [ ] Documentation updated
- [ ] LSP compliance maintained

## 📄 License

MIT License - see LICENSE file for details.

## 🔗 Related Projects

- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
- [lsp-types](https://crates.io/crates/lsp-types) - LSP type definitions for Rust
- [TypeScript Language Server](https://github.com/typescript-language-server/typescript-language-server)
- [rust-analyzer](https://rust-analyzer.github.io/)
- [Python LSP Server](https://github.com/python-lsp/python-lsp-server)

---

**Built for LLM tools that need semantic code understanding** 🤖✨
