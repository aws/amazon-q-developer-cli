---
description: Generate V2 (kiro-tui) adoption telemetry reports from Kibana/Elasticsearch
---

# V2 Adoption Telemetry Report

Generate V2 (kiro-tui) adoption reports by querying the telemetry Elasticsearch cluster via Kibana.

## Prerequisites

- Scripts: `scripts/es-query.sh`, `scripts/es-v2-report.py`
- Cookie file: `scripts/.es-cookie`

## Process

### 1. Check for a valid cookie (REQUIRED)

Check if `scripts/.es-cookie` exists and test it:

```bash
./scripts/es-query.sh 'metrics-*' '{"size":0,"query":{"match_all":{}}}'
```

If the response contains `"aggregations"` or `"hits"`, the cookie is valid. If it returns HTML, an error, or `Rate exceeded`, the cookie is expired.

**If expired, ask the user:**

```
The Kibana cookie has expired. Please:
1. Open https://telemetry-externalprod.ide-toolkits.dev-tools.aws.dev/_plugin/kibana/app/dev_tools#/console
2. Run any query (e.g. GET _search)
3. Open Chrome DevTools (Cmd+Option+I) → Network tab
4. Find the _search request → right-click → Copy as cURL
5. Paste the full curl command here
```

Once the user pastes the curl command, extract the cookie value from the `-b '...'` parameter and save it:

```bash
cat > scripts/.es-cookie << 'COOKIE'
<extracted cookie value>
COOKIE
```

### 2. Ask what report the user wants

Ask the user:

```
What date range do you want? And do you want:
1. All versions (default)
2. Filter to a minimum version (e.g. --min-version 1.29.3)
```

### 3. Run the report

```bash
# All versions
python3 scripts/es-v2-report.py <start-date> <end-date>

# With version filter
python3 scripts/es-v2-report.py <start-date> <end-date> --min-version <version>

# Last N days
python3 scripts/es-v2-report.py <N>
```

Output is written to `v2-adoption-report.md`.

### 4. Present the results

- Show the adoption % table inline
- Highlight key trends (growth, drops, inflection points)
- Note any anomalies (weekend dips, release bumps)

### 5. Ad-hoc queries

If the user wants custom queries, use `es-query.sh` directly:

```bash
./scripts/es-query.sh '<index-pattern>' '<query-json>'
```

**Index pattern**: `metrics-YYYY-MM-DD` for a single day, `metrics-2026-04-*` for a month.

**Common queries:**

Unique V2 users on a day:
```bash
./scripts/es-query.sh 'metrics-2026-04-09' '{
  "size": 0,
  "query": {
    "bool": {
      "must": [
        { "match_phrase": { "product": "CodeWhisperer for Terminal" } },
        { "match": { "metadata.kirocli_appType": "V2" } }
      ]
    }
  },
  "aggs": { "unique_users": { "cardinality": { "field": "clientId" } } }
}'
```

V1 vs V2 vs ACP breakdown:
```bash
./scripts/es-query.sh 'metrics-2026-04-09' '{
  "size": 0,
  "query": {
    "bool": {
      "must": [
        { "match_phrase": { "product": "CodeWhisperer for Terminal" } },
        { "match_phrase": { "metadata.metricName": "codewhispererterminal_recordUserTurnCompletion" } }
      ]
    }
  },
  "aggs": {
    "by_app_type": {
      "terms": { "field": "metadata.kirocli_appType", "size": 10 },
      "aggs": { "users": { "cardinality": { "field": "clientId" } } }
    },
    "missing_app_type": {
      "missing": { "field": "metadata.kirocli_appType" },
      "aggs": { "users": { "cardinality": { "field": "clientId" } } }
    }
  }
}'
```

Version breakdown:
```bash
./scripts/es-query.sh 'metrics-2026-04-09' '{
  "size": 0,
  "query": {
    "bool": {
      "must": [
        { "match_phrase": { "product": "CodeWhisperer for Terminal" } },
        { "match_phrase": { "metadata.metricName": "codewhispererterminal_recordUserTurnCompletion" } }
      ]
    }
  },
  "aggs": {
    "by_version": {
      "terms": { "field": "productVersion", "size": 30 },
      "aggs": { "users": { "cardinality": { "field": "clientId" } } }
    }
  }
}'
```

## Key field reference

| Field | Description |
|-------|-------------|
| `product` | Always `"CodeWhisperer for Terminal"` |
| `productVersion` | CLI version, e.g. `"1.29.5"` |
| `clientId` | Unique per-install UUID |
| `metadata.metricName` | Event name, e.g. `codewhispererterminal_recordUserTurnCompletion` |
| `metadata.kirocli_appType` | `"V2"` (kiro-tui), `"ACP"` (external clients), or missing (V1) |
| `metadata.kirocli_acpClientName` | ACP client name, e.g. `"kiro-tui"`, `"meshclaw"` |
| `metadata.credentialStartUrl` | SSO start URL. `amzn.awsapps.com` = internal Amazon |
| `metadata.result` | `"Succeeded"`, `"Failed"`, `"Cancelled"` |
| `metadata.reason` | Error reason code on failure |

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Rate exceeded (TooManyRequestsException)` | Cognito rate limit | Script retries automatically; wait a minute if persistent |
| Empty results / all zeros | Cookie expired | Re-do Step 1 |
| `Expecting value` parse error | 504 gateway timeout | Narrow the date range or query single days |
| `"hits": {"total": {"value": 10000, "relation": "gte"}}` | ES caps at 10k | Use `"track_total_hits": true` for exact counts |
