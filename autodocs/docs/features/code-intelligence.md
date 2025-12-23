---
doc_meta:
  validated: 2025-12-22
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: feature
  title: Code Intelligence
  description: LSP-powered semantic code understanding with symbol search, references, definitions, and diagnostics
  keywords: [code, intelligence, lsp, semantic, symbols, references]
  related: [code-tool, slash-code, enable-code-intelligence]
---

# Code Intelligence

Code Intelligence gives Kiro CLI semantic understanding of your codebase through Language Server Protocol (LSP) integration. Search symbols, find references, go to definitions, and get diagnostics just like your IDE.

## Overview

Code Intelligence provides these LSP-powered operations:

- **search_symbols**: Find functions, classes, methods by name
- **find_references**: Locate all usages of a symbol at a position
- **goto_definition**: Navigate to where a symbol is defined
- **get_document_symbols**: List all symbols in a file
- **lookup_symbols**: Look up specific symbols by exact name
- **rename_symbol**: Rename symbols across the codebase
- **get_diagnostics**: Get errors and warnings for a file
- **initialize_workspace**: Initialize LSP servers

## Onboarding

### Prerequisites

Install language servers for your languages:

**TypeScript/JavaScript**
```bash
npm install -g typescript-language-server typescript
```

**Rust**
```bash
rustup component add rust-analyzer
```

**Python**
```bash
pip install pyright
# or with pipx (recommended for isolation)
pipx install pyright
```

**Go**
```bash
go install golang.org/x/tools/gopls@latest
```

**Java**
```bash
# macOS
brew install jdtls

# Linux - download from https://download.eclipse.org/jdtls/snapshots/
# Extract and add to PATH
```

**Ruby**
```bash
gem install solargraph
```

**C/C++**
```bash
# macOS
brew install llvm
# or
brew install clangd

# Linux (Debian/Ubuntu)
sudo apt install clangd

# Linux (Arch)
sudo pacman -S clang
```

### Initialize Code Intelligence

Run this slash command in your project root:

```
/code init
```

This creates `lsp.json` configuration and starts language servers.

**What you'll see:**
```
✓ Workspace initialization started

Workspace: /path/to/your/project
Detected Languages: ["python", "rust", "typescript"]
Project Markers: ["Cargo.toml", "package.json"]

Available LSPs:
○ clangd (cpp) - available
○ gopls (go) - not installed
◐ jdtls (java) - initializing...
✓ pyright (python) - initialized (687ms)
✓ rust-analyzer (rust) - initialized (488ms)
○ solargraph (ruby) - not installed
✓ typescript-language-server (typescript) - initialized (214ms)
```

**Status indicators:**
- `✓` - Initialized and ready
- `◐` - Currently initializing
- `○ available` - Installed but not needed for detected languages
- `○ not installed` - Not installed on your system

**Restart LSP servers:**
If language servers shut down or become unresponsive, use `/code init -f`.

**Auto-initialization:**
After the first `/code init`, KIRO CLI automatically initializes code intelligence on startup when `lsp.json` exists in the workspace.

**Disabling code intelligence:**
Delete `lsp.json` from your project root to disable. Re-enable anytime with `/code init`.

## Operations

### search_symbols

Search for symbols by name across the workspace.

**Parameters:**
- `symbol_name` (required): Name to search for
- `file_path`: Limit search to specific file
- `symbol_type`: Filter by type (function, class, etc.)
- `limit`: Max results (default 50)
- `language`: Filter by language
- `exact_match`: Require exact name match

**Example queries:**
```
> Find the UserService class
> Search for functions named "validate"
> Find all classes in auth.ts
```

### find_references

Find all references to a symbol at a specific position.

**Parameters:**
- `file_path` (required): File containing the symbol
- `row` (required): Line number (1-based)
- `column` (required): Column number (1-based)

**Example queries:**
```
> Find all references to the symbol at line 42, column 10 in user.ts
> Where is the function at auth.rs:15:5 used?
```

