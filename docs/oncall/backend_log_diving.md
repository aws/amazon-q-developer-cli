---
name: backend-log-diving
description: Guide for searching Maestro and MPS backend logs via CloudWatch Logs Insights. Use when tracing request IDs, investigating backend errors, or root-causing ValidationException/ContextWindowOverflow issues.
---

# Backend Log Diving

## Overview

When investigating backend errors for Kiro CLI, there are downstream services with their own logs:

```
Maestro → MPS → Bedrock
```

These services process CLI requests. Each has separate CloudWatch log groups in separate AWS accounts. Request IDs from the CLI appear in Maestro logs but do **not** correlate to MPS — you need to use timestamps and narrow time windows to trace a request from Maestro to MPS.

## Accounts and Log Groups

### Maestro (Java + Python Sidecar)

- **Account:** `767828757792`
- **Role:** `ReadOnly`
- **Region:** `us-east-1`
- **Log groups:**
  - `QDeveloperMaestro-prod-ApplicationLogs` — Java application logs — request lifecycle, errors, routing
  - `QDeveloperMaestro-prod-SidecarLogs` — Python sidecar logs (limited — may not contain request IDs)
  - `QDeveloperMaestro-prod-RequestLogs` — Request-level logs
  - `QDeveloperMaestro-OnePod-prod-SidecarLogs` — OnePod sidecar logs

When querying Maestro by request ID, include all log groups to avoid missing entries:

```bash
--log-group-name 'QDeveloperMaestro-prod-ApplicationLogs' \
--log-group-name 'QDeveloperMaestro-prod-SidecarLogs' \
--log-group-name 'QDeveloperMaestro-prod-RequestLogs' \
--log-group-name 'QDeveloperMaestro-OnePod-prod-SidecarLogs' \
```

### MPS (Model Proxy Service)

- **Account:** `344555145015`
- **Role:** `ReadOnly`
- **Region:** `us-east-1`
- **Log groups:**
  - `AWSVectorConsolasModelProxyService-prod-ApplicationLogs` — MPS application logs — Bedrock calls, error details, model IDs
  - `AWSVectorConsolasModelProxyService-prod-RequestLogs` — MPS request-level logs

## Querying Logs

All queries use `ada` for credentials and `aws logs` CLI for CloudWatch Logs Insights.

### Basic Pattern

```bash
# 1. Start the query
env $(ada cred print --account <ACCOUNT> --role ReadOnly --format env) \
  aws logs start-query \
  --log-group-name '<LOG_GROUP>' \
  --start-time <EPOCH_START> \
  --end-time <EPOCH_END> \
  --query-string '<QUERY>' \
  --region us-east-1 \
  --output json

# 2. Poll for results (queries can take 15-60+ seconds depending on time range)
env $(ada cred print --account <ACCOUNT> --role ReadOnly --format env) \
  aws logs get-query-results \
  --query-id "<QUERY_ID>" \
  --region us-east-1 \
  --output json
```

### Converting Timestamps

CloudWatch `--start-time` and `--end-time` expect Unix epoch seconds (UTC). On macOS, `date -j` interprets input as local time by default — use `TZ=UTC` to convert UTC timestamps:

```bash
# UTC timestamp to epoch (correct)
TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-09T16:25:00" +%s

# PST to UTC: add 8 hours (PST) or 7 hours (PDT)
# Feb 9 8:25 AM PST = Feb 9 16:25 UTC
TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-09T16:25:00" +%s
```

## Common Queries

### Search Maestro logs by request ID

When the CLI encounters an error, it prints a request ID. This same request ID appears in Maestro logs.

```bash
env $(ada cred print --account 767828757792 --role ReadOnly --format env) \
  aws logs start-query \
  --log-group-name 'QDeveloperMaestro-prod-ApplicationLogs' \
  --log-group-name 'QDeveloperMaestro-prod-SidecarLogs' \
  --log-group-name 'QDeveloperMaestro-prod-RequestLogs' \
  --log-group-name 'QDeveloperMaestro-OnePod-prod-SidecarLogs' \
  --start-time <EPOCH_START> \
  --end-time <EPOCH_END> \
  --query-string 'fields message, @timestamp, @message, @logStream
    | filter @message like "<REQUEST_ID>"
    | sort @timestamp desc
    | limit 10000' \
  --region us-east-1 --output json
```

### Search MPS logs for the actual Bedrock error

When Maestro returns a generic `"Request input fails validation"` or other `ValidationException`, MPS logs contain the actual Bedrock error message. Use a narrow time window based on the Maestro error timestamp (MPS errors typically appear 100-500ms before the corresponding Maestro error).

