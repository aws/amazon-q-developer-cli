---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: code
  description: LSP-powered code intelligence for semantic symbol search, references, definitions, and diagnostics
  keywords: [code, lsp, symbols, references, definition, diagnostics, intelligence]
  related: [fs-read, grep, slash-code]
---

# code

LSP-powered code intelligence for semantic symbol search, references, definitions, and diagnostics.

## Overview

The code tool provides IDE-quality code understanding through Language Server Protocol integration. Search for symbols by name, find all references, navigate to definitions, get diagnostics, and rename symbols across your codebase. Supports TypeScript, Rust, Python, Go, Java, Ruby, and C/C++.

## How It Works

Code intelligence uses LSP servers (rust-analyzer, typescript-language-server, pyright, etc.) to analyze your codebase. Initialize with `/code init` to detect languages and start servers. The tool then provides semantic operations that understand code structure, not just text patterns.

## Usage

### Basic Usage

```json
{
  "operation": "search_symbols",
  "symbol_name": "UserService"
}
```

### Common Use Cases

#### Use Case 1: Find Symbol Definition

```json
{
  "operation": "search_symbols",
  "symbol_name": "authenticate",
  "symbol_type": "Function"
}
```

**What this does**: Finds WHERE the authenticate function is DEFINED across the workspace.

#### Use Case 2: Find All Usages

```json
{
  "operation": "find_references",
  "file_path": "src/auth.ts",
  "row": 42,
  "column": 10
}
```

**What this does**: Finds all places where the symbol at that position is used.

#### Use Case 3: Navigate to Definition

```json
{
  "operation": "goto_definition",
  "file_path": "src/main.ts",
  "row": 25,
  "column": 12,
  "show_source": true
}
```

**What this does**: Shows where the symbol is defined with source code.

#### Use Case 4: List File Symbols

```json
{
  "operation": "get_document_symbols",
  "file_path": "src/user.service.ts",
  "top_level_only": true
}
```

**What this does**: Lists all top-level symbols (classes, functions) in the file.

#### Use Case 5: Get Errors and Warnings

```json
{
  "operation": "get_diagnostics",
  "file_path": "src/main.ts"
}
```

**What this does**: Returns compiler errors, warnings, and hints for the file.

## Configuration

No agent configuration - code tool is trusted by default. Initialize workspace with `/code init`.

## Operations

### search_symbols

Find symbols by name across workspace.

**Parameters**:
- `symbol_name` (string, required): Name to search for
- `file_path` (string, optional): Limit to specific file
- `symbol_type` (string, optional): Filter by type (Function, Method, Class, Struct, Enum, Interface, Constant, Variable, Module, Import)
- `limit` (integer, optional): Max results (default 50, max 50)
- `language` (string, optional): Filter by language (rust, typescript, python, etc.)
- `exact_match` (boolean, optional): Require exact name match (default false)

### find_references

Find all references to symbol at position.

**Parameters**:
- `file_path` (string, required): File containing symbol
- `row` (integer, required): Line number (1-based)
- `column` (integer, required): Column number (1-based)
- `limit` (integer, optional): Max results (default 500, max 1000)
- `workspace_only` (boolean, optional): Exclude dependencies (default true)

### goto_definition

Navigate to symbol definition.

**Parameters**:
- `file_path` (string, required): File containing symbol
- `row` (integer, required): Line number (1-based)
- `column` (integer, required): Column number (1-based)
- `show_source` (boolean, optional): Include source code (default true)

### get_document_symbols

List symbols in file.

**Parameters**:
- `file_path` (string, required): File to analyze
- `top_level_only` (boolean, optional): Only top-level symbols (default true)

### lookup_symbols

Look up specific symbols by name.

**Parameters**:
- `symbols` (array, required): List of symbol names
- `file_path` (string, optional): Limit to specific file

### rename_symbol

Rename symbol across codebase.