### goto_definition

Navigate to where a symbol is defined.

**Parameters:**
- `file_path` (required): File containing the symbol
- `row` (required): Line number (1-based)
- `column` (required): Column number (1-based)
- `show_source`: Include source code in result (default true)

**Example queries:**
```
> Go to definition of symbol at main.ts:25:12
> Where is the symbol at line 100, column 5 in handler.rs defined?
```

### get_document_symbols

Get all symbols defined in a file.

**Parameters:**
- `file_path` (required): File to analyze
- `top_level_only`: Only return top-level symbols

**Example queries:**
```
> What symbols are in auth.service.ts?
> Show me all functions in utils.py
> List the classes in models.rs
```

### lookup_symbols

Look up specific symbols by exact name.

**Parameters:**
- `symbols` (required): List of symbol names to find
- `file_path`: Limit search to specific file

**Example queries:**
```
> Find the symbols named "processOrder" and "validateInput"
> Look up UserModel and AuthService
```

### rename_symbol

Rename a symbol across the entire codebase.

**Parameters:**
- `file_path` (required): File containing the symbol
- `row` (required): Line number (1-based)
- `column` (required): Column number (1-based)
- `new_name` (required): New name for the symbol
- `dry_run`: Preview changes without applying (default false)

**Example queries:**
```
> Rename the symbol at user.ts:42:10 to "fetchUserData"
> Dry run: rename symbol at line 15 column 5 in auth.rs to "authenticateUser"
```

### get_diagnostics

Get errors, warnings, and hints for a file.

**Parameters:**
- `file_path` (required): File to check
- `identifier`: Optional diagnostic identifier
- `previous_result_id`: For incremental diagnostics

**Example queries:**
```
> What errors are in main.ts?
> Get diagnostics for auth.rs
> Check handler.py for problems
```

## Usage Examples

### Example 1: Find a Symbol

```
> Find the UserRepository class

Searching for symbols matching: "UserRepository"

  1. Class UserRepository at src/repositories/user.repository.ts:15:1
```

### Example 2: Find All References

```
> Find references to symbol at auth.ts line 42 column 10

Finding all references at: auth.ts:42:10

  1. src/auth.ts:42:10 - export function authenticate(...)
  2. src/handlers/login.ts:15:5 - authenticate(credentials)
  3. src/handlers/api.ts:89:12 - await authenticate(token)
  (3 more items found)
```

### Example 3: Go to Definition

```
> Find the definition of UserService

src/services/user.service.ts:42:1: export class UserService { ...
```

### Example 4: Get File Symbols

```
> What symbols are in auth.service.ts?

Getting symbols from: auth.service.ts

  1. Class AuthService at auth.service.ts:12:1
  2. Function login at auth.service.ts:25:3
  3. Function logout at auth.service.ts:45:3
  4. Function validateToken at auth.service.ts:62:3
```

### Example 5: Rename with Dry Run

```
> Dry run: rename symbol at user.ts:42:10 to "fetchUserData"

Dry run: Would rename 12 occurrences in 5 files
```

### Example 6: Get Diagnostics

```
> Get diagnostics for main.ts

  1. Error line 15:10: Cannot find name 'undefined_var'
  2. Warning line 42:5: 'result' is declared but never used
```

## Custom Language Servers

Add custom language servers by editing `lsp.json` in your project root:

```json
{
  "languages": {
    "mylang": {
      "name": "my-language-server",
      "command": "my-lsp-binary",
      "args": ["--stdio"],
      "file_extensions": ["mylang", "ml"],
      "project_patterns": ["mylang.config"],
      "exclude_patterns": ["**/build/**"],
      "multi_workspace": false,
      "initialization_options": {
        "custom": "options"
      },
      "request_timeout_secs": 60
    }
  }
}
```

