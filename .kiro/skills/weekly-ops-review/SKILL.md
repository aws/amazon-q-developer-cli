---
name: weekly-ops-review
description: Generate the weekly "[Kiro-CLI] Weekly Ops Review" oncall report for the Amazon Q for CLI resolver group and write it to .ops/weekly-reviews/ in the repo. Use when asked to create/write the weekly oncall report, ops review, or ops meeting doc. Triggers on "weekly ops review", "oncall report", "ops review doc", "weekly report".
---

# Kiro CLI Weekly Ops Review — Report Generation SOP

Generate the **`[Kiro-CLI] Weekly Ops Review`** report for the `Amazon Q for CLI`
resolver group, write it to **`.ops/weekly-reviews/`**, and open a review PR so the team
can comment on it during the ops meeting.

- Output: `.ops/weekly-reviews/YYYY-MM-DD.md` (where `YYYY-MM-DD` is the `end_date`)
- Template: `.ops/weekly-reviews/TEMPLATE.md` (the canonical 10-section layout to fill in)
- Example: `.ops/weekly-reviews/2026-06-15.md`
- Review branch: `ops/weekly-review-YYYY-MM-DD` → PR titled `[Kiro-CLI] Weekly Ops Review - MM/DD/YYYY`

The report has **10 sections** (Step 7). Ticket data is pulled directly from the
ticketing system. By default the report is committed to a dedicated branch and a PR is
opened (Step 9); reviewers leave comments during the meeting, and
`@apply-ops-review-comments` (Step 11) folds them back in. Merge the PR when finalized.

## Team Constants

- **Resolver / assigned group:** `Amazon Q for CLI` (`extensions.tt.assignedGroup:"Amazon Q for CLI"`)
- **CTI:** `Kiro / CLI / Intake`
- **Primary paging alias:** `page-amazon-q-cli-primary@amazon.com`
- **Ticket Queue saved query** (Section 1 link):
  `https://t.corp.amazon.com/issues/?q=extensions.tt.status%3A%28Assigned%20OR%20Researching%20OR%20%22Work%20In%20Progress%22%20OR%20Pending%29%20AND%20extensions.tt.assignedGroup%3A%22Amazon%20Q%20for%20CLI%22`
- **DeeOps dashboard:** `https://deeops.aka.amazon.com/ops_readiness/sto_health/`
- **Previous Week's Action Items (Section 5):** `https://tiny.amazon.com/1auvbeoty/taskamazdevroom7c22task`
- **Dashboard Review notes (Section 10):** `https://quip-amazon.com/umwaAzDXcFo1`
- **Reports directory:** `.ops/weekly-reviews/` in the repository root. Every weekly report
  is written here and read from here to find the prior week's report (for the Section 1
  starting queue). This directory is the source of truth.

## Parameters

- **start_date** (optional): `YYYY-MM-DD`. Oncall week starts 9 AM PST → `{start_date}T17:00:00Z`.
- **end_date** (optional): `YYYY-MM-DD`. Oncall week ends 9 AM PST → `{end_date}T17:00:00Z`.
  The report title uses this date as `MM/DD/YYYY`.

  **If dates are omitted** (e.g. the user just says "generate the oncall report"), default
  to the **most recently completed oncall week**: the oncall week runs Monday 9 AM PST →
  the following Monday 9 AM PST, so `end_date` = the most recent past Monday (the last
  handoff at/before today) and `start_date` = `end_date − 7 days`. Compute this with
  `shell` using the current date (e.g. `date`), do NOT hardcode. Print the resolved week
  back to the user before proceeding so they can correct it if they meant a different week.
- **oncall_alias** (optional): primary oncall for the week (Section 8 security links).
  Resolved from the schedule if omitted.
- **previous_report_url** (optional): last week's report path — overrides the auto-lookup of
  the prior report for the Section 1 starting-queue figure and open action items.
- **no_pr** (optional, default `false`): write the report to `.ops/weekly-reviews/` on the
  current branch and STOP — do not create a branch or open a PR.
- **dry_run** (optional, default `false`): print the Markdown to stdout and skip writing the
  file, creating a branch, or opening a PR.

## Context discipline

Write ticket data to disk before analyzing — do NOT hold raw API responses in context.
Initialize scratch files at the start:

