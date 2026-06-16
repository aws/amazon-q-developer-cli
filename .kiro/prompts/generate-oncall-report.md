---
description: Generate the [Kiro-CLI] Weekly Ops Review report and publish it to Quip
---

# Generate Oncall Report

Generate the team's **`[Kiro-CLI] Weekly Ops Review`** report by following the
`weekly-ops-review` skill exactly — it is the single source of truth for the workflow,
the 10-section template, the ticket/oncall queries, the metric definitions, and the Quip
publishing step.

You do **not** need any arguments. If no oncall week is given, the skill defaults to the
most recently completed week (Mon 9 AM PST → Mon 9 AM PST) and prints the resolved week for
confirmation before continuing.

Optional inputs you may pass in natural language:
- a date range (e.g. `2026-06-08 to 2026-06-15`) to override the auto-detected week
- `dry_run` / `dry run` — write the Markdown to `/tmp/kcli_oncall_report.md` and skip Quip
- `oncall_alias`, `previous_report_url` — see the skill for details

Key rules (enforced by the skill):
- Each week is a NEW Quip document in the reports folder
  (`https://quip-amazon.com/nfVzO8ENPg5z/series`) — never edit the template or a prior
  week's report.
- Use ticket display IDs (`V…`/`P…`/`D…`), never internal UUIDs.
- Investigate a ticket's correspondence/links for root cause before ever writing "Unknown".
