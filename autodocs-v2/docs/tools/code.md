---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: true
  category: tool
  title: code
  description: Code intelligence with tree-sitter (built-in) and LSP (optional) for symbol search, pattern matching, and codebase exploration
  keywords: [code, lsp, symbols, references, definition, diagnostics, intelligence, tree-sitter, pattern, ast, codebase-overview]
  related: [fs-read, grep, slash-code]
---

# code

Code intelligence with tree-sitter (built-in) and LSP (optional) for symbol search, pattern matching, and codebase exploration.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to analyze code as needed.

The code tool provides two layers of code understanding:

**Tree-sitter (Built-in)** - Out-of-the-box support for 18 languages. Search symbols with fuzzy matching, pattern search/rewrite, and codebase exploration without installing any LSP.

**LSP (Optional)** - Enhanced precision with find references, go to definition, hover, rename, and diagnostics. Initialize with `/code init`.

Supported languages: Bash, C, C++, C#, Elixir, Go, Java, JavaScript, Kotlin, Lua, PHP, Python, Ruby, Rust, Scala, Swift, TSX, TypeScript

## How It Works

Tree-sitter operations work immediately - no setup required. For LSP features, run `/code init` to detect languages and start servers. The tool then provides semantic operations that understand code structure.

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

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

#### Use Case 2: Pattern Search (AST-based)

```json
{
  "operation": "pattern_search",
  "pattern": "console.log($ARG)",
  "language": "javascript"
}
```

**What this does**: Finds all console.log calls using AST matching.

#### Use Case 3: Pattern Rewrite

```json
{
  "operation": "pattern_rewrite",
  "pattern": "var $N = $V",
  "replacement": "const $N = $V",
  "language": "javascript",
  "dry_run": true
}
```

**What this does**: Previews converting var to const across the codebase.

#### Use Case 4: Codebase Overview

```json
{
  "operation": "generate_codebase_overview",
  "path": "./src"
}
```

**What this does**: Gets high-level structure of the src directory.

#### Use Case 5: Find All Usages (LSP)

```json
{
  "operation": "find_references",
  "file_path": "src/auth.ts",
  "row": 42,
  "column": 10
}
```

**What this does**: Finds all places where the symbol at that position is used.

#### Use Case 6: Navigate to Definition (LSP)

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

## Configuration

No agent configuration - code tool is trusted by default. For LSP features, initialize workspace with `/code init`.

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

Get errors and warnings (LSP required).

**Parameters**:
- `file_path` (string, required): File to check

### get_hover

Get type info and docs at position (LSP required).

**Parameters**:
- `file_path` (string, required): File path
- `row` (integer, required): Line number (1-based)
- `column` (integer, required): Column number (1-based)

### get_completions

Get available completions at position (LSP required).

**Parameters**:
- `file_path` (string, required): File path
- `row` (integer, required): Line number (1-based)
- `column` (integer, required): Column number (1-based)
- `filter` (string, optional): Fuzzy search filter (recommended)
- `symbol_type` (string, optional): Filter by type
- `trigger_character` (string, optional): Trigger character (`.`, `::`, etc.)

### pattern_search

AST-based structural code search. Language-specific.

**Parameters**:
- `pattern` (string, required): AST pattern to match
- `language` (string, required): Programming language
- `file_path` (string, optional): Limit to specific file
- `limit` (integer, optional): Max results

**Metavariables**:
- `$VAR` - Matches single node (identifier, expression)
- `$$$` - Matches zero or more nodes (statements, parameters)

### pattern_rewrite

Automated code transformations using AST patterns.

**Parameters**:
- `pattern` (string, required): AST pattern to match
- `replacement` (string, required): Replacement pattern
- `language` (string, required): Programming language
- `file_path` (string, optional): Limit to specific file
- `limit` (integer, optional): Max files to modify
- `dry_run` (boolean, optional): Preview without applying (default true)

### generate_codebase_overview

Get high-level codebase structure overview.

**Parameters**:
- `path` (string, optional): Directory path (defaults to workspace root)

### search_codebase_map

Explore directory structure and code organization.

**Parameters**:
- `file_path` (string, optional): Focus on specific file
- `path` (string, optional): Focus on specific directory

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

**Supported Languages (Tree-sitter)**: Bash, C, C++, C#, Elixir, Go, Java, JavaScript, Kotlin, Lua, PHP, Python, Ruby, Rust, Scala, Swift, TSX, TypeScript

**LSP Servers**:
- TypeScript/JavaScript: typescript-language-server
- Rust: rust-analyzer
- Python: pyright
- Go: gopls
- Java: jdtls
- Ruby: solargraph
- C/C++: clangd
- Kotlin: kotlin-language-server

**Tree-sitter Operations** (no setup required): search_symbols, get_document_symbols, lookup_symbols, pattern_search, pattern_rewrite, generate_codebase_overview, search_codebase_map

**LSP Operations** (requires `/code init`): find_references, goto_definition, rename_symbol, get_diagnostics, get_hover, get_completions

**Initialization**: Run `/code init` in project root. Creates `lsp.json` config. Auto-initializes on subsequent startups.

**Permissions**: Trusted by default, no configuration needed.

**Position Format**: Row and column are 1-based (first line is 1, first column is 1).

**Symbol Types**: Function, Method, Class, Struct, Enum, Interface, Constant, Variable, Module, Import.
