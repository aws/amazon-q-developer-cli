---
doc_meta:
  validated: 2026-01-13
  commit: be9ce792
  status: validated
  testable_headless: false
  category: slash_command
  title: /code
  description: Manage code intelligence with init, status, logs, overview, and summary subcommands
  keywords: [code, lsp, intelligence, init, status, logs, overview, summary, documentation]
  related: [code-tool, enable-code-intelligence]
---

# /code

Manage code intelligence with init, status, logs, overview, and summary subcommands.

## Overview

Manages code intelligence. Initialize LSP workspace, check server status, view logs, and get codebase overviews.

## Usage

```
/code init
/code init -f
/code status
/code logs
/code overview
/code summary
```

## Subcommands

### init

Initialize code intelligence in workspace.

```
/code init
```

Detects languages, creates lsp.json, starts servers.

**Force restart**:
```
/code init -f
```

### status

Show workspace and LSP server status.

```
/code status
```

### logs

Display LSP logs.

```
/code logs
/code logs -l INFO -n 50
/code logs -p ./lsp-logs.json
```

**Options**:
- `-l, --level <LEVEL>`: Log level (ERROR, WARN, INFO, DEBUG, TRACE). Default: ERROR
- `-n, --lines <N>`: Number of lines. Default: 20
- `-p, --path <PATH>`: Export to JSON file

### overview

Get a high-level overview of the codebase structure.

```
/code overview
/code overview --silent
```

**Options**:
- `--silent`: Cleaner output for deep dives

Ideal for:
- Onboarding to new codebases
- Q&A sessions about project structure
- Understanding unfamiliar packages quickly

### summary

Generate comprehensive codebase documentation using agentic analysis.

```
/code summary
```

Starts an interactive session that:
1. Generates a codebase overview
2. Asks for documentation parameters (output directory, consolidation options, etc.)
3. Creates structured documentation files including architecture, components, interfaces, and workflows

Ideal for:
- Creating AI-friendly documentation (AGENTS.md)
- Generating README.md or CONTRIBUTING.md
- Building comprehensive knowledge bases for codebases

## Examples

### Example 1: Initialize

```
/code init
```

**Output**:
```
✓ Workspace initialization started

Detected Languages: ["rust", "typescript"]
✓ rust-analyzer (rust) - initialized (488ms)
✓ typescript-language-server (typescript) - initialized (214ms)
```

### Example 2: Check Status

```
/code status
```

### Example 3: View Errors

```
/code logs -l ERROR -n 50
```

### Example 4: Codebase Overview

```
/code overview
```

**Output**:
```
Codebase Overview: /path/to/project

Languages: TypeScript, Rust
Entry Points: src/main.ts, src/lib.rs
Key Directories:
  - src/components (42 files)
  - src/utils (15 files)
  - tests (28 files)
```

### Example 5: Generate Documentation

```
/code summary
```

**Output**:
```
✓ Overview generated (~2500 tokens) in 1.2s

I'll help you create comprehensive documentation. Please provide:
1. Output directory (default: .agents/summary)
2. Consolidate into single file? (a) Yes (b) No
3. Target file if consolidating: (a) AGENTS.md (b) README.md (c) CONTRIBUTING.md

(Reply with your choices, e.g., '1=default, 2=a, 3=a')
```

## Related

- [code](../tools/code.md) - Code intelligence tool
- [chat.enableCodeIntelligence](../settings/enable-code-intelligence.md) - Enable setting

## Technical Details

**Config**: Creates `.kiro/lsp.json` in workspace.

**Auto-init**: Automatically initializes on startup if lsp.json exists.

## Troubleshooting

### Issue: "Workspace is still initializing"

**Symptom**: Commands fail with initialization message  
**Cause**: LSP servers starting up  
**Solution**: Wait a moment and retry. If persists, use `/code init -f`

### Issue: Language Server Not Starting

**Symptom**: Server shows "not initialized" in status  
**Cause**: Language server not installed  
**Solution**: Install required language server (see code tool docs for install commands)

### Issue: Slow Initialization

**Symptom**: Init takes very long  
**Cause**: Large codebase indexing  
**Solution**: Wait for initial indexing. Subsequent operations will be faster.

### Issue: LSP Logs Show Errors

**Symptom**: Errors in `/code logs`  
**Cause**: LSP server errors or incompatibility  
**Solution**: Check server version. Try `/code init -f` to restart.
