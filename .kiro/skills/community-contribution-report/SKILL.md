---
name: community-contribution-report
description: Generate a weekly community contribution PR report for kiro-team/kiro-cli — a bucketed snapshot of external-contributor PRs (excluding core team and bots) with engagement, merge, and TTFE metrics, plus deltas against the previous snapshot. Use when asked for the community PR report, contribution report, weekly contribution digest, PR engagement metrics, or contribution deltas. Triggers on "community PR report", "contribution report", "community engagement", "weekly PR digest", "contribution delta", "office hours report".
---

# Community Contribution Report — Generation SOP

Generate a weekly report of community (non-core, non-bot) pull request activity for
`kiro-team/kiro-cli`. The report is used to track engagement and merge throughput for
external contributors — starting point of the contribution model on **2026-06-15**.

Output:
- `.ops/community-reports/YYYY-MM-DD.md` (Markdown, always)
- `.ops/community-reports/YYYY-MM-DD.json` (raw snapshot data — feeds the next report's delta table)
- Optional `~/Desktop/community_prs_since_YYYY-MM-DD.pdf` (rich PDF, requires `reportlab`)

## When to use

- Slack ping like "can you share the community PR data?"
- Before **Kiro CLI office hours** (weekly).
- Any request to see engagement %, merge rate, or delta vs. last week for external
  contributors.

## Parameters

- **`--since`** *(optional, default `2026-06-15`)*: window start date. The contribution
  model started here, so this is normally left alone.
- **`--as-of`** *(optional, default today)*: window end date. Used for the report's
  filename and for computing "current" state.
- **`--previous`** *(optional)*: path to a prior snapshot JSON in
  `.ops/community-reports/`. If omitted, the script auto-picks the most-recent snapshot
  whose date is strictly before `--as-of`. Pass `--previous none` to skip delta.
- **`--no-pdf`** *(optional)*: skip the PDF (Markdown only).
- **`--dry-run`** *(optional)*: print the Markdown to stdout, do not write files or PDF.

## Data source

- `gh pr list --repo kiro-team/kiro-cli --search "created:>=YYYY-MM-DD" --state all`
  with the JSON fields the script needs.
- `gh pr view <n> --json comments` per PR (parallelizable) to get comment authors —
  formal `reviews` are already in the list output.

## Exclusion rules (what counts as "community")

- **Author is not in** `core_team.json` (co-located with this skill, editable — this is
  the maintenance surface). Update the file whenever a new engineer joins the team.
- **Author is not a bot** — filter names starting with `app/` and the fixed bot list
  in the script (`github-actions`, `dependabot`, `codecov-commenter`, `sonarcloud`).
- **Not a draft** at report generation time.

## Metrics reported

- **Submitted / Engaged / Merged / Open / Closed unmerged** — top-level counts.
- **Community engagement** = at least one review OR comment from a non-author, non-bot
  user. The GitHub Actions summary bot is excluded so its per-PR comment doesn't count.
- **TTFE (time-to-first-engagement)** — median, p75, max in hours. Uses whichever
  arrived first, a review or a human comment.
- **Merge rate once engaged** — merged ÷ engaged (the key throughput number).
- **Weekly snapshot delta** — a compact table with previous snapshot, current snapshot,
  and the Δ row.
- **Full PR list bucketed by snapshot period** — each bucket is a mini-tally + a table.
  Newest bucket appears first. PRs stay in the bucket they were *submitted* in, so a PR
  that's created in week 1 and merged in week 3 keeps appearing in week 1's bucket with
  its updated state.

## Where the report lives

`.ops/community-reports/` in the repo root — one Markdown + one JSON per weekly run,
named `YYYY-MM-DD.md` / `YYYY-MM-DD.json` after the `--as-of` date.

Both are checked into the repo so the next run can compute deltas without any external
state. Keep them under version control.

## Step 0 — Resolve the reporting window

If `--as-of` was not passed, use today's date. If `--previous` was not passed, glob
`.ops/community-reports/*.json` and take the most-recent snapshot strictly before
`--as-of`. Print the resolved parameters back to the user before proceeding so they
can correct if needed.

## Step 1 — Fetch PR data

```bash
gh pr list --repo kiro-team/kiro-cli \
  --search "created:>=<since>" --state all --limit 1000 \
  --json number,author,state,createdAt,mergedAt,closedAt,updatedAt,reviews,title,additions,deletions,isDraft \
  > /tmp/community-report-prs.json
```

Then, for each non-core non-bot non-draft PR, fetch comments (`gh pr view <n> --json
comments`) — sequential is fine for weekly cadence (~30 PRs), or parallelize with
`xargs -P` if the list grows.

## Step 2 — Compute metrics and buckets

Delegate to `generate_report.py` (co-located). The script reads `core_team.json`,
applies the exclusion rules, computes all metrics, loads `--previous` if provided,
and emits both Markdown and JSON. See the script's `--help`.

## Step 3 — PDF (optional)

If `reportlab` is available and `--no-pdf` was not passed, generate a rich PDF to
`~/Desktop/community_prs_since_<since>.pdf`. The PDF has the same content as the
Markdown but with clickable PR links, colored rows for zero-engagement PRs, and a
proper snapshot-delta table.

## Step 4 — Commit and share

Commit the Markdown + JSON on a branch (`ops/community-report-YYYY-MM-DD`) and open
a PR titled `[Community] Contribution Report — YYYY-MM-DD`. Post the Markdown summary
+ full table in Slack when ready to share (usually before office hours).

If the user asks for a specific format like "Slack-ready", print the Markdown table
inline in the response — no PR needed.

## Common failure modes

- **PR count seems too low** → confirm the author filter isn't accidentally excluding
  a new team member (edit `core_team.json`) or dropping a valid contributor.
- **Engagement drops unexpectedly** → check whether a new bot was added. Bots must be
  added to the exclusion list in `generate_report.py`.
- **Delta rows look wrong** → confirm the previous snapshot JSON was loaded correctly.
  Passing `--previous <path>` explicitly is a good sanity check.
- **PDF fails to render** → `pip install --user reportlab`; the Markdown always
  generates regardless of `reportlab` availability.

## Maintenance surface

- **`core_team.json`** — keep updated as engineers join/leave the team. The file is a
  simple JSON array of GitHub handles.
- **Bot list** in `generate_report.py` — extend when new automation comments start
  appearing on PRs.
- **Snapshot storage** — never delete old snapshots. Each report's delta relies on the
  prior week's JSON.
