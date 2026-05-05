---
doc_meta:
  validated: 2026-05-05
  commit: 464ada50
  status: validated
  testable_headless: false
  category: slash_command
  title: /stats
  description: Show request IDs and timings for debugging slow turns
  keywords: [stats, debug, timing, latency, request-id, performance, slow, ttfc]
  related: [usage, logdump]
---

# /stats

Show request IDs and timings for debugging slow turns.

## Overview

The `/stats` command displays an in-memory log of recent API requests with their request IDs, durations, time-to-first-chunk (TTFC), token counts, and error status. This is useful for debugging slow responses or reporting issues to support.

This command is hidden from the autocomplete menu but remains fully functional when typed directly.

## Usage

```
/stats
/stats N
/stats save <filename>
```

## Subcommands

### (no subcommand)

Display all recorded requests in a panel.

```
/stats
```

Shows a table with:
- Request ID (for support tickets)
- Duration (total request time)
- TTFC (time to first chunk)
- Input/output token counts
- Status (ok, ok with tool_use, or error)

### N (number)

Show only the last N requests.

```
/stats 5
```

Filters the display to the most recent N requests.

### save

Export all recorded stats to a JSON file.

```
/stats save
/stats save <filename>
```

Default filename is `stats.json` in the current working directory.

## Output

The stats panel displays:

| Column | Description |
|--------|-------------|
| # | Request number (1-indexed) |
| Request ID | Unique identifier for support |
| Duration | Total request time in ms |
| TTFC | Time to first chunk in ms |
| In | Input tokens |
| Out | Output tokens |
| Status | ok, ok (tool_use), or ERR: message |

**Summary footer** shows aggregate metrics:
- `avg` - Average duration
- `p90` - 90th percentile duration
- `max` - Maximum duration
- `errors` - Count of failed requests

## Examples

### Example 1: View All Stats

```
/stats
```

**Output** (panel):
```
/stats · 3 requests

#     Request ID                               Duration    TTFC      In        Out       Status
▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔
1     abc123-def456-789                        1523ms      342ms     1234      5678      ok
2     xyz789-abc012-345                        2891ms      521ms     2345      8901      ok (tool_use)
3     mno456-pqr789-012                        892ms       198ms     567       1234      ok

avg=1769ms  p90=2891ms  max=2891ms  errors=0
```

### Example 2: View Last 5 Requests

```
/stats 5
```

Shows only the 5 most recent requests.

### Example 3: Export to File

```
/stats save debug-stats.json
```

**Output**:
```
Saved 3 records to /home/user/project/debug-stats.json
```

### Example 4: Export with Default Filename

```
/stats save
```

**Output**:
```
Saved 3 records to /home/user/project/stats.json
```

## JSON Export Format

The `save` subcommand exports an array of request records:

```json
[
  {
    "request_id": "abc123-def456-789",
    "timestamp": "2026-05-05T18:30:00.000Z",
    "duration_ms": 1523.45,
    "ttfc_ms": 342.12,
    "input_tokens": 1234,
    "output_tokens": 5678,
    "status_code": 200,
    "had_tool_use": false,
    "error": null
  }
]
```

## Troubleshooting

### Issue: No Requests Recorded

**Symptom**: "No requests recorded yet"  
**Cause**: No AI responses have been generated in this session  
**Solution**: Send a message and wait for a response, then try `/stats` again

### Issue: Request ID is Null

**Symptom**: Request ID shows as `-`  
**Cause**: Request metadata not available from backend  
**Solution**: This is normal for some request types. Other fields should still be populated.

### Issue: Duration Seems Wrong

**Symptom**: Duration much longer than expected  
**Cause**: Duration includes full response streaming time  
**Solution**: Check TTFC for time to first token. Long durations with short TTFC indicate large responses.

### Issue: Save Failed

**Symptom**: "Failed to write" error  
**Cause**: Permission denied or invalid path  
**Solution**: Check write permissions. Use absolute path if needed.

## Related Features

- [/usage](usage.md) - View token usage and costs
- [/logdump](logdump.md) - Export full session logs

## Limitations

- Stores last 100 requests only (ring buffer)
- Stats are session-only (not persisted across restarts)
- Hidden from autocomplete (debugging tool)
- Request IDs depend on backend providing them

## Technical Details

**Storage**: In-memory ring buffer of 100 records.

**Timing**: Duration measured from request start to stream end. TTFC measured from request start to first chunk received.

**Tool Use**: Requests that included tool calls are marked with "ok (tool_use)" status.

**Highlighting**: Durations over 5000ms are highlighted in warning color.
