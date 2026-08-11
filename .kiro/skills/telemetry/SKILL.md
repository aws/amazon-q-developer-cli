---
name: telemetry
description: Query Kiro CLI production telemetry from KUTS (CloudWatch Logs Insights). Covers authentication, schema, key fields, and sample queries. Use when investigating CLI metrics, user counts, engine usage, session data, or writing new telemetry queries.
---

# Kiro CLI Telemetry

Query production Kiro CLI telemetry stored in CloudWatch Logs via the KUTS pipeline.

## Authentication

Credentials target AWS account `615299732016` (KUTS prod), region `us-east-1`.

```bash
# Refresh credentials (expires after ~12h)
ada credentials update --profile kuts_telemetry_prod_read-only --account 615299732016 --role ReadOnly --once

# Verify
aws sts get-caller-identity --profile kuts_telemetry_prod_read-only
```

The profile chain tried by scripts: `--profile` flag → `KUTS_AWS_PROFILE` env var → `kuts_telemetry_prod_read-only` → `kuts`.

## Data Source

| Property | Value |
|----------|-------|
| Log group | `/kiro/metrics` |
| Region | `us-east-1` |
| Account | `615299732016` |
| Record filter | `kuts.forwarded` = `"true"` |
| Index pattern | Daily partitioned (no explicit index) |

Records are OTEL metric data points forwarded by the KUTS ADOT collector. Each record contains one metric value plus its dimensions as top-level fields.

## Schema

### Key Metrics (top-level numeric fields)

| Metric | What it counts |
|--------|---------------|
| `kiro_cli_user_turns` | Completed user turns (one prompt → full agent response) |
| `kiro_cli_daily_heartbeat` | One per active installation per day |
| `kiro_cli_run_started_total` | CLI process launches |
| `kiro_cli_user_turn_duration_seconds` | Duration of each turn |
| `kiro_cli_time_to_first_visible_response_ms` | Time to first content streamed |
| `kiro_cli_tool_call_total` | Tool invocations |

### Key Dimensions (top-level string fields)

| Field | Values | Description |
|-------|--------|-------------|
| `agent_engine` | `v1`, `v2`, `v3` | Which agent engine processed the turn |
| `session_interface` | `interactive_cli`, `noninteractive_cli`, `external_acp` | How the user connected |
| `version_full` | e.g. `2.16.2`, `2.16.2-nightly.1` | CLI version |
| `agent_mode` | `default`, `spec`, etc. | Agent mode/persona |
| `user_id` | `d-XXXXXXXXXX.UUID` or `https://...` | User identity (SSO/IdC directory format) |
| `os_type` | `linux`, `macos`, `windows` | Operating system |
| `install_method` | `internal_toolbox`, `brew`, `unknown` | How CLI was installed |
| `release_channel` | `stable`, `nightly` | Release channel |
| `OTelLib` | `kiro-telemetry`, `kiro.tui` | Instrumentation library |

### Understanding `user_id`

