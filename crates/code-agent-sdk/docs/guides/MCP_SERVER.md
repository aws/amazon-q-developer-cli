# Code Agent SDK MCP Server

A minimal MCP (Model Context Protocol) server that provides code intelligence capabilities using LSP integration.

## Features

- **init_workspace**: Initialize workspace with auto-detected language servers
- **find_symbols**: Find symbols by name with fuzzy matching
- **goto_definition**: Navigate to symbol definitions
- **find_references**: Find all references to a symbol

## Building

```bash
cargo build --bin code-agent-mcp
```

## Running

```bash
./target/debug/code-agent-mcp
```

The server communicates via stdio using the MCP protocol.

## Tools

### init_workspace
Initialize the workspace with language server detection.

**Parameters:**
- `workspace_root` (optional): Path to workspace root directory

**Example:**
```json
{
  "workspace_root": "/path/to/project"
}
```

### find_symbols
Find symbols by name with fuzzy matching.

**Parameters:**
- `symbol_name`: Name of symbol to search for
- `file_path` (optional): File path to search within
- `limit` (optional): Maximum results to return (default: 10)

**Example:**
```json
{
  "symbol_name": "function_name",
  "limit": 5
}
```

### goto_definition
Go to definition of symbol at specific position.

**Parameters:**
- `file_path`: File path
- `line`: Line number (0-based)
- `character`: Character position (0-based)

**Example:**
```json
{
  "file_path": "src/main.rs",
  "line": 10,
  "character": 5
}
```

### find_references
Find all references to symbol at specific position.

**Parameters:**
- `file_path`: File path
- `line`: Line number (0-based)
- `character`: Character position (0-based)

**Example:**
```json
{
  "file_path": "src/main.rs",
  "line": 10,
  "character": 5
}
```

## Integration

This MCP server can be integrated with any MCP-compatible client (Claude Desktop, etc.) to provide code intelligence capabilities.

## Architecture

- Uses the existing CodeIntelligence SDK
- Auto-detects workspace languages
- Initializes appropriate language servers (TypeScript, Rust, Python)
- Provides unified interface via MCP protocol
