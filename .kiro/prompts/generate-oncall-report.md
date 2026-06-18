---
description: Generate the [Kiro-CLI] Weekly Ops Review report and write it to .ops/weekly-reviews/
---

# Generate Oncall Report

Generate the team's **`[Kiro-CLI] Weekly Ops Review`** report by following the
`weekly-ops-review` skill exactly — it is the single source of truth for the workflow,
the 10-section template, the ticket/oncall queries, the metric definitions, and the file
output step.

You do **not** need any arguments. If no oncall week is given, the skill defaults to the
most recently completed week (Mon 9 AM PST → Mon 9 AM PST) and prints the resolved week for
confirmation before continuing.

Optional inputs you may pass in natural language:
- a date range (e.g. `2026-06-08 to 2026-06-15`) to override the auto-detected week
- `dry_run` / `dry run` — preview the Markdown without writing to `.ops/weekly-reviews/`
- `oncall_alias`, `previous_report_url` — see the skill for details

Key rules (enforced by the skill):
- Each week produces a new file: `.ops/weekly-reviews/YYYY-MM-DD.md` (end date).
- Use ticket display IDs (`V…`/`P…`/`D…`), never internal UUIDs.
- Investigate a ticket's correspondence/links for root cause before ever writing "Unknown".
