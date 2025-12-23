---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /logdump
  description: Create zip file with diagnostic logs for support investigation
  keywords: [logdump, logs, debug, support, zip]
  related: [diagnostic]
---

# /logdump

Create zip file with diagnostic logs for support investigation.

## Overview

The `/logdump` command creates a zip file containing diagnostic logs, configuration, and session information. Useful for sharing with support when troubleshooting issues.

## Usage

```
/logdump
```

Creates zip file with chat logs.

### With MCP Logs

```
/logdump --mcp
```

Includes MCP server logs in addition to chat logs.

## Options

| Option | Description |
|--------|-------------|
| `--mcp` | Include MCP logs in the archive |

## Output

Zip file created in current directory with name `q-logs-{timestamp}.zip`.

**Contains**:
- `logs/kiro-chat.log` - Main chat logs
- `logs/mcp.log` - MCP server logs (if --mcp flag used)

## Examples

### Example 1: Create Log Archive

```
/logdump
```

**Output**:
```
Collecting logs...
✓ Successfully created q-logs-2025-12-22T12-00-00Z.zip with 1 log files
```

### Example 2: Include MCP Logs

```
/logdump --mcp
```

**Output**:
```
Collecting logs...
✓ Successfully created q-logs-2025-12-22T12-00-00Z.zip with 2 log files
```

## What's Included

- **kiro-chat.log**: Main application logs (always included)
- **mcp.log**: MCP server logs (only with --mcp flag)

## Troubleshooting

### Issue: Can't Create Zip

**Symptom**: Error creating archive  
**Cause**: Permission or disk space issue  
**Solution**: Check disk space and write permissions

## Related Features

- [kiro-cli diagnostic](../commands/diagnostic.md) - Run diagnostics

## Limitations

- Logs may contain sensitive paths
- Large log files create large zips
- No automatic upload to support

## Technical Details

**Location**: Creates zip in logs directory

**Contents**: Logs, config, diagnostics

**Privacy**: Sanitizes sensitive information
