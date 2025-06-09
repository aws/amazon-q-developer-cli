# Amazon Q Developer CLI Codebase Summary

## Overview

The **Amazon Q Developer CLI** is a command-line interface that provides AI-powered assistance to developers through natural language chat and tool execution capabilities. The project is built primarily in Rust and integrates with AWS services including CodeWhisperer and Q Developer.

## Key Components

1. **CLI Application** (`crates/cli/`): The main command-line interface implementation
2. **AWS Service Clients** (`crates/amzn-*`): Client libraries for various AWS services
3. **Model Context Protocol (MCP)**: Extensibility framework for custom tools and integrations
4. **Build Scripts** (`scripts/`): Python scripts for building, signing, and testing

## Project Structure

### Core Crates

- `crates/cli/` - Main CLI application with the following modules:
  - `api_client/` - Handles communication with AWS services
  - `auth/` - Authentication and authorization (Builder ID, SSO)
  - `cli/` - Command implementations (chat, settings, diagnostics, etc.)
  - `database/` - Local SQLite database for settings and state
  - `mcp_client/` - Model Context Protocol client
  - `platform/` - Platform-specific implementations
  - `telemetry/` - Usage tracking and analytics
  - `util/` - Common utilities

### AWS Service Clients

- `amzn-codewhisperer-client/` - AWS CodeWhisperer service client
- `amzn-codewhisperer-streaming-client/` - Streaming client for CodeWhisperer
- `amzn-consolas-client/` - Client for Consolas service
- `amzn-qdeveloper-streaming-client/` - Streaming client for Q Developer
- `amzn-toolkit-telemetry-client/` - Telemetry client

### Build and Scripts

- `scripts/` - Python scripts for:
  - `build.py` - Build orchestration
  - `signing.py` - Code signing
  - `test.py` - Test execution
  - `doc.py` - Documentation generation
  - `manifest.py` - Manifest management
  - `setup.sh` - Development environment setup

## Amazon Q Chat Implementation

### Core Components

1. **Chat Module** (`crates/cli/src/cli/chat/`)
   - Interactive terminal-based chat interface
   - Command parsing and execution
   - Response streaming and rendering
   - Tool management and execution

2. **Conversation Management**
   - `ConversationState` maintains chat history and context
   - Supports multi-turn conversations with context preservation
   - Tracks environmental state (working directory, shell environment)

3. **Tool System**
   - Built-in tools:
     - `fs_read`: Read files and list directories
     - `fs_write`: Create or modify files
     - `execute_bash`: Execute shell commands
     - `use_aws`: Execute AWS CLI commands
     - `gh_issue`: Create GitHub issues
     - `thinking`: Internal reasoning display
   - User confirmation required for system-modifying operations
   - Tool trust system for skipping confirmations

### Model Context Protocol (MCP)

MCP enables extensibility by allowing external programs to provide tools to Amazon Q:

1. **MCP Commands** (`q mcp`)
   - `add`: Add MCP server configurations
   - `remove`: Remove MCP servers
   - `list`: List configured servers
   - `import`: Import server configurations
   - `status`: Check server status

2. **Configuration**
   - Workspace-level: `.amazonq/mcp.json`
   - Global-level: `~/.aws/amazonq/mcp.json`

3. **Integration**
   - MCP servers expose tools via JSON-RPC protocol
   - Communication over stdio or websocket
   - Tools are namespaced as `{server_name}____{tool_name}`
   - Dynamic tool discovery during chat initialization

4. **Features**
   - Environment variable support
   - Timeout controls
   - Process lifecycle management
   - Tool trust integration

### Technical Implementation

1. **API Communication**
   - Uses AWS SDK clients for service communication
   - Streaming responses for real-time interaction
   - Supports both CodeWhisperer and Q Developer backends

2. **User Interface**
   - Built with `rustyline` for readline-like editing
   - `crossterm` for terminal control
   - Syntax highlighting with `syntect`
   - Markdown rendering in terminal

3. **Authentication**
   - Builder ID authentication flow
   - SSO support for enterprise users
   - Token refresh and management

4. **Platform Support**
   - Cross-platform: macOS, Linux, Windows (via WSL)
   - Platform-specific code in `platform/` module
   - SSH support for remote development

## CLI Commands

The main binary is `q` with the following subcommands:

- `chat` (default): Interactive AI chat with tool execution
- `login`: Authenticate with AWS
- `logout`: Sign out
- `whoami`: Display current user
- `profile`: Manage context profiles
- `settings`: Configure appearance and behavior
- `diagnostic`: Run diagnostic tests
- `issue`: Create GitHub issues
- `mcp`: Manage Model Context Protocol servers
- `version`: Display version information

## Development Workflow

1. **Prerequisites**
   - Rust toolchain (stable + nightly for formatting)
   - Python 3.8+ (for build scripts)
   - Node.js 18+ (for some tooling)
   - Platform-specific dependencies

2. **Build and Test**
   - `cargo build`: Build the project
   - `cargo run --bin cli`: Run the CLI
   - `cargo run --bin cli -- chat`: Run the chat interface
   - `cargo test -p cli`: Run tests
   - `cargo +nightly fmt`: Format code
   - `cargo clippy`: Run linter

3. **Configuration**
   - Uses `mise` for managing Python/Node versions
   - Pre-commit hooks via `pnpm install`
   - Telemetry definitions in `telemetry_definitions.json`

The project has evolved from a multi-component desktop application to a focused CLI tool that provides AI-powered assistance directly in the terminal, with extensibility through the Model Context Protocol.