- Format: `<directory-id>.<user-uuid>` for SSO/IdC users
- `d-9067925563` = Amazon corporate directory (internal users)
- Other `d-XXXXXXXXXX` prefixes = external enterprise customers
- `https://...` prefix = Builder ID users
- ~81% of `kiro_cli_user_turns` records have `user_id` populated
- ~19% are missing (Builder ID users on versions that don't emit it)

### Understanding `session_interface`

| Value | Meaning |
|-------|---------|
| `interactive_cli` | User typing in the TUI (V1 Classic or V2/V3 TUI) |
| `noninteractive_cli` | Headless/CI/scripted (no terminal UI) |
| `external_acp` | External ACP client connecting to CLI (meshclaw, kirocrew, agentspaces-proxy, etc.) |

### What V3 (KAS) does NOT have

- V3 does NOT emit to the legacy Elasticsearch telemetry cluster (Kibana)
- V3 does NOT have `external_acp` traffic (no ACP clients connect to KAS yet)
- V3 only appears in KUTS (`/kiro/metrics`) data

## Running Queries

### Via Script Helper

```bash
python3 .kiro/skills/telemetry/scripts/query.py '<logs-insights-query>' --days 1
python3 .kiro/skills/telemetry/scripts/query.py '<logs-insights-query>' --start 2026-08-10 --end 2026-08-10
```

### Via AWS CLI Directly

```bash
aws --profile kuts_telemetry_prod_read-only --region us-east-1 \
  logs start-query \
  --log-group-name /kiro/metrics \
  --start-time <epoch> --end-time <epoch> \
  --query-string '<query>'

# Then poll:
aws --profile kuts_telemetry_prod_read-only --region us-east-1 \
  logs get-query-results --query-id <id>
```

## Sample Queries

### Total distinct users by engine (1 day)

```
fields @timestamp, user_id, agent_engine
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
| stats count_distinct(user_id) as users
  by bin(24h) as day, agent_engine
| sort day asc
```

### Users by engine and interface

```
fields @timestamp, user_id, agent_engine, session_interface
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
| stats count_distinct(user_id) as users
  by bin(24h) as day, agent_engine, session_interface
| sort day asc
```

### Internal users only (Amazon corporate)

```
fields @timestamp, user_id, agent_engine
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
  and user_id like /d-9067925563/
| stats count_distinct(user_id) as users
  by bin(24h) as day, agent_engine
| sort day asc
```

### Total distinct users (deduplicated across everything)

```
fields @timestamp, user_id
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
| stats count_distinct(user_id) as users
  by bin(24h) as day
| sort day asc
```

### Turns by version (top versions)

```
fields @timestamp, version_full
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
| stats sum(kiro_cli_user_turns) as turns
  by version_full
| sort turns desc
| limit 10
```

### Check what fields exist on a record

```
fields @message
| filter `kuts.forwarded` = "true"
  and ispresent(kiro_cli_user_turns)
| limit 5
```

## Adoption Report

Generate the adoption report showing users by engine and interface:

```bash
# Last 7 complete UTC days
python3 .kiro/skills/telemetry/scripts/adoption-report.py 7

# Specific date range
python3 .kiro/skills/telemetry/scripts/adoption-report.py 2026-08-04 2026-08-10

# Single day
python3 .kiro/skills/telemetry/scripts/adoption-report.py 1
```

Output: `<run-date>-adoption-report.md` (override with `-o PATH`).

The report contains:
- **Users by Engine and Interface** — Total (deduplicated), V1 Classic, Headless, V2 TUI, V2 Non-Interact., V2 ACP, V3 TUI, V3 Non-Interact., V3 ACP with (%)
- **Users by Engine and Internal/External** — V1/V2/V3 split by Amazon corporate vs external

When presenting, call out:
- V3 growth trends
- Weekend dips (typically ~60-70% drop)
- V2 ACP as percentage of total (external client ecosystem health)
- Internal vs external ratio shifts

## Cost and Performance

- The `/kiro/metrics` log group scans ~3.4 TiB per day queried, per query.
- The adoption report issues **four** Logs Insights queries over the same window (main, total, total-by-engine, internal-by-engine), so a 1-day report scans ~13 TiB and a 7-day report scans ~90+ TiB. The report footer sums the scan across all four.
- Keep date ranges as narrow as possible.
- Use `ispresent(<metric>)` to filter to only the relevant metric's records.
- Queries time out at 15 minutes by default (CloudWatch Logs Insights limit).

## Guardrails

- Use ReadOnly credentials. Never write to this account.
- `count_distinct(user_id)` undercounts by ~19% (Builder ID users missing `user_id`).
- Users can appear in multiple `session_interface` and `agent_engine` buckets on the same day.
- `kiro_cli_daily_heartbeat` counts installations, not users. One person with two installs = two heartbeats.
- Do not conflate turns with users. High turn counts can come from automated pipelines with few users.
- The current UTC day is always incomplete. Exclude it from reports.