```bash
env $(ada cred print --account 344555145015 --role ReadOnly --format env) \
  aws logs start-query \
  --log-group-name 'AWSVectorConsolasModelProxyService-prod-ApplicationLogs' \
  --start-time <EPOCH_START> \
  --end-time <EPOCH_END> \
  --query-string 'fields @timestamp, @message
    | filter @message like "ValidationException"
    | sort @timestamp asc
    | limit 100' \
  --region us-east-1 --output json
```

Replace `"ValidationException"` with whatever error pattern you're looking for. Common patterns:
- `"ValidationException"` — any validation error from Bedrock
- `"prompt is too long"` — context window overflow (token count exceeded)
- `"too many total text bytes"` — payload size exceeded
- `"Retries exhausted"` — MPS gave up retrying Bedrock

### Search MPS logs by MPS request ID

If you have an MPS request ID (from MPS RequestLogs or other sources):

```bash
env $(ada cred print --account 344555145015 --role ReadOnly --format env) \
  aws logs start-query \
  --log-group-name 'AWSVectorConsolasModelProxyService-prod-ApplicationLogs' \
  --start-time <EPOCH_START> \
  --end-time <EPOCH_END> \
  --query-string 'fields @timestamp, @message
    | filter @message like "<MPS_REQUEST_ID>"
    | sort @timestamp desc
    | limit 10000' \
  --region us-east-1 --output json
```

## Tracing a Request Across Services

The CLI request ID appears in Maestro logs but does **not** appear in MPS logs. To trace from Maestro to MPS:

1. **Start with the CLI request ID** — search Maestro logs to find the error timestamp and error type
2. **Narrow the time window** — use the Maestro error timestamp +- a few seconds
3. **Search MPS logs in that window** — filter by error message pattern (e.g., `"ValidationException"`)
4. **Correlate by timestamp** — MPS errors will typically appear 100-500ms before the corresponding Maestro error (network round-trip)

### Example: Tracing a ValidationException

```bash
# Step 1: Find the request in Maestro (use a wide window if unsure of exact time)
env $(ada cred print --account 767828757792 --role ReadOnly --format env) \
  aws logs start-query \
  --log-group-name 'QDeveloperMaestro-prod-ApplicationLogs' \
  --log-group-name 'QDeveloperMaestro-prod-SidecarLogs' \
  --log-group-name 'QDeveloperMaestro-prod-RequestLogs' \
  --log-group-name 'QDeveloperMaestro-OnePod-prod-SidecarLogs' \
  --start-time $(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-09T16:00:00" +%s) \
  --end-time $(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-09T17:00:00" +%s) \
  --query-string 'fields message, @timestamp, @message
    | filter @message like "<REQUEST_ID>"
    | sort @timestamp desc
    | limit 10000' \
  --region us-east-1 --output json

# Step 2: From results, note the error timestamp (e.g., 16:25:16.991)
# Look for "Got prestream error from Sidecar" — this is the sidecar returning an error

# Step 3: Search MPS in a narrow window around that timestamp for the actual Bedrock error
env $(ada cred print --account 344555145015 --role ReadOnly --format env) \
  aws logs start-query \
  --log-group-name 'AWSVectorConsolasModelProxyService-prod-ApplicationLogs' \
  --start-time $(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-09T16:25:10" +%s) \
  --end-time $(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "2026-02-09T16:25:20" +%s) \
  --query-string 'fields @timestamp, @message
    | filter @message like "ValidationException"
    | sort @timestamp asc
    | limit 100' \
  --region us-east-1 --output json
```

## Known Error Patterns

### ValidationException

This is a **generic catch-all** from the Maestro Python sidecar. It means Bedrock returned a `ValidationException` whose message did not match any of the known "input too long" patterns. The actual Bedrock error reason is only available in MPS logs — use the tracing pattern above to find it.

## Tips

- **Wide time ranges are slow.** A full-day query on Maestro ApplicationLogs scans 7-10 billion records. Start with a narrow window (minutes) and widen if needed.
- **Sidecar logs are sparse.** The SidecarLogs and OnePod-SidecarLogs groups may not contain request IDs. Including all log groups in your query ensures you don't miss entries.
- **CLI request IDs don't appear in MPS.** Use timestamps and error message patterns to correlate across Maestro and MPS.
- **Poll for results.** `start-query` returns immediately with a query ID. Use `get-query-results` to poll — check the `status` field (`Running` vs `Complete`). Large queries can take 30-90 seconds.
- **Log retention.** Logs may be rotated after ~30-90 days depending on the account configuration. If you can't find old logs, they may have aged out.
