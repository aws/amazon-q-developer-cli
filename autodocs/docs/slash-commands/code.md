---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /code
  description: Manage code intelligence with init, status, and logs subcommands
  keywords: [code, lsp, intelligence, init, status, logs]
  related: [code-tool, enable-code-intelligence]
---

# /code

Manage code intelligence with init, status, and logs subcommands.

## Overview

Manages LSP-based code intelligence. Initialize workspace, check server status, and view logs.

## Usage

```
/code init
/code init -f
/code status
/code logs
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
