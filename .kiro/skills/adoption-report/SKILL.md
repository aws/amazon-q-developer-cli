---
name: adoption-report
description: Generate Kiro CLI adoption reports for all three major agent versions (V1, V2, and V3) from the production KUTS kiro_cli_user_turns metric in CloudWatch Logs Insights. Use when asked for agent-version adoption, engine usage, modern TUI adoption, or daily V1/V2/V3 users and turns from KUTS.
---

# Agent Version Adoption Report

Generate adoption reports from forwarded KUTS metric records in the production
`/kiro/metrics` CloudWatch log group.

## Workflow

1. Resolve a narrow UTC date range with the user. Prefer complete days; large windows are
   expensive because Logs Insights scans the shared metrics log group.
2. Run the report:

   ```bash
   python3 .kiro/skills/adoption-report/scripts/adoption-report.py START_DATE END_DATE
   ```

   Use a count instead of dates for the last N complete UTC days:

   ```bash
   python3 .kiro/skills/adoption-report/scripts/adoption-report.py 7
   ```

   The script selects the first available profile in this order:
   `--profile`, `KUTS_AWS_PROFILE`, `kuts_telemetry_prod_read-only`, `kuts`.
   It queries account `615299732016`, region `us-east-1`, and writes
   `adoption-report.md` by default.
3. Read the generated report. Show the adoption table inline, summarize meaningful trends,
   and call out partial days, rollout boundaries, or missing engines.
4. Use `--query-only` to inspect the Logs Insights query without calling AWS. Use
   `--input-json PATH` to regenerate a report from saved `get-query-results` output.

## Metric Definition

Use only `kiro_cli_user_turns` records that:

- have `kuts.forwarded = "true"`
- have a non-empty `user_id`
- have `is_subagent = "false"`
- have `engine` equal to `v1`, `v2`, or `v3`

Calculate a separate daily adoption share for every agent version:

```text
version adoption = version distinct users / (V1 users + V2 users + V3 users)
```

Also report the combined modern TUI share as `(V2 users + V3 users) / total users`.

Always preserve these caveats from the generated report:

- CloudWatch `count_distinct` values are approximate.
- A user who uses multiple engines in one day is counted once for each engine, so the
  percentage is an adoption proxy rather than an exact mutually exclusive user split.
- Records without `user_id`, including some logged-out usage, are excluded.
- V1 data is incomplete before the V1 KUTS metric rollout. Do not interpret pre-rollout
  dates as full V1-versus-modern adoption.

## Guardrails

- Do not use `scripts/es-v2-report.py` or `scripts/es-query.sh` from this skill; those
  scripts remain available to the separate legacy V2 adoption workflow.
- Do not offer a minimum-version filter. KUTS turn records do not include a supported
  version dimension for this report.
- Do not classify users as internal or external. The new records do not contain an exact
  signal for that classification; never infer it from `credential_kind` or
  `install_method`.
- Do not substitute `active_users_daily`; it is not a confirmed queryable production
  series. Derive daily activity from `kiro_cli_user_turns`.
