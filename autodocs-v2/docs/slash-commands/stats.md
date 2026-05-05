---
doc_meta:
  validated: 2026-05-05
  commit: 464ada50
  status: validated
  testable_headless: false
  category: slash_command
  title: /stats
  description: Show request IDs and timings for debugging slow turns
  keywords: [stats, performance, debugging, latency, request-id, timing]
  related: [usage, diagnostic]
---

# /stats

Show request IDs and timings for debugging slow turns.

## Overview

The `/stats` command displays an in-memory log of recent API requests with their request IDs, durations, token counts, and error status. This is useful for debugging slow responses or reporting issues to support with specific request IDs.

The command is hidden from the autocomplete menu but can be typed directly.

## Usage

```
/stats
/stats N
/stats save <filename>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `N` | Show only the last N requests (default: all) |
| `save <filename>` | Export stats to a JSON file (default: `stats.json`) |

## Output

Displays a panel with a table showing:
- **#** - Request number
- **Request ID** - Unique identifier for the API request
- **Duration** - Total request time in milliseconds
- **TTFC** - Time to first chunk (streaming latency)
- **In** - Input token count
- **Out** - Output token count
- **Status** - Success/error status and tool use indicator

Footer shows summary statistics:
- **avg** - Average duration
- **p90** - 90th percentile duration
- **max** - Maximum duration
- **errors** - Count of failed requests

## Examples

### Example 1: View All Stats

```
/stats
```

**Output**: Opens panel showing all recorded requests with timing data.

### Example 2: View Last 5 Requests

```
/stats 5
```

**Output**: Opens panel showing only the 5 most recent requests.

### Example 3: Export Stats to File

```
/stats save debug-session.json
```

**Output**:
```
Saved 12 records to /path/to/debug-session.json
```

### Example 4: Export with Default Filename

```
/stats save
```

**Output**:
```
Saved 12 records to /path/to/stats.json
```

## JSON Export Format

When using `/stats save`, the output file contains an array of request records:

```json
[
  {
    "request_id": "abc123-def456",
    "timestamp": "2026-05-05T18:30:00.000Z",
    "duration_ms": 2500.5,
    "ttfc_ms": 450.2,
    "input_tokens": 1500,
    "output_tokens": 800,
    "status_code": 200,
    "had_tool_use": true,
    "error": null
  }
]
```

## Use Cases

### Debugging Slow Responses

When a response feels slow, use `/stats` to see actual timing data:
- High `duration_ms` indicates slow overall response
- High `ttfc_ms` indicates slow initial response (network or queue delay)
- Compare with token counts to understand if slowness is due to large context

### Reporting Issues

When reporting performance issues to support:
1. Run `/stats save issue-report.json`
2. Include the JSON file with your report
3. Reference specific `request_id` values for investigation

### Monitoring Token Usage

Track token consumption across requests to understand context growth and optimize prompts.

## Troubleshooting

### Issue: No Requests Recorded

**Symptom**: Panel shows "No requests recorded yet"  
**Cause**: No API requests have been made in this session  
**Solution**: Send a message to generate at least one request

### Issue: Missing Request IDs

**Symptom**: Request ID shows as `-`  
**Cause**: Request ID not returned by API (rare)  
**Solution**: This is informational only; other metrics still available

### Issue: Save Failed

**Symptom**: Error writing file  
**Cause**: Invalid path or permission issue  
**Solution**: Use absolute path or ensure write permissions in current directory

## Limitations

- Stores last 100 requests only (ring buffer)
- Stats are session-only (not persisted across restarts)
- Hidden from autocomplete (type `/stats` directly)
- Panel view only available in TUI mode

## Technical Details

Request metadata is captured from `StreamMetadata` at the end of each response stream. The ring buffer holds up to 100 records, with oldest records dropped when full.

Timing metrics:
- `duration_ms` = `request_end_time - request_start_time`
- `ttfc_ms` = Time from request start to first response chunk
