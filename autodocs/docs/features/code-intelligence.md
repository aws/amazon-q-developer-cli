---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: false
  category: feature
  title: Code Intelligence
  description: Code understanding with tree-sitter (built-in) and LSP integration (optional) for symbol search, pattern matching, and codebase exploration
  keywords: [code, intelligence, lsp, semantic, symbols, references, tree-sitter, pattern, ast, codebase-overview]
  related: [code-tool, slash-code, enable-code-intelligence]
---

# Code Intelligence

Code Intelligence provides two complementary layers of code understanding:

**Tree-sitter (Built-in)** - Out-of-the-box code intelligence for 18 languages. Search symbols with fuzzy matching, get document symbols, and lookup definitions without installing an LSP. With incremental loading and support for millions of tokens of indexed content, agents can efficiently search large codebases.

**LSP Integration (Optional)** - Enhanced precision with find references, go to definition, hover documentation, rename refactoring, and diagnostics. Requires language server installation.

## Supported Languages

Bash, C, C++, C#, Elixir, Go, Java, JavaScript, Kotlin, Lua, PHP, Python, Ruby, Rust, Scala, Swift, TSX, TypeScript

## Overview

Code Intelligence provides these operations (no LSP required):

- **search_symbols**: Find functions, classes, methods by name (fuzzy matching)
- **get_document_symbols**: List all symbols in a file
- **lookup_symbols**: Look up specific symbols by exact name
- **pattern_search**: AST-based structural code search
- **pattern_rewrite**: Automated code transformations using AST patterns
- **generate_codebase_overview**: High-level codebase structure overview
- **search_codebase_map**: Explore directory structure and understand code organization

With LSP enabled (optional), additional operations become available:

- **find_references**: Locate all usages of a symbol at a position
- **goto_definition**: Navigate to where a symbol is defined
- **rename_symbol**: Rename symbols across the codebase
- **get_diagnostics**: Get errors and warnings for a file
- **get_hover**: Get type information and documentation at position
- **get_completions**: Get completion suggestions at position

## Codebase Overview

Get a complete overview of any workspace in seconds:

```
/code overview
```

Specify a path to focus on a specific directory:

```
/code overview ./src/components
```

Use `--silent` for a cleaner output when diving deep into a package:

```
/code overview --silent
```

Ideal for:
- Onboarding to new codebases
- Q&A sessions about project structure
- Understanding unfamiliar packages quickly

## Codebase Summary

Generate comprehensive documentation for your codebase:

```
/code summary
```

Starts an interactive session that analyzes the codebase and creates structured documentation including architecture, components, interfaces, and workflows. Can consolidate into AGENTS.md, README.md, or CONTRIBUTING.md.

## Pattern Search & Rewrite

AST-based structural code search and transformation. Find and modify code by structure, not just text.

### Metavariables

- `$VAR` - Matches single node (identifier, expression)
- `$$$` - Matches zero or more nodes (statements, parameters)

### Pattern Search Examples

```
# Find all console.log calls
pattern: console.log($ARG)
language: javascript

# Find all async functions
pattern: async function $NAME($$$PARAMS) { $$$ }
language: typescript

# Find all .unwrap() calls
pattern: $E.unwrap()
language: rust
```

### Pattern Rewrite Examples

```
# Convert var to const
pattern: var $N = $V
replacement: const $N = $V
language: javascript

# Modernize hasOwnProperty
pattern: $O.hasOwnProperty($P)
replacement: Object.hasOwn($O, $P)
language: javascript

# Convert unwrap to expect
pattern: $E.unwrap()
replacement: $E.expect("unexpected None")
language: rust
```

### Rewrite Workflow

1. Use `pattern_search` first to verify matches
2. Review matches to ensure correctness
3. Run `pattern_rewrite` with `dry_run: true` to preview
4. Apply changes with `dry_run: false`

## LSP Integration (Optional)

Run `/code init` to unlock full LSP-powered code intelligence with enhanced features like find references, hover documentation, and rename refactoring.

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

**Kotlin**
```bash
# Download from https://github.com/fwcd/kotlin-language-server
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

### pattern_search

AST-based structural code search. Find code by structure, not just text. Language-specific.

**Parameters:**
- `pattern` (required): AST pattern to match
- `language` (required): Programming language
- `file_path`: Limit search to specific file
- `limit`: Max results

**Example queries:**
```
> Find all console.log calls in JavaScript files
> Search for async functions in TypeScript
> Find all .unwrap() calls in Rust
```

### pattern_rewrite

Automated code transformations using AST patterns.

**Parameters:**
- `pattern` (required): AST pattern to match
- `replacement` (required): Replacement pattern
- `language` (required): Programming language
- `file_path`: Limit to specific file
- `limit`: Max files to modify
- `dry_run`: Preview changes without applying (default true)

**Example queries:**
```
> Replace all var declarations with const in JavaScript
> Convert .unwrap() to .expect() in Rust files
> Dry run: replace console.log with logger.debug
```

### generate_codebase_overview

Get high-level codebase structure overview.

**Parameters:**
- `path`: Directory path (optional, defaults to workspace root)

**Example queries:**
```
> Give me an overview of this codebase
> What's the structure of the src directory?
```

### search_codebase_map

Explore directory structure and understand code organization.

**Parameters:**
- `file_path`: Focus on a specific file
- `path`: Focus on a specific directory path

**Example queries:**
```
> Show me the structure of the src/api directory
> What's in the components folder?
> Explore the tests directory
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

## Supported LSP Servers

The following LSP servers are pre-configured and auto-detected:

| Language | Extensions | Server | Install Command |
|----------|------------|--------|-----------------|
| TypeScript/JavaScript | `.ts`, `.js`, `.tsx`, `.jsx` | `typescript-language-server` | `npm install -g typescript-language-server typescript` |
| Rust | `.rs` | `rust-analyzer` | `rustup component add rust-analyzer` |
| Python | `.py` | `pyright` | `pip install pyright` |
| Go | `.go` | `gopls` | `go install golang.org/x/tools/gopls@latest` |
| Java | `.java` | `jdtls` | `brew install jdtls` (macOS) |
| Kotlin | `.kt`, `.kts` | `kotlin-lsp` | See [kotlin-lsp](https://github.com/fwcd/kotlin-language-server) |
| Ruby | `.rb` | `solargraph` | `gem install solargraph` |
| C/C++ | `.c`, `.cpp`, `.h`, `.hpp` | `clangd` | `brew install llvm` (macOS) or `apt install clangd` (Linux) |

Additional languages can be added via custom `lsp.json` configuration (see Custom Language Servers section).

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