```bash
> /tmp/kcli_oncall_sev2.jsonl       # high-sev (Sev 1/2/2.5) ticket details
> /tmp/kcli_oncall_sev2_ids.txt     # deduped high-sev display IDs (V/P/D)
> /tmp/kcli_oncall_incoming.jsonl   # raw incoming tickets (for dedup → Section 1 Incoming)
> /tmp/kcli_oncall_resolved.jsonl   # resolved tickets (Section 1 Resolved + Section 2)
> /tmp/kcli_oncall_open_sev2.jsonl  # currently-open Sev2s (Section 7)
> /tmp/kcli_oncall_partner.jsonl    # tickets cut to other teams (Section 9)
> /tmp/kcli_oncall_pages.jsonl      # page log — one row per page EVENT (Section 6)
> /tmp/kcli_oncall_metrics.json     # Section 1 numbers
> /tmp/kcli_oncall_report.md        # final report
```

Fetch ticket details (`get-ticket`) in parallel batches of 8; extract + append to the
scratch file after each batch before the next. Paginate `search-tickets` via `start`
(100, 200, …) up to 1000 if `totalCount > 100`.

## Step 0 — Resolve the reporting window

If the user gave explicit `start_date`/`end_date`, use them. **If they didn't** (e.g. just
"generate the oncall report"), compute the most recently completed oncall week with `shell`
(do NOT hardcode):

```bash
# Oncall week: Monday 09:00 PST → following Monday 09:00 PST.
# end_date = most recent past Monday (last handoff at/before today); start_date = end_date - 7d
end_date=$(python3 -c "import datetime;t=datetime.date.today();print(t-datetime.timedelta(days=(t.weekday()) or 7))")
start_date=$(python3 -c "import datetime;print(datetime.date.fromisoformat('$end_date')-datetime.timedelta(days=7))")
echo "Reporting week: $start_date -> $end_date"
```