**Fields:**
- `name`: Display name for the language server
- `command`: Binary/command to execute
- `args`: Command line arguments (usually `["--stdio"]`)
- `file_extensions`: File extensions this server handles
- `project_patterns`: Files that indicate a project root (e.g., `package.json`)
- `exclude_patterns`: Glob patterns to exclude from analysis
- `multi_workspace`: Set to `true` if the LSP supports multiple workspace folders (default: `false`)
- `initialization_options`: LSP-specific configuration passed during initialization
- `request_timeout_secs`: Timeout in seconds for LSP requests (default: `60`)

After editing, restart KIRO CLI to load the new configuration.

## Slash Commands

### `/code init`
Initialize code intelligence in current directory.

### `/code init -f`
Force re-initialization (restart all LSP servers).

### `/code status`
Show workspace status and LSP server states.

### `/code logs`
Display LSP logs for troubleshooting.
```
/code logs                    # Show last 20 ERROR logs
/code logs -l INFO            # Show INFO level and above
/code logs -n 50              # Show last 50 entries
/code logs -l DEBUG -n 100    # Show last 100 DEBUG+ logs
/code logs -p ./lsp-logs.json # Export logs to JSON file
```

**Options:**
- `-l, --level <LEVEL>`: Log level filter (ERROR, WARN, INFO, DEBUG, TRACE). Default: ERROR
- `-n, --lines <N>`: Number of log lines to display. Default: 20
- `-p, --path <PATH>`: Export logs to JSON file

## Supported Languages

| Language | Extensions | Server | Install Command |
|----------|------------|--------|-----------------|
| TypeScript/JavaScript | `.ts`, `.js`, `.tsx`, `.jsx` | `typescript-language-server` | `npm install -g typescript-language-server typescript` |
| Rust | `.rs` | `rust-analyzer` | `rustup component add rust-analyzer` |
| Python | `.py` | `pyright` | `pip install pyright` |
| Go | `.go` | `gopls` | `go install golang.org/x/tools/gopls@latest` |
| Java | `.java` | `jdtls` | `brew install jdtls` (macOS) |
| Ruby | `.rb` | `solargraph` | `gem install solargraph` |
| C/C++ | `.c`, `.cpp`, `.h`, `.hpp` | `clangd` | `brew install llvm` (macOS) or `apt install clangd` (Linux) |

### "Code tool is not enabled for this agent"
**Cause**: The agent you're using doesn't have the `code` tool in its tool list.
**Solution**: Add the code tool to your agent configuration:
- Add `"code"` to the agent's tools array, or
- Use `@builtin` to include all built-in tools, or
- Use `@builtin/code` to include only the code tool

**Example agent configuration:**
```json
{
  "tools": ["@builtin/code", "other_tool"]
}
```

### "Workspace is still initializing"
**Cause**: LSP servers are starting up.
**Solution**: Wait a moment and try again. If servers crashed, use `/code init -f` to restart.

### LSP initialization failed
**Solution**: Check logs for details:
```
/code logs -l ERROR
```

### "No symbols found"
**Causes**:
- Language server still indexing
- File has syntax errors
- Symbol name doesn't match

**Solution**: Check file for errors, try broader search terms.

### "No definition found"
**Cause**: Position doesn't point to a symbol.
**Solution**: Verify the row and column numbers point to a symbol name.

## Best Practices

1. **Initialize once per project** - Run `/code init` in project root
2. **Use exact positions** - Row and column must point to the symbol
3. **Use dry_run for renames** - Preview changes before applying
4. **Check diagnostics first** - Syntax errors can prevent analysis
5. **Be specific in searches** - "UserService" > "user"

## Limitations

- **LSP feature support** varies by language server - not all servers support every operation (e.g., some may not support rename or formatting)
- **Large codebases** may have slow initial indexing

## Related Features

- **File Operations**: See [Built-in Tools](built-in-tools.md) for fs_read/fs_write
- **Agent Configuration**: See [Agent Format](agent-format.md) for permanent trust
