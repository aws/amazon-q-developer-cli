---
name: adoption-report
description: Generate Kiro CLI active-installation version adoption and V1/V2/V3 engine-usage reports from production KUTS heartbeats and user-turn metrics in CloudWatch Logs Insights. Use when asked about exact-version, stable/nightly, OS, install-method, engine, or interactive-versus-external-ACP usage from KUTS.
---

# Kiro CLI Adoption Report

Generate one report from forwarded KUTS metric records in the production `/kiro/metrics`
CloudWatch log group.

## Workflow

1. Resolve a narrow UTC date range. Prefer complete days because the shared log group is
   expensive to scan.
2. Run:

   ```bash
   python3 .kiro/skills/adoption-report/scripts/adoption-report.py START_DATE END_DATE
   ```

   Use a count for the last N complete UTC days:

   ```bash
   python3 .kiro/skills/adoption-report/scripts/adoption-report.py 7
   ```

   The script selects the first available profile in this order: `--profile`,
   `KUTS_AWS_PROFILE`, `kuts_telemetry_prod_read-only`, `kuts`. It queries account
   `615299732016`, region `us-east-1`, and writes `adoption-report.md` by default.
3. Present active-installation adoption separately from engine usage. Call out rollout
   boundaries and unexpectedly large `unknown` cohorts.
4. Use `--query-only` to inspect the query without calling AWS. Use `--input-json PATH`
   to render saved `get-query-results` output.

## Metrics

Use `kiro_cli_daily_heartbeat` for adoption. Sum daily heartbeats by `version_full`,
`release_channel`, `os_type`, and `install_method`. One heartbeat represents one active
installation-version day, not one person. Report exact-version adoption share against all
heartbeats for the same UTC day.

Use `kiro_cli_user_turns` for usage. Sum completed top-level turns by `agent_engine` and
`session_interface`. Report V1/V2/V3 turn share and the `interactive_cli` versus
`noninteractive_cli` versus `external_acp` split. The producer suppresses subagent turn counters,
so do not require or filter on `is_subagent`.

Preserve these caveats:

- One installation that runs multiple versions in a day contributes once to each version.
- Summing several days produces installation-version days, not weekly or monthly active
  installations.
- Turn share weights frequent users more heavily and is engine usage, not user or installation
  adoption.
- Pre-migration records lack some reviewed dimensions and appear under `unknown`. Do not
  interpret that cohort as a real product category.
- `install_method=unknown` includes unattributable installs. Do not relabel it as
  `installation_script`.

## Guardrails

- Keep `.kiro/prompts/v2-adoption-report.md`, `scripts/es-v2-report.py`, and
  `scripts/es-query.sh` unchanged; they belong to the separate legacy workflow.
- Do not use `user_id` or CloudWatch `count_distinct` for installation adoption.
- Do not substitute retired `active_users_daily`, `client_version_seen`, or
  `version_adoption_pct` metrics. Derive adoption from heartbeat sums.
- Do not infer weekly active installations, monthly active installations, unique people,
  new users, or internal/external identity from these metrics.