Print the resolved week to the user (e.g. "Generating the oncall report for 2026-06-08 →
2026-06-15") so they can correct it before the run continues. Then derive
`start_iso = {start_date}T17:00:00Z`, `end_iso = {end_date}T17:00:00Z`, and the title date
`{end_date}` as `MM/DD/YYYY`.

## Step 1 — Resolve oncall schedule

Use `@builder-mcp/OncallReadActions` to resolve current / next / previous primary oncall
for `Amazon Q for CLI` (paging alias `page-amazon-q-cli-primary`):

- `search-teams` query `amazon-q-cli` → `currentOncalls` gives the shift starting on
  `end_date` (the **next** oncall).
- `get-team-shifts` / `get-user-shifts` with `startDate={start_date − 7d}`,
  `endDate={end_date + 7d}` → identify current-week and previous-week oncall.

`oncall_alias` (if provided) overrides the resolved current oncall. If lookup fails, use
`TBD` and continue (do not hard-fail).

## Step 2 — Section 1 summary metrics

Run with `@builder-mcp/TicketingReadActions action=search-tickets` (read `totalCount`):

1. **Ending queue `y`** — open now (count, `rows:1`):
   ```
   assignedGroup: ["Amazon Q for CLI"]
   status: ["Assigned","Researching","Work In Progress","Pending"]
   rows: 1
   ```

2. **Incoming (deduplicated issues)** — fetch the list, then collapse alarm storms:
   ```
   assignedGroup: ["Amazon Q for CLI"]
   status: ["Assigned","Researching","Work In Progress","Pending","Resolved","Closed"]
   createDate: "[{start_iso} TO {end_iso}]"
   rows: 100
   sort: "createDate asc"
   responseFields: ["id","aliases","title","extensions","createDate"]
   ```
   Write raw rows to `/tmp/kcli_oncall_incoming.jsonl`, then compute **distinct issues**:
   group rows by `extensions.tt.dedupeString` prefix (strip trailing region/timestamp), and by
   normalized alarm name in the title — all `QCLI-SuccessRateDown`, all `…CacheHitRate…`,
   all `ConsolasRTS-…Availability…` each collapse to ONE issue. `incoming = number of
   distinct issues` (this is how the team reports "Incoming" — an alarm storm of 5 pages is
   1 incoming issue). Also record `incoming_raw = totalCount` for transparency.

3. **Resolved** — union of two searches, deduped by `display_id`:
   - **(a) assigned-group resolved**, widened to the full handoff day. The team counts
     tickets resolved after the 9 AM handoff up to when the report is written — e.g.
     `P454617284` resolved `06/15 18:29 UTC` (after the 17:00Z cutoff) was still counted:
     ```
     assignedGroup: ["Amazon Q for CLI"]
     status: ["Resolved","Closed"]
     lastResolvedDate: "[{start_iso} TO {end_date}T23:59:59Z]"
     rows: 100
     responseFields: ["id","aliases","title","extensions.tt.rootCause","lastResolvedDate"]
     ```
   - **(b) CLI-CTI resolved across ALL groups** — catches CLI tickets the oncall drove to
     closure that live in adjacent groups:
     ```
     query: 'extensions.tt.category:"Kiro" AND extensions.tt.type:"CLI" AND status:(Resolved OR Closed)'
     lastResolvedDate: "[{start_iso} TO {end_date}T23:59:59Z]"
     rows: 100
     responseFields: ["id","aliases","title","extensions.tt.assignedGroup","extensions.tt.rootCause","lastResolvedDate"]
     ```
   Union (a)+(b), dedupe by `display_id`, write to `/tmp/kcli_oncall_resolved.jsonl`.
   `resolved = distinct count`. This list also feeds the Section 2 root-cause table.

**Starting queue `x` — auto-find the previous report:**
1. Look for `.ops/weekly-reviews/{start_date}.md` (the prior week's end date = this week's
   start date). Read that file and take its Section 1 **ending** queue (the `y` in `x → y`)
   — that is this week's `x`.
2. If `previous_report_url` was passed, read it directly instead of searching.
3. Only if no previous report can be found, estimate `x = y − incoming_raw + resolved` and
   prefix it with `~`.

**Pages — real page events (NOT distinct tickets):**
1. First call `@builder-mcp/OncallReadActions action=get-report-instructions` with
   `resolverGroup: "Amazon Q for CLI"`, `startDate`, `endDate`. It is purpose-built for
   oncall reports and may return the page list/count or the canonical paging source — use
   it if it yields page data.
2. Otherwise count page **events**: for every Sev2 ticket from Step 3, inspect its
   worklog/communications for paging notifications to `page-amazon-q-cli-primary` and count
   EACH one — a ticket that was New, then Escalated, then Reassigned = **3** pages (this is
   why counting distinct tickets under-counted). Recognized page triggers: `New`,
   `Reopened`, `Reactivated`, `Escalated`, `Reassigned`, `Upgraded` to Sev2. Each event
   becomes one Page Log row in Step 5; `pages` = total events.

Write `/tmp/kcli_oncall_metrics.json`:
`{"pages":N,"queue_start":N,"queue_end":N,"incoming":N,"incoming_raw":N,"resolved":N,"lse_count":N,"queue_start_source":"prev-report|estimate"}`
(`lse_count` filled in Step 6).

## Step 3 — Fetch high-severity (Sev2) tickets

`search-tickets` keyed on `createDate`, and again on `lastUpdatedDate` (to catch reopened/
escalated tickets), then union the **display IDs** (from `aliases`, i.e.
`V…`/`P…`/`D…`) into `/tmp/kcli_oncall_sev2_ids.txt` (`sort -u`). These display IDs are
what appears in the report and what Step 8 validates against; `get-ticket` accepts the
display ID as its `ticketId`:

```
assignedGroup: ["Amazon Q for CLI"]
currentSeverity: ["1","2","2.5"]
status: ["Assigned","Researching","Work In Progress","Pending","Resolved","Closed"]
createDate: "[{start_iso} TO {end_iso}]"
rows: 100
sort: "createDate asc"
responseFields: ["id","aliases","title","status","extensions","createDate"]
```

**Ticket ID rule (IMPORTANT):** the top-level `id` field is an internal UUID
(e.g. `63dddddb-bd71-40e6-...`) — do NOT use it in the report. Use the human-readable
**display ID** (`V…` / `P…` / `D…`), which is in the `aliases` array (pick the alias
that looks like `V2239674541`, `P449035647`, `D468841287`). If you are unsure which field
holds it, call `get-search-instructions` once. Record this as `display_id` for every ticket
and use it for all link text and URLs.

Fetch each ID with `get-ticket` in batches of 8. For each, append a JSON line to
`/tmp/kcli_oncall_sev2.jsonl` with: `id (UUID, internal use only), display_id (V/P/D — the
one shown in the report), title, status, sev (extensions.tt.impact), closureCode,
rootCause, rootCauseDetails, resolution, dedupeString (from extensions.tt), createDate,
lastResolvedDate`, **human comments only** (author + first 300 chars; skip automated
authors: Medic, SmartTTBots, SnowEngine, asbx-medic-prod, AutoSIM, OSSA, ossa-genai-agent,
TRI BOT, SIMCrux, flx-cloudwatch, PitMinerArsenic, ShoehornProofNotifier,
TicketyCategorizationMaxisRole), `mcms`, and any `paging` events/links.

## Step 4 — Open Sev2s + partner-team tickets

**Open Sev2s (Section 7):**
```
assignedGroup: ["Amazon Q for CLI"]
currentSeverity: ["1","2","2.5"]
status: ["Assigned","Researching","Work In Progress","Pending"]
rows: 100
responseFields: ["id","aliases","title","status","createDate"]
```
→ `/tmp/kcli_oncall_open_sev2.jsonl`. Reuse description/comments from
`/tmp/kcli_oncall_sev2.jsonl` where the ID already exists. Record `display_id` (V/P/D) for
each, not the UUID.

**Tickets cut to other teams (Section 9):**
- Scan `/tmp/kcli_oncall_sev2.jsonl` comments for `https://t.corp.amazon.com/(issues/)?(P|V|D)\d+`
  (incl. `/communication`) pointing at tickets owned by other teams (backend, ASBX, etc.).
- Also search reassigned tickets:
  ```
  status: ["Assigned","Researching","Work In Progress","Pending","Resolved","Closed"]
  createDate: "[{start_iso} TO {end_iso}]"
  query: 'extensions.tt.tags:"Amazon Q for CLI" AND NOT extensions.tt.assignedGroup:"Amazon Q for CLI"'
  rows: 50
  responseFields: ["id","aliases","title","status","extensions.tt.assignedGroup"]
  ```
  → `/tmp/kcli_oncall_partner.jsonl`. If empty, Section 9 says `None`.

## Step 5 — Page Log (Section 6)

The Page Log has **one row per page event** — the same ticket appears multiple times if it
paged multiple times (the manual 06/15 report had 29 page rows for ~18 distinct tickets:
`V2249300247`, `V2245890305`, `P432259107`, etc. each appear 2–3×). Build it to match:

1. If `get-report-instructions` (Step 2) returned page data, use it directly — each page is
   a row `{display_id, page_title, page_url, paged_at}`. Page URLs look like
   `https://paging.corp.a2z.com/#/pages/<id>`.
2. Otherwise reconstruct from ticket history: for each Sev2 ticket, emit one row per paging
   notification (`New` / `Reopened` / `Reactivated` / `Escalated` / `Reassigned` /
   `Upgraded` to Sev2) found in its worklog/communications. Synopsis = the notification
   subject (e.g. `New Sev2 - [ALARM] [us-east-1] QCLI-SuccessRateDown`); link to the ticket
   `https://t.corp.amazon.com/<display_id>` when no paging-page URL is available.

Append every event to `/tmp/kcli_oncall_pages.jsonl`, ordered chronologically. Set
`metrics.pages` = the number of rows (this MUST equal the `pages` count from Step 2).

## Step 6 — Root-cause breakdown, LSEs, grouping

- **Section 2 (Resolved by Root Cause):** read `/tmp/kcli_oncall_resolved.jsonl` and group
  by `extensions.tt.rootCause`. Each row: `Root Cause | Count | Topic | Tickets` (list ALL
  ticket links by `display_id`). The `Total` count MUST equal the Section 1 Resolved
  number. A genuinely ticketless item the oncall resolved (e.g. a backend-capacity issue
  with "no explicit ticket") may be added as a row with an empty Tickets cell, matching how
  the team authors this table. When `rootCause` is empty, derive it per the **Root cause &
  descriptions** rule below — do NOT write "Unknown" unless truly nothing is found.
- **Section 4 (LSEs):** identify customer-impacting / cross-team incidents (broad blast
  radius, cascading tickets, PII exposure, prod outages) → one bullet each. Update
  `metrics.lse_count`.
- **Grouping** (for Sections 2/4/6/7): group by same `extensions.tt.dedupeString` prefix, same
  alarm across regions, explicitly linked tickets, or same root cause. Never group
  unrelated tickets; every ticket ID stays individually traceable.

### Root cause & descriptions (applies to Sections 2, 6, 7, 9)

When you need a root cause or a description for a ticket and the structured field is empty
or unhelpful, you MUST try, in order, before falling back to "Unknown":

1. **Use the ticket's correspondence** already captured in `/tmp/kcli_oncall_sev2.jsonl` —
   read the human comments/worklog, the `resolution` text, and `closureCode`. The cause is
   very often stated by the oncall in a resolve/worklog comment (e.g. P454617284's resolve
   comment "Cut a Sev2 to the toolkit telemetry team").
2. **Re-fetch the ticket** with `@builder-mcp/TicketingReadActions action=get-ticket` to
   read the full comment thread if the cached extract is insufficient.
3. **Infer from the alarm/title + linked artifacts** — the alarm name, region, model, or a
   linked CR/MCM/PR in the comments usually makes the cause clear (e.g.
   `…CacheHitRate…Critical` → "Cache-hit-rate alarm, backend-owned"; a linked hotfix PR →
   the defect it fixed).
4. **Briefly investigate** if still unclear — read a referenced CR/PR/COE/wiki via
   `@builder-mcp/ReadInternalWebsites` or search with `@builder-mcp/InternalSearch`.

Only write `Unknown` (or a one-line "no root cause recorded; <what you checked>") if all of
the above genuinely yield nothing. Never default to "Unknown" just because the structured
`rootCause` field was blank.

## Step 7 — Assemble the Markdown report

Start from the canonical template at **`.ops/weekly-reviews/TEMPLATE.md`** — read it,
fill in every `{placeholder}`, and write the result to `/tmp/kcli_oncall_report.md`. The
template defines exactly these 10 sections; do not add, remove, or reorder them.

Formatting rules: use Markdown tables (header + `|---|` row, no blank lines inside tables).
Bullet lists use `*` with a blank line between a label and its first item. Ticket links
MUST use the human-readable **display ID** (`display_id` — `V…`/`P…`/`D…`), never the
internal UUID: `[<display_id>](https://t.corp.amazon.com/<display_id>)` (e.g.
`[V2239674541](https://t.corp.amazon.com/V2239674541)`). Throughout the template, `{id}`
means the display ID. Never leave auto-populated fields blank — use real data, `None`, or
`Unknown`.

Sections 3, 5, 8, 10 keep their standing placeholders/links (filled live during the
meeting). Grouped tickets must list ALL their IDs.

## Step 8 — Validate

```bash
REPORT=/tmp/kcli_oncall_report.md
for s in "## 1. Summary" "## 2. Graphs" "## 3. Operational Pain Level" \
         "## 4. Large Scale Significant Events" "## 5. Previous Week's Action Items" \
         "## 6. Page Log" "## 7. Open Sev2s" "## 8. Security Risks" \
         "## 9. Tickets Cut to Other Teams" "## 10. Dashboard Review"; do
  grep -qF "$s" "$REPORT" || echo "MISSING SECTION: $s"
done
while read id; do [ -z "$id" ] && continue; grep -q "$id" "$REPORT" || echo "MISSING ID: $id"; done < /tmp/kcli_oncall_sev2_ids.txt
grep -q "^# \[Kiro-CLI\] Weekly Ops Review - " "$REPORT" || echo "MISSING TITLE"
```

Fix anything reported. Confirm Section 2 `Total` == Section 1 Resolved, and Section 6 row
count == `metrics.pages` (or note the discrepancy).

## Step 9 — Write the report and open a review PR

If `dry_run` is `true`: print `/tmp/kcli_oncall_report.md` to stdout and STOP — do not write
a file, create a branch, or open a PR.

Otherwise, write the report into the repo:

```bash
mkdir -p .ops/weekly-reviews
cp /tmp/kcli_oncall_report.md .ops/weekly-reviews/{end_date}.md
```

Where `{end_date}` is the `YYYY-MM-DD` end date (e.g. `.ops/weekly-reviews/2026-06-15.md`).

**If `no_pr` is `true`:** STOP here — the file is written on the current branch and the
oncall can commit it however they like.

**Otherwise (default): open a review PR** so the team can comment during the ops meeting.
First confirm the working tree has no unrelated staged changes, then:

```bash
branch="ops/weekly-review-{end_date}"
git checkout -b "$branch"
git add .ops/weekly-reviews/{end_date}.md
git commit -m "ops: weekly ops review {end_date}"
git push -u origin "$branch"
gh pr create \
  --title "[Kiro-CLI] Weekly Ops Review - {MM/DD/YYYY}" \
  --label no-changelog \
  --body "Auto-generated weekly ops review for the {start_date} → {end_date} oncall week.

Review during the ops meeting by leaving PR comments — Sections 3 (Pain Level), 5 (Action
Items), 8 (Security Risks) and 10 (Dashboard Review) are filled live this way. Run
\`@apply-ops-review-comments {pr_number}\` to fold comments back in, then merge when finalized."
```

Capture the PR URL from `gh pr create`. Only commit the single report file
(`.ops/weekly-reviews/{end_date}.md`) — never stage unrelated changes. If the branch
already exists (a regenerate of the same week), `git checkout "$branch"`, overwrite the
file, and amend/append a commit instead of creating a duplicate branch.

## Step 10 — Report completion

Print a short summary (do NOT paste the full report): the PR URL (or local file path on
`no_pr`, or "dry run — not written" on `dry_run`); oncall week + current oncall; Pages,
Queue `x→y`, Incoming, Resolved, LSE count; counts of pages logged / open Sev2s / tickets
cut to other teams; any approximate (`~`) figures or warnings; and a reminder that
Sections 3, 5, 8, 10 are filled during the meeting via PR comments
(`@apply-ops-review-comments`).

## Step 11 — Apply review comments (separate run)

This step runs **on its own** (via `@apply-ops-review-comments`), after the ops meeting,
once reviewers have left comments on the PR from Step 9. It is NOT part of report
generation.

1. Resolve the PR: use the PR number/URL if given, else find the open PR whose head branch
   matches `ops/weekly-review-*` (`gh pr list --head ops/weekly-review-{end_date}` or
   `gh pr list --search "Weekly Ops Review"`).
2. Read **all** unresolved comments:
   - Conversation/issue comments: `gh pr view {pr} --json comments,reviews,title,headRefName`
   - Inline review comments (with file + line anchors):
     `gh api repos/{owner}/{repo}/pulls/{pr}/comments`
   Skip comments already marked resolved/outdated and your own prior fixup acknowledgements.
3. For each actionable comment, interpret it as an edit to
   `.ops/weekly-reviews/{end_date}.md` and apply it:
   - Inline comments map to the file/line they anchor to (often Sections 3/5/8/10 or a
     correction in a table row).
   - General comments name the section or change ("pain level is 4", "add action item X",
     "drop ticket Vxxx from section 9"). If a comment is ambiguous, leave the text as-is and
     note it in the completion summary rather than guessing.
4. Check out the PR branch, apply the edits, and push a fixup:
   ```bash
   gh pr checkout {pr}
   # edit .ops/weekly-reviews/{end_date}.md
   git add .ops/weekly-reviews/{end_date}.md
   git commit -m "ops: apply review comments for {end_date}"
   git push
   ```
5. Optionally reply on the PR (`gh pr comment {pr}`) summarizing what was applied and
   listing any comments that need human clarification. Do NOT resolve reviewers' comment
   threads yourself; let them confirm. Do NOT merge the PR — the team merges when finalized.

## Prohibited

- Do NOT write a file, create a branch, or open a PR when `dry_run` is true.
- Do NOT commit or stage anything other than `.ops/weekly-reviews/{end_date}.md` — never
  bundle unrelated working-tree changes into the report commit.
- Do NOT merge the review PR — the team merges it when the report is finalized.
- Do NOT resolve reviewers' comment threads on their behalf when applying comments.
- Do NOT push to `main`/`master`; the report always lives on its `ops/weekly-review-*` branch.
- Do NOT leave auto-populated fields blank; do NOT assume ticket details without fetching.
- Do NOT group unrelated tickets or use a representative ticket for a group — list every ID.
- Do NOT hold raw ticket responses in context; do NOT skip any Sev2 ID; do NOT produce a
  partial report.

## Examples

```
generate the oncall report
```
(no dates → most recently completed oncall week; writes `.ops/weekly-reviews/` and opens a review PR)
```
generate the Kiro CLI weekly ops review for 2026-06-08 to 2026-06-15
```
```
write the weekly oncall report for 2026-06-08 to 2026-06-15, oncall_alias girpooja
```
```
generate the oncall report, no pr
```
(write the file on the current branch without opening a PR)
```
dry-run the weekly ops review for 2026-06-08 to 2026-06-15
```
```
apply the ops review comments on PR 3145
```
(fold reviewer PR comments back into the report and push a fixup — see Step 11)

## Tips

- You only need `start_date` and `end_date`; everything else defaults sensibly.
- Oncall week is 9 AM PST → 9 AM PST; convert to UTC by adding 8h (`17:00:00Z`); do NOT
  adjust for DST.
- Provide `previous_report_url` for an accurate starting-queue figure when the previous
  week's report hasn't been generated yet.
- The review PR is the meeting surface: drop comments during the ops review, run
  `@apply-ops-review-comments` to fold them in, then merge when finalized. Use `no_pr` to
  skip the PR and just drop the file on the current branch.
