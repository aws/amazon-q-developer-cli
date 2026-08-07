---
description: Generate Kiro Crew (meshclaw/kirocrew) ACP client adoption telemetry reports from Kibana/Elasticsearch
---

# Kiro Crew ACP Client Adoption Report

Generate Kiro Crew adoption reports by querying the telemetry Elasticsearch cluster via Kibana. Tracks meshclaw, kirocrew, and all other ACP clients connecting to the CLI.

## Prerequisites

- Scripts: `scripts/es-query.sh`, `scripts/es-kiro-crew-report.py`
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
What date range do you want? Options:
1. Last N days (default: 7)
2. Specific date range (YYYY-MM-DD to YYYY-MM-DD)
```

### 3. Run the report

```bash
# Last 7 days (default)
python3 scripts/es-kiro-crew-report.py 7

# Last N days
python3 scripts/es-kiro-crew-report.py <N>

# Specific range
python3 scripts/es-kiro-crew-report.py <start-date> <end-date>
```

Output is written to `kiro-crew-adoption-report.md`.

### 4. Present the results

- Show the traffic breakdown table inline (by app type and by ACP client)
- Highlight kiro_crew family trends (meshclaw, kirocrew growth/decline)
- Note the unnamed ACP traffic and what it likely represents
- Call out any anomalies (weekend dips, release bumps, new clients appearing)

### 5. Ad-hoc queries

If the user wants custom queries, use `es-query.sh` directly:

```bash
./scripts/es-query.sh '<index-pattern>' '<query-json>'
```

**Index pattern**: `metrics-YYYY-MM-DD` for a single day, `metrics-2026-08-*` for a month.

**Common queries:**

All ACP client names on a day:
```bash
./scripts/es-query.sh 'metrics-2026-08-06' '{
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
    "by_acp_client": {
      "terms": { "field": "metadata.kirocli_acpClientName", "size": 50, "missing": "(none)" },
      "aggs": { "users": { "cardinality": { "field": "clientId" } } }
    }
  }
}'
```

Kiro Crew clients breakdown by version:
```bash
./scripts/es-query.sh 'metrics-2026-08-06' '{
  "size": 0,
  "query": {
    "bool": {
      "must": [
        { "match_phrase": { "product": "CodeWhisperer for Terminal" } },
        { "match_phrase": { "metadata.metricName": "codewhispererterminal_recordUserTurnCompletion" } },
        { "match": { "metadata.kirocli_acpClientName": "meshclaw" } }
      ]
    }
  },
  "aggs": {
    "by_version": {
      "terms": { "field": "productVersion", "size": 20 },
      "aggs": { "users": { "cardinality": { "field": "clientId" } } }
    }
  }
}'
```

Traffic by app type (V1/V2/ACP):
```bash
./scripts/es-query.sh 'metrics-2026-08-06' '{
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
      "terms": { "field": "metadata.kirocli_appType", "size": 10, "missing": "(missing)" },
      "aggs": {
        "by_client": {
          "terms": { "field": "metadata.kirocli_acpClientName", "size": 15, "missing": "(none)" },
          "aggs": { "users": { "cardinality": { "field": "clientId" } } }
        }
      }
    }
  }
}'
```

Internal vs external for a specific ACP client:
```bash
./scripts/es-query.sh 'metrics-2026-08-06' '{
  "size": 0,
  "query": {
    "bool": {
      "must": [
        { "match_phrase": { "product": "CodeWhisperer for Terminal" } },
        { "match_phrase": { "metadata.metricName": "codewhispererterminal_recordUserTurnCompletion" } },
        { "match": { "metadata.kirocli_acpClientName": "meshclaw" } }
      ]
    }
  },
  "aggs": {
    "internal": {
      "filter": { "wildcard": { "metadata.credentialStartUrl": "*amzn.awsapps.com*" } },
      "aggs": { "users": { "cardinality": { "field": "clientId" } } }
    },
    "external": {
      "filter": { "bool": { "must_not": [{ "wildcard": { "metadata.credentialStartUrl": "*amzn.awsapps.com*" } }] } },
      "aggs": { "users": { "cardinality": { "field": "clientId" } } }
    }
  }
}'
```

## Key field reference

| Field | Description |
|-------|-------------|
| `product` | Always `"CodeWhisperer for Terminal"` |
| `productVersion` | CLI version, e.g. `"2.16.1"` |
| `clientId` | Unique per-install UUID |
| `metadata.metricName` | Event name, e.g. `codewhispererterminal_recordUserTurnCompletion` |
| `metadata.kirocli_appType` | `"V2"` (kiro-tui), `"ACP"` (external clients), `"V1"` (legacy), or missing |
| `metadata.kirocli_acpClientName` | ACP client name from `InitializeRequest`. Key values: `"kiro-tui"`, `"meshclaw"`, `"kirocrew"`, `"agentspaces-proxy"`, `"kiroom"` |
| `metadata.credentialStartUrl` | SSO start URL. `amzn.awsapps.com` = internal Amazon |
| `metadata.result` | `"Succeeded"`, `"Failed"`, `"Cancelled"` |

## Known ACP client names

| Client | Category | Description |
|--------|----------|-------------|
| `kiro-tui` | Kiro CLI | Built-in TUI (app type = V2, not ACP) |
| `meshclaw` | Kiro Crew | Original kiro_crew ACP client |
| `kirocrew` | Kiro Crew | New/renamed kiro_crew client (appeared Aug 2026) |
| `kiroclaw` | Kiro Crew | Variant claw client |
| `spudclaw` | Kiro Crew | Variant claw client |
| `cargoclaw` | Kiro Crew | Variant claw client |
| `agentspaces-proxy` | Cloud Workspaces | Cloud dev environment proxy |
| `kiroom` | Room Agent | Room-based collaborative agent |
| `talkstream` | Voice/Stream | Voice/stream agent (high automation) |
| `kask` | Task Agent | Task-based agent |
| `kiro-ide` | IDE | Kiro IDE direct integration |
| `JetBrains.IntelliJ IDEA` | IDE | JetBrains plugin |
| `zed` | IDE | Zed editor integration |
| `agent-server` | Infrastructure | Generic agent server |

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Rate exceeded (TooManyRequestsException)` | Cognito rate limit | Script retries automatically; wait a minute if persistent |
| Empty results / all zeros | Cookie expired | Re-do Step 1 |
| `Expecting value` parse error | 504 gateway timeout | Narrow the date range or query single days |
| `"hits": {"total": {"value": 10000, "relation": "gte"}}` | ES caps at 10k | Use `"track_total_hits": true` for exact counts |