**Parameters**:
- `file_path` (string, required): File containing symbol
- `row` (integer, required): Line number (1-based)
- `column` (integer, required): Column number (1-based)
- `new_name` (string, required): New symbol name
- `dry_run` (boolean, optional): Preview without applying (default false)

### get_diagnostics

Get errors and warnings.

**Parameters**:
- `file_path` (string, required): File to check

### get_hover

Get type info and docs at position.

**Parameters**:
- `file_path` (string, required): File path
- `row` (integer, required): Line number (1-based)
- `column` (integer, required): Column number (1-based)

### get_completions

Get available completions at position.

**Parameters**:
- `file_path` (string, required): File path
- `row` (integer, required): Line number (1-based)
- `column` (integer, required): Column number (1-based)
- `filter` (string, optional): Fuzzy search filter (recommended)
- `symbol_type` (string, optional): Filter by type
- `trigger_character` (string, optional): Trigger character (`.`, `::`, etc.)

### initialize_workspace

Initialize LSP servers for workspace.

**Parameters**: None

## Examples

### Example 1: Find Class Definition

```json
{
  "operation": "search_symbols",
  "symbol_name": "UserRepository",
  "symbol_type": "Class"
}
```

### Example 2: Find All Function Calls

```json
{
  "operation": "find_references",
  "file_path": "src/auth.ts",
  "row": 42,
  "column": 10,
  "limit": 100
}
```

### Example 3: Rename with Preview

```json
{
  "operation": "rename_symbol",
  "file_path": "src/user.ts",
  "row": 15,
  "column": 5,
  "new_name": "fetchUserData",
  "dry_run": true
}
```

### Example 4: Get Type Information

```json
{
  "operation": "get_hover",
  "file_path": "src/main.rs",
  "row": 25,
  "column": 10
}
```

## Troubleshooting

### Issue: "Workspace is still initializing"

**Symptom**: Operations fail with initialization message  
**Cause**: LSP servers starting up  
**Solution**: Wait a moment and retry. If persists, use `/code init -f` to restart servers.

### Issue: "No symbols found"

**Symptom**: search_symbols returns empty  
**Cause**: Symbol doesn't exist, wrong name, or LSP still indexing  
**Solution**: Check spelling, try broader search, wait for indexing to complete.

### Issue: "No definition found"

**Symptom**: goto_definition fails  
**Cause**: Position doesn't point to a symbol  
**Solution**: Verify row/column point to symbol name, not whitespace or comments.

### Issue: LSP Server Not Starting

**Symptom**: `/code init` shows server not initialized  
**Cause**: Language server not installed  
**Solution**: Install required language server (see docs/code-intelligence.md for install commands).

### Issue: Slow Performance

**Symptom**: Operations take long time  
**Cause**: Large codebase, initial indexing  
**Solution**: Wait for initial indexing. Subsequent operations will be faster.

## Related Features

- [/code](../slash-commands/code.md) - Slash commands for code intelligence
- [grep](grep.md) - Text-based pattern search
- [fs_read](fs-read.md) - Read source files

## Limitations

- Requires language server installation
- Initial indexing can be slow for large codebases
- LSP feature support varies by language server
- Position-based operations require exact row/column
- Some servers don't support all operations (rename, format, etc.)
- Max 50 results for search_symbols and get_completions
- Max 1000 results for find_references

## Technical Details

**Aliases**: `code`

**Supported Languages**: TypeScript, JavaScript, Rust, Python, Go, Java, Ruby, C, C++

**LSP Servers**:
- TypeScript/JavaScript: typescript-language-server
- Rust: rust-analyzer
- Python: pyright
- Go: gopls
- Java: jdtls
- Ruby: solargraph
- C/C++: clangd

**Initialization**: Run `/code init` in project root. Creates `lsp.json` config. Auto-initializes on subsequent startups.

**Permissions**: Trusted by default, no configuration needed.

**Position Format**: Row and column are 1-based (first line is 1, first column is 1).

**Symbol Types**: Function, Method, Class, Struct, Enum, Interface, Constant, Variable, Module, Import.
