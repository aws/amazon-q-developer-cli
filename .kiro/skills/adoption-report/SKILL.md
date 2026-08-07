---
name: adoption-report
description: Generate source-aware Kiro CLI adoption reports that combine production KUTS heartbeats and turns with legacy Toolkit Elasticsearch installation and turn data. Use when comparing KUTS with Toolkit, checking exact-version, channel, OS, install-method, V1/V2/V3 engine, session-interface, internal/external, or telemetry-migration coverage.
---

# Kiro CLI Adoption Report

Generate one report with clearly separated KUTS and Toolkit sections. Compare only compatible
turn counts; never imply that the two installation measures are equivalent.

## Workflow

1. Choose a narrow range of complete UTC days.
2. Run both sources by default:

   ```bash
   python3 .kiro/skills/adoption-report/scripts/adoption-report.py START_DATE END_DATE
   ```

   Use a count for the last N complete UTC days:

   ```bash
   python3 .kiro/skills/adoption-report/scripts/adoption-report.py 7
   ```

3. Present each section with its source label. Call out source failures, rollout boundaries,
   unexpectedly large `unknown` KUTS cohorts, and material gaps in the turn comparison.

The script writes `adoption-report.md`. If one source is unavailable, it produces a clearly
marked partial report from the other source. Do not describe an unavailable source as zero.

## Credentials

For KUTS, the script selects the first available profile in this order: `--profile`,
`KUTS_AWS_PROFILE`, `kuts_telemetry_prod_read-only`, `kuts`. It verifies AWS account
`615299732016` and queries `/kiro/metrics` in `us-east-1`.

For Toolkit, set `ES_COOKIE` or populate `scripts/.es-cookie`. If the cookie is missing or
expired, ask the user to:

1. Open
   `https://telemetry-externalprod.ide-toolkits.dev-tools.aws.dev/_plugin/kibana/app/dev_tools#/console`.
2. Run a query and copy the `_search` request as cURL from browser developer tools.
3. Extract the `-b` cookie value into `scripts/.es-cookie`.

Use `--source both|kuts|toolkit` to limit live queries. Use `--query-only` to inspect queries.
Use `--kuts-input-json PATH` and `--toolkit-input-json PATH` for offline rendering. Toolkit
multi-day input must contain `{"responses": {"YYYY-MM-DD": <Elasticsearch response>}}`.
When `--source` is omitted, supplied input files select only their offline sources; use
`--source both` explicitly to mix saved input with a live query. A legacy `--input-json`
invocation without `--source` remains KUTS-only and never queries either service.

## Source Semantics

**KUTS**

- Sum `kiro_cli_daily_heartbeat` by `version_full`, `release_channel`, `os_type`, and
  `install_method`. One heartbeat is one active installation-version day.
- Sum completed top-level `kiro_cli_user_turns` by `agent_engine` and `session_interface`.
- Report V1/V2/V3 turn share and interactive, noninteractive, and external ACP usage.

**Toolkit**

- Query `codewhispererterminal_recordUserTurnCompletion` from each daily `metrics-*` index.
- Report approximate active-installation segments with Elasticsearch cardinality on `clientId`;
  use exact matching event document counts for turns.
- Classify V2 with `metadata.kirocli_appType=V2` and V1 by the absence of that field.
- Classify external ACP with `metadata.kirocli_appType=ACP`.
- Classify internal activity by `metadata.credentialStartUrl` matching `amzn.awsapps.com`.

## Comparison Rules

- Compare daily Toolkit V1 turns with KUTS non-ACP V1, Toolkit's V2 app-type bucket with KUTS
  non-ACP V2+V3, and Toolkit ACP with KUTS `external_acp`, as migration-coverage signals.
- Compute each all-source total from the same V1/V2/ACP population and show signed gaps.
- Treat Toolkit app-type buckets as approximations, not exact equivalents of KUTS
  `agent_engine`; Toolkit cannot isolate V3.
- Do not directly compare Toolkit installation cardinality with KUTS heartbeats. Toolkit
  estimates daily installation IDs observed on turns; KUTS counts installation-version days.
- Do not sum Toolkit segment cardinalities into a unique total. An installation can appear in
  more than one engine segment on the same day.
- Treat Toolkit `clientId` as an installation identifier, not a unique person.
- Treat KUTS turn share as usage weighted by frequent users, not user adoption.

## Guardrails

- Keep `.kiro/prompts/v2-adoption-report.md`, `scripts/es-v2-report.py`, and
  `scripts/es-query.sh` unchanged. They remain the standalone legacy workflow.
- Do not use KUTS `user_id` or CloudWatch `count_distinct` for installation adoption.
- Do not infer weekly or monthly active installations, unique people, or new users.
- Keep missing KUTS dimensions as `unknown`; never relabel `install_method=unknown` as
  `installation_script`.
