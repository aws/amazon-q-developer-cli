---
name: weekly-ops-review
description: Generate the weekly "[Kiro-CLI] Weekly Ops Review" oncall report for the Amazon Q for CLI resolver group and write it to .ops/weekly-reviews/ in the repo. Use when asked to create/write the weekly oncall report, ops review, or ops meeting doc. Triggers on "weekly ops review", "oncall report", "ops review doc", "weekly report".
---

# Kiro CLI Weekly Ops Review — Report Generation SOP

Generate the **`[Kiro-CLI] Weekly Ops Review`** report for the `Amazon Q for CLI`
resolver group, write it to **`.ops/weekly-reviews/`**, and open a review PR so the team
can comment on it during the ops meeting.

- Output: `.ops/weekly-reviews/YYYY-MM-DD.md` (where `YYYY-MM-DD` is the `end_date`)
- Template: `.ops/weekly-reviews/TEMPLATE.md` (the canonical 11-section layout to fill in)
- Chart assets: `.ops/weekly-reviews/artifacts/` (date-prefixed, e.g.
  `artifacts/2026-08-03-release-rate.svg`, mirroring the report filename)
- Example: `.ops/weekly-reviews/2026-06-15.md`
- Review branch: `ops/weekly-review-YYYY-MM-DD` → PR titled `[Kiro-CLI] Weekly Ops Review - MM/DD/YYYY`

The report has **11 sections** (Step 7). Ticket data is pulled directly from the
ticketing system; paging data comes from Builder Insights (Step 2). By default the report is
committed to a dedicated branch and a PR is opened (Step 9); reviewers leave comments during
the meeting, and `@apply-ops-review-comments` (Step 11) folds them back in. Merge the PR when
finalized.

## Team Constants

- **Resolver / assigned group:** `Amazon Q for CLI` (`extensions.tt.assignedGroup:"Amazon Q for CLI"`)
- **CTI:** `Kiro / CLI / Intake`
- **Oncall team name:** `amazon-q-cli-primary` (the full name — `amazon-q-cli` works for
  `search-teams` but returns an empty result for `get-team-shifts`)
- **Primary paging aliases:** `page-amazon-q-cli-primary@amazon.com` **and**
  `page-cw-jupiter@amazon.com`. Both are `PAGE` aliases on the same team routing to the same
  person; `page-cw-jupiter` has no `resolverGroups` entry, so filtering on that field drops
  it. Harvest them at runtime (Step 1) rather than trusting this list.
- **Escalation tier:** `page-kiro-gm-delegates` carries `Amazon Q for CLI` at
  `supportOrder: 2`. Pages to it are manager escalations, reported separately from `pages`.
- **Handoff:** 09:30 `America/Los_Angeles`, DST-aware (see Parameters)
- **Ticket Queue saved query** (Section 1 link):
  `https://t.corp.amazon.com/issues/?q=extensions.tt.status%3A%28Assigned%20OR%20Researching%20OR%20%22Work%20In%20Progress%22%20OR%20Pending%29%20AND%20extensions.tt.assignedGroup%3A%22Amazon%20Q%20for%20CLI%22`
- **Previous Week's Action Items (Section 5):** `https://tiny.amazon.com/1auvbeoty/taskamazdevroom7c22task`
- **Dashboard Review notes (Section 11):** `https://quip-amazon.com/umwaAzDXcFo1`
- **Reports directory:** `.ops/weekly-reviews/` in the repository root. Every weekly report
  is written here and read from here to find the prior week's report (for the Section 1
  starting queue). This directory is the source of truth.

## Parameters

- **start_date** (optional): `YYYY-MM-DD`. Oncall week starts at the **09:30 handoff in
  `America/Los_Angeles`** → `start_iso`.
- **end_date** (optional): `YYYY-MM-DD`. Oncall week ends at the **09:30 handoff in
  `America/Los_Angeles`** → `end_iso`. The report title uses this date as `MM/DD/YYYY`.

  **The handoff is 09:30 local Pacific time and IS subject to DST.** Verified from
  `OncallReadActions search-teams` → `oncallDetails.shiftStart`, which returns e.g.
  `2026-08-03T09:30:00-07:00[America/Los_Angeles]`. In PDT that is `16:30:00Z`; in PST it
  is `17:30:00Z`. Do NOT hardcode `17:00:00Z` and do NOT assume 09:00 — both are wrong.
  A page can land seconds after a handoff (D499857327 paged at `16:30:03Z`, three seconds
  into the 2026-07-27 shift), so a boundary that is off by even 30 minutes silently drops
  real pages from the report. Always derive the boundary with a timezone-aware conversion:

  ```bash
  # start_iso / end_iso from the 09:30 America/Los_Angeles handoff, DST-aware
  iso() { python3 -c "
  import sys,datetime,zoneinfo
  d=datetime.date.fromisoformat(sys.argv[1])
  t=datetime.datetime.combine(d,datetime.time(9,30),zoneinfo.ZoneInfo('America/Los_Angeles'))
  print(t.astimezone(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'))" "$1"; }
  start_iso=$(iso "$start_date"); end_iso=$(iso "$end_date")
  echo "Window: $start_iso -> $end_iso"
  ```

  If the shift boundary ever differs from 09:30 in the `shiftStart` response, trust the
  API and say so in the completion summary.

  **If dates are omitted** (e.g. the user just says "generate the oncall report"), default
  to the **most recently completed oncall week**: the oncall week runs Monday 09:30
  `America/Los_Angeles` → the following Monday 09:30, so `end_date` = the most recent Monday
  whose 09:30 handoff has already passed (which is **today** when run on a Monday after 09:30
  PT, not the previous Monday) and `start_date` = `end_date − 7 days`. Compute this with
  `shell` using the current date (e.g. `date`), do NOT hardcode. Print the resolved week
  back to the user before proceeding so they can correct it if they meant a different week.
- **oncall_alias** (optional): primary oncall for the week (Section 9 security links).
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
> /tmp/kcli_oncall_sev2.jsonl       # high-sev (min-sev 2) ticket details
> /tmp/kcli_oncall_sev2_ids.txt     # deduped high-sev display IDs (V/P/D)
> /tmp/kcli_oncall_incoming.jsonl   # raw incoming tickets (for dedup → Section 1 Incoming)
> /tmp/kcli_oncall_resolved.jsonl   # resolved tickets (Section 1 Resolved + Section 2)
> /tmp/kcli_oncall_open_sev2.jsonl  # currently-open Sev2s (Section 7)
> /tmp/kcli_oncall_partner.jsonl    # tickets/CRs cut to other teams (Section 10)
> /tmp/kcli_oncall_queue.jsonl      # every currently-open ticket (Section 8 composition)
> /tmp/kcli_oncall_queue_cats.tsv   # category<TAB>display_id, one line per open ticket
> /tmp/kcli_oncall_pages_raw.jsonl  # one row per page NOTIFICATION from Builder Insights
> /tmp/kcli_oncall_pages.jsonl      # page log — one row per paged TICKET w/ page_count (Section 6)
> /tmp/kcli_oncall_releases.json    # stable release tags in window + trailing 4-week counts
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
# Oncall week: Monday 09:30 America/Los_Angeles → following Monday 09:30.
# end_date = the most recent Monday whose 09:30 handoff has ALREADY passed.
# On a Monday that hinges on the time of day, so this is handoff-aware, not date-only:
# after 09:30 PT the week that just closed ends today; before it, step back a full week.
end_date=$(python3 -c "
import datetime, zoneinfo
now = datetime.datetime.now(zoneinfo.ZoneInfo('America/Los_Angeles'))
wd = now.weekday()                                    # Mon=0
back = 7 if (wd == 0 and now.time() < datetime.time(9, 30)) else wd
print(now.date() - datetime.timedelta(days=back))")
start_date=$(python3 -c "import datetime;print(datetime.date.fromisoformat('$end_date')-datetime.timedelta(days=7))")
echo "Reporting week: $start_date -> $end_date"
```

Why this is not `days=(t.weekday()) or 7`: that idiom always steps back a full week on Monday,
so a report generated on handoff Monday after 09:30 silently covers the week *before* the one
that just closed. Running it interactively the printed window catches this; unattended it would
not. Every other day of the week both forms agree.

Print the resolved week to the user (e.g. "Generating the oncall report for 2026-06-08 →
2026-06-15") so they can correct it before the run continues. Then derive `start_iso` and
`end_iso` with the **DST-aware `iso()` helper in Parameters** (09:30 `America/Los_Angeles`,
NOT a hardcoded `17:00:00Z`), and the title date `{end_date}` as `MM/DD/YYYY`.

## Step 1 — Resolve oncall schedule and paging aliases

Use `@builder-mcp/OncallReadActions` to resolve current / next / previous primary oncall
for `Amazon Q for CLI`:

- `search-teams` query `amazon-q-cli` → returns the team `amazon-q-cli-primary`. Each
  entry in `aliases[]` carries `oncallDetails.currentOncalls`, `shiftStart` and `shiftEnd`.
  `shiftStart` on the alias whose `resolverGroup` is `Amazon Q for CLI` gives the shift
  beginning on `end_date` (the **next** oncall) and is the authoritative handoff timestamp.
- `get-team-shifts` needs `teamName: "amazon-q-cli-primary"` — the **full team name**, not
  `amazon-q-cli`. Passing `amazon-q-cli` returns an empty buffer with a success status,
  which looks like "no shifts" rather than an error. Use `startDate={start_date − 7d}`,
  `endDate={end_date + 7d}` to identify current-week and previous-week oncall.

`oncall_alias` (if provided) overrides the resolved current oncall. If lookup fails, use
`TBD` and continue (do not hard-fail).

**Harvest the paging aliases from this same response** — do NOT hardcode them. Record every
alias with `aliasType: "PAGE"` on the `amazon-q-cli-primary` team as
`oncall_page_aliases`. As of 2026-08 that is `page-amazon-q-cli-primary` (whose
`resolverGroups` lists `Amazon Q for CLI` at `supportOrder: 0`) and `page-cw-jupiter`
(whose `resolverGroups` is `null`). Both route to the same person, so select on
`aliasType == "PAGE"` and NOT on the presence of a `resolverGroups` entry, or you will drop
`page-cw-jupiter` and undercount. In the 2026-08-03 week that alias alone carried 3 pages.

**Escalation tiers are a different team.** `page-kiro-gm-delegates` also lists
`Amazon Q for CLI` in its `resolverGroups`, at `supportOrder: 2`. Pages to it are manager
escalations, not primary-oncall load. Record its members as `escalation_recipients` and
report those pages separately in Section 6 rather than folding them into `pages`
(see Step 2). Any team whose `supportOrder` for `Amazon Q for CLI` is greater than 0 is an
escalation tier.

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
     responseFields: ["id","aliases","title","currentSeverity","extensions.tt.rootCause","lastResolvedDate"]
     ```
   - **(b) CLI-CTI resolved across ALL groups** — catches CLI tickets the oncall drove to
     closure that live in adjacent groups:
     ```
     query: 'extensions.tt.category:"Kiro" AND extensions.tt.type:"CLI" AND status:(Resolved OR Closed)'
     lastResolvedDate: "[{start_iso} TO {end_date}T23:59:59Z]"
     rows: 100
     responseFields: ["id","aliases","title","currentSeverity","extensions.tt.assignedGroup","extensions.tt.rootCause","lastResolvedDate"]
     ```
   Union (a)+(b), dedupe by `display_id`, write to `/tmp/kcli_oncall_resolved.jsonl`.
   `resolved = distinct count`. This list also feeds the Section 2 root-cause tables.
   Split it by severity for Section 2: `sev2_resolved` = rows whose current severity is
   Sev2 (or higher), `non_sev2_resolved` = the rest. `currentSeverity` must be in
   `responseFields` or the field is absent from the response and the split silently
   collapses; `search-tickets` rejects the literal `"severity"` there.

**Starting queue `x` — auto-find the previous report:**
1. Look for `.ops/weekly-reviews/{start_date}.md` (the prior week's end date = this week's
   start date). Read that file and take its Section 1 **ending** queue (the `y` in `x → y`)
   — that is this week's `x`.
2. If `previous_report_url` was passed, read it directly instead of searching.
3. Only if no previous report can be found, estimate `x = y − incoming_raw + resolved` and
   prefix it with `~`.

**Pages — count notifications delivered to the primary oncall during the week, and record how many each ticket produced:**

`Pages` is the oncall's interrupt load: the number of paging notifications delivered to the
person who held the primary rotation, inside `[start_iso, end_iso]`. A ticket that paged 4×
contributes **4** to `pages` and **1** to `paged_tickets`, and gets ONE row in Section 6
carrying a `Pages` count of 4 (Step 5).

**Do NOT try to reconstruct pages from ticket threads.** Paging notifications are emails.
They are never written into the ticket. Verified on `P472355122`, a ticket with four known
notifications: `ANNOUNCEMENTS` empty, `SYNOPSIS` empty, `WORKLOG` one agent comment,
`AI_AUTOMATIONS` one AI comment, `CORRESPONDENCE` only human comments and SLA-bot notices;
the web UI's combined thread is 18 items, all of type `COMMENT`. Scanning threads for
`New Sev2` / `Re-page` / `Escalated` text finds nothing and silently falls back to counting
new Sev2 alarm tickets, which is how the 2026-08-03 report first reported 3 pages against
an actual 19. `get-report-instructions` also does not return page data (it returns generic
report guidance), so do not rely on it either.

### Primary source — Builder Insights Silver `PAGING / pages_dim_derived_v2`

This is the paging service's own event log, surfaced through the Nightingale data lake. One
row per notification. It is the authoritative source: it reproduced the paging website
exactly for the 2026-08-03 week (all 10 tickets, every per-ticket count) and additionally
captured escalations the oncall's own mailbox never received.

Load the `builder-insights` skill and use the `skillPath` it returns — do NOT hardcode the
path, it contains a package eventId that changes on update. Then:

```bash
BI=<skillPath>/scripts/bi-api.sh

# 1. Confirm the table and its CURRENT version (never assume the version)
bash $BI listSilverDatasets '{"domains":["PAGING"]}'

# 2. Confirm the schema and that page_create_timestamp is a valid date column
bash $BI describeSilverDataset '{"datasetIdentifier":{"domain":"PAGING","tableName":"pages_dim_derived_v2","version":"<version>"}}'

# 3. Submit the async query (returns a queryId immediately)
bash $BI startQuerySilverData '{
  "datasetIdentifier":{"domain":"PAGING","tableName":"pages_dim_derived_v2","version":"<version>"},
  "leaderLogin":"<manager_alias>",
  "dateRange":{"dateColumn":"page_create_timestamp",
               "startDate":"{start_date}","endDate":"{end_date}"},
  "rowLimit":200
}'

# 4. Poll with backoff (~20s, then 30s/45s/60s+). Queries can run 10+ minutes.
bash $BI getQuerySilverDataResults '{"queryId":"<queryId>","maxResults":1000}'
```

Columns returned per row: `page_create_timestamp`, `recipient`, `case_id`, `is_primary`,
`is_recipient_oncall`, `is_recipient_builder`, `is_outside_work_hour`, `is_during_weekend`,
`is_send_from_sim`, `ticket_resolver_group`, `ticket_title_text`, `ticket_root_cause`,
`ticket_status`, `ticket_cti`.

**Submit this query FIRST, before any other data collection**, and poll it while the rest of
the report is being built. It is async and slow, so blocking the whole run on it wastes the
window. Values are string-encoded (`Map<String,String>`) and NULLs are absent keys, so parse
by the declared type rather than assuming presence.

**`leaderLogin` must be a manager (L8 or below); ICs are rejected.** Resolve it at runtime,
never hardcode it, or the skill breaks the moment the rotation changes hands:

```bash
bash $BI getUser '{"alias":"<oncall_alias>"}'   # → supervisorLogin
```

Use `supervisorLogin` as `leaderLogin`. Row-level security scopes results to that leader's
org, so the oncall must be inside it (they are, since it is their own manager).

**Filter the returned rows yourself. Three filters, in this order:**

1. `ticket_resolver_group == "Amazon Q for CLI"` — scopes to our group. `leaderLogin` alone
   returns the manager's whole org across every resolver group.
2. `page_create_timestamp ∈ [start_iso, end_iso]` — **required**, not optional.
   `startDate`/`endDate` in the query accept only `YYYY-MM-DD`, so the API returns whole
   calendar days and always over-selects at both ends of the 09:30 handoff. Compare against
   the DST-aware boundary from Parameters.
3. `recipient == <oncall_alias>` for the headline `pages` figure.

**Filter on `recipient`, NOT on `is_primary`.** `is_primary` looks like the tidier filter but
it is a rotation snapshot and lags the handoff. In the 2026-08-03 week it returned 18 instead
of 19, because D499857327's page at `16:30:00.799Z` landed 0.8s into the shift before the
snapshot flipped, and was recorded `recipient=abhraina, is_primary=false`. `recipient`
reproduces the paging website exactly, so the report stays verifiable against something a
human can eyeball, and it does not depend on snapshot timing at the contested week boundary.

Then:
- `pages` = number of rows surviving all three filters.
- `paged_tickets` = distinct `case_id` among them.
- `pages_outside_work_hours` = rows with `is_outside_work_hour == true` (a better pain
  signal than the raw count; report it in Section 1 or Section 3 when non-zero).
- **Escalations**, reported separately: rows passing filters 1 and 2 whose `recipient` is
  NOT the oncall. These are the `supportOrder > 0` tiers from Step 1. Do NOT fold them into
  `pages` — they measure escalation reach, not oncall burden, and mixing them makes the
  headline number unverifiable against any single source. In the 2026-08-03 week this was
  5 manager escalations on top of 19 oncall pages, each firing 20 to 30 minutes after the
  oncall's page on the same ticket, which is the auto-escalation ladder behaving normally.
  Note that managers carry `is_recipient_builder=false`.

### Optional cross-check — `opshealth / page_count`

`batchGetMetricData` on `opshealth / page_count` returns a page total instantly with no async
wait, which is a cheap sanity check on the Silver row count. Two caveats: it is aggregated by
`leader_login` only and **cannot be sliced by resolver group**, and its `report_week` runs
Sunday to Saturday, which never aligns with the 09:30-Monday oncall window. Treat a close
match as corroboration, not equality.

### Fallback — the `engagements` array

If Builder Insights is unavailable (no access, query `FAILED` twice, back-filling a week
outside the retention window), fall back to `get-ticket`'s `engagements[]` on the Step 3
ticket set. A ticket paged the oncall if any engagement's `engagedEmailAddress` matches an
`oncall_page_aliases` entry from Step 1 with `lastUpdatedAt` inside the window.

This yields the **ticket list but not the counts**, so it is degraded but still correct. Set
`pages = paged_tickets` and say explicitly in Section 6 and the completion summary that
per-ticket counts were unavailable. The reason counts are unrecoverable this way is that
`engagements[]` is **overwritten in place**, one row per alias holding only the latest
`reason` and `lastUpdatedAt`: `P472355122` shows a single `ROTATED` row for
`page-cw-jupiter` at its *third* rotation, and the two earlier ones are gone. A
`lastUpdatedAt` after `end_iso` is therefore lossy — the last touch was outside the window
but an earlier one may have been inside it. Flag those tickets rather than silently
dropping or counting them.

What does NOT work, so nobody re-derives it: the paging portal at `paging.corp.a2z.com` is a
JS SPA and returns an empty shell to `ReadInternalWebsites` (`/api/pages` is 404); the SOS
API permits only `CreateEngagement` and `DescribeEngagement` for the Amazon tenant, with no
list operation; and the oncall's mailbox via `aws-outlook-mcp email_search` works but only
for the person who was paged, misses sibling-alias rotations, and cannot back-fill a prior
oncall's week.

**Releases shipped (Section 1 + the cadence chart):**

Count **stable** releases whose tag landed inside the window. Tags live in git, so this needs
no ticket data, but it does need the tags fetched first — a stale clone silently undercounts:

```bash
git fetch --tags origin
```

Three rules that each caused a wrong number when skipped:

1. **Match strict semver only:** `^v[0-9]+\.[0-9]+\.[0-9]+$`. A filter that merely excludes
   `nightly`/`beta`/`rc` still admits feature-branch tags such as
   `v2.8.1-external-idp-refresh-scope.1`, `v2.9.0-rpm-build.1` and
   `v2.9.0-usage-credit-pooling-overages.2`, inflating the count.
2. **Bucket by tag timestamp against the 09:30 handoff, not by calendar date.** `v2.15.0` was
   tagged 2026-07-26 17:00 PDT, which is the *previous* oncall week even though its calendar
   date sits adjacent to the 2026-07-27 window. Date-only bucketing put it in the wrong week
   and produced 4 releases instead of 3. Use `creatordate` and compare to `start_iso`/`end_iso`.
3. **Do not use Python's `round()` for the average.** It is banker's rounding, so
   `round(3.25, 1)` returns `3.2`, not `3.3`. Format explicitly (e.g.
   `f"{v:.1f}"` after a `Decimal`/half-up adjustment) or the trailing average is off by 0.1.

`release_count` = tags in `[start_iso, end_iso]`; `release_versions` = their names in tag
order; `release_avg` = mean per week over the trailing 4 oncall weeks (for the week ending
2026-08-03 those were 3, 5, 3, 2 → 13/4 → `3.3`).

Then write an SVG bar chart of per-week release counts to
`.ops/weekly-reviews/artifacts/{end_date}-release-rate.svg` and reference it from Section 1.
Emit the SVG text directly (no plotting dependency); keep it small and dark-theme consistent
(background `#19161D`, text `#ffffff`, muted `#938f9b`). Include a comment in the SVG stating
the derivation method and the reliable-data start date.

**Reliable tag data starts 2026-05-01.** Earlier months used a `chat`-prefixed tagging scheme
and are not comparable, so do not chart or average across that boundary. State this caveat
under the chart via `{release_caveat}` rather than presenting an unqualified long-term trend.

Write `/tmp/kcli_oncall_metrics.json`:
`{"pages":N,"paged_tickets":N,"escalation_pages":N,"pages_outside_work_hours":N,"queue_start":N,"queue_end":N,"incoming":N,"incoming_raw":N,"resolved":N,"sev2_resolved":N,"non_sev2_resolved":N,"release_count":N,"release_avg":N,"lse_count":N,"queue_start_source":"prev-report|estimate","pages_source":"builder-insights|engagements-fallback"}`
(`lse_count` filled in Step 6).

## Step 3 — Fetch high-severity (Sev2) tickets

Run **two** `search-tickets` calls and union the **display IDs** (from `aliases`, i.e.
`V…`/`P…`/`D…`) into `/tmp/kcli_oncall_sev2_ids.txt` (`sort -u`). These display IDs are
what appears in the report and what Step 8 validates against; `get-ticket` accepts the
display ID as its `ticketId`.

**Use `minimumSeverity: 2`, NOT `currentSeverity`.** `minimumSeverity` matches on the highest
severity a ticket ever reached, so it retains tickets that paged at Sev2 and were later
downgraded. `currentSeverity: ["1","2","2.5"]` drops them. Measured on the 2026-08-03 week:
`currentSeverity` returned 26 tickets and found 9 of the 10 that paged, missing `D499857327`
(`severity: SEV_3`, `minimumSeverity: SEV_2`) which had taken 3 pages — the single worst
ticket to lose. `minimumSeverity: 2` returned 45 and found all 10. The cost is ~78% noise,
which the Step 2 paging filter and the Section 7 status filter remove anyway.

**(a) keyed on `lastUpdatedDate`** — this is the one that matters and the one that was
previously described in prose but never written out as a query, so it got skipped and cost the
2026-08-03 report 5 of its 8 then-known pages:

```
assignedGroup: ["Amazon Q for CLI"]
minimumSeverity: 2
status: ["Assigned","Researching","Work In Progress","Pending","Resolved","Closed"]
lastUpdatedDate: "[{start_iso} TO {end_iso}]"
rows: 100
responseFields: ["id","aliases","title","status","extensions","createDate","lastUpdatedDate"]
```

**(b) keyed on `createDate`** — strictly redundant, since creating a ticket is also an update
and every in-window created ticket verifiably appears in (a). Keep it as a cheap belt-and-braces
check and reconcile: if (b) surfaces an ID that (a) missed, the window arithmetic is wrong and
you should stop and re-derive `start_iso`/`end_iso` before continuing.

```
assignedGroup: ["Amazon Q for CLI"]
minimumSeverity: 2
status: ["Assigned","Researching","Work In Progress","Pending","Resolved","Closed"]
createDate: "[{start_iso} TO {end_iso}]"
rows: 100
sort: "createDate asc"
responseFields: ["id","aliases","title","status","extensions","createDate"]
```

Note the field-name asymmetry between operations, which silently returns nothing when
confused: `search-tickets` takes `currentSeverity`/`minimumSeverity` in the request and
returns severity under `extensions.tt`, while `get-ticket` returns top-level `severity` and
`minimumSeverity`. `search-tickets` also rejects `"severity"` in `responseFields` (use
`currentSeverity`) and rejects `"Open"` in `status` (the valid set is `Assigned`,
`Researching`, `Work In Progress`, `Pending`, `Resolved`, `Closed`).

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

When you fetch the ticket, also request the `SYNOPSIS` thread
(`threads: [...,"SYNOPSIS"]`) and capture its six fields when present — `impact_summary`,
`root_cause`, `mitigation`, `action_items`, `risk_of_recurrence`, `related_tickets` — into
the same JSON line. These feed the Section 6 per-ticket synopsis (Step 5). Most alarm and
customer-support tickets have no synopsis; leave the fields absent here and derive them in
Step 5 per the "Root cause & descriptions" fallback rule.

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
each, not the UUID. `currentSeverity` is correct here (unlike Step 3): Section 7 lists tickets
that are Sev2 *right now*, so a downgraded-but-open ticket does not belong.

Also derive a **`next_step`** for each open Sev2 — the concrete action the oncall or an
owning team is pursuing right now (e.g. "Waiting on backend fix in CR-XXX", "Awaiting
customer repro", "Retest after MCM lands", "Confirm alarm can be downgraded"). Pull it from
the ticket's most recent human worklog/correspondence (skip the automated authors listed in
Step 3); if nothing actionable is stated, use `TBD`.

**Read the ticket `description` before writing "pending root cause".** Automated security
findings (Heartwork, Cataphract, Delphi, AppSec) and detailed customer reports routinely ship
a complete root cause, a remediation plan and a deadline inside the description, so
"pending root cause analysis" is simply wrong for them and understates the work. `V2264387664`
carried a CRITICAL supply-chain root cause (a public-repo-controlled buildspec with read
access to the Linux release signing key), a five-step remediation, and a stated 14-day
post-CR-merge deployment expectation — yet was summarized as "automated security scan
findings requiring triage". If the description states a cause, use it, and make the
`next_step` the next remediation action with its deadline.

**Tickets cut to other teams (Section 10):**
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
  → `/tmp/kcli_oncall_partner.jsonl`.
- **Also include cross-team code reviews.** Most weeks the oncall drives a fix in another
  team's package via a CR rather than cutting them a ticket, and a ticket-only search reports
  `None` while real cross-team work happened. In the 2026-08-03 week the only tickets the
  oncall submitted were two of our own release tickets, while the actual cross-team items were
  `CR-292786335` (KiroTelemetryCDK) and `CR-292839013` (ToolkitTelemetryInfrastructure). List
  those as `{team} - [CR-XXXXXXXXX](https://code.amazon.com/reviews/CR-XXXXXXXXX)` with their
  approval state. Harvest CR links from the same worklog scan above.
- If genuinely nothing was cut and no cross-team CR was raised, Section 10 says `None`.

## Step 5 — Page Log (Section 6)

The Page Log has **one row per paged ticket**, carrying a `Pages` count of how many
notifications that ticket produced. Sort by `Pages` descending so the noisiest tickets lead.
A ticket that paged 4× is ONE row with `Pages = 4`, not four rows.

Build it from the filtered Builder Insights rows from Step 2 (or the engagements fallback).
Group the surviving notification rows by `case_id`; each group becomes one Section 6 row:

```json
{"display_id":"P472355122","page_count":4,
 "first_paged_at":"07-28 10:00",
 "page_sequence":"Rotated ×3, Reassigned",
 "notifications":[{"kind":"Rotated","recipient":"abhraina","at":"2026-07-28T10:00:00-07:00"}],
 "impact_summary":"…","root_cause":"…","mitigation":"…",
 "action_items":"…","risk_of_recurrence":"…","related_tickets":"…"}
```

Append one line per **ticket** to `/tmp/kcli_oncall_pages.jsonl`. Derive `page_count` and
`page_sequence` from the retained `notifications` array rather than authoring them separately,
so they cannot drift from the underlying data.

- `first_paged_at` — the earliest notification timestamp in the group, rendered in Pacific
  time as `MM-DD HH:MM` (the report is read by a Pacific-time audience and the handoff is
  defined in Pacific time).
- `page_sequence` — the notification kinds in chronological order, collapsing repeats with an
  `×N` multiplier: `Upgraded ×3, Reopened`, `Rotated ×3, Reassigned`, `New Sev2, Escalated`.
  Kinds come from the paging notification type, not the ticket status.

**Two invariants, both machine-checked in Step 8:**
- Section 6 row count == `metrics.paged_tickets` == distinct `case_id`.
- Sum of the `Pages` column == `metrics.pages`, and the table's `Total` row states it.

This is stricter than the previous one-invariant model, and it is why the `Pages` column is
worth carrying: it exposes distribution. In the 2026-08-03 week 3 tickets produced 11 of the
19 pages while 6 tickets paged once each, which is the actual finding for the meeting and is
invisible in a flat ticket list.

A paged ticket that is also still open appears in **both** Section 6 and Section 7. That is
correct and is not a duplicate — Section 6 is "paged this week", Section 7 is "open now".

**Per-ticket synopsis (same table).** Each row carries the six synopsis columns. Because
there is now exactly one row per ticket, derive the synopsis once and place it once — the old
"repeat the synopsis across a ticket's multiple page rows" step no longer applies.

Populate the six synopsis fields in this order of preference:

1. The ticket's `SYNOPSIS` thread (fetched in Step 2/3) — use its fields verbatim where
   present.
2. The ticket's **`description`**, structured fields, and human comments already captured in
   `/tmp/kcli_oncall_sev2.jsonl` — `resolution`, `rootCause`/`rootCauseDetails`,
   `closureCode`, worklog, and linked CR/MCM/Taskei/Sauron items.
3. Derive per the "Root cause & descriptions" fallback rule below (alarm name, region,
   linked artifacts).

**Do not inherit the `rootCause` field uncritically when the description contradicts it.**
The field is often set early and never revised. `V2277483794` carries
`rootCause: "Downstream Dependency Issue"`, but its description contains a full customer
root cause showing our own defect: Kiro unconditionally derives a `resource` parameter from
the MCP server URL and sends it to the Entra ID v2 authorize endpoint, which rejects it with
`AADSTS9010010` when scopes are set and `AADSTS900144` when they are not. Reporting that as a
downstream dependency misattributes our bug to a third party. When the description and the
field disagree, prefer the description and say what the evidence was.

Field guidance (keep each to ONE concise line — these are table cells, so NO pipes `|` and
NO line breaks): **Impact Summary** = who/what was affected and how; **Root Cause** = the
confirmed cause, else best current hypothesis; **Mitigation** = what stopped the bleeding /
the fix shipped; **Action Items** = concrete follow-ups (CR/MCM/Taskei/Sauron IDs) or
`None recorded`; **Risk of Recurrence** = `Low`/`Medium`/`High` + a short reason, else
`TBD`; **Related Tickets** = linked ticket display-ID links (backend/KAS/SOC/COE) or
`None`. Never leave a cell blank — use `TBD`, `None`, or `None recorded`.

**Call out shared infrastructure across paged tickets.** If two or more paged tickets name the
same pipeline, package or app bindle, state it in Section 3 or under Section 6 — it is a
stronger finding than either ticket alone. In the 2026-08-03 week `P472355122` and
`V2264387664` were both on the `FigIoDesktopDeploy` pipeline and together accounted for 5 of
the 19 pages, making the release pipeline the largest single source of paging load that week.

## Step 6 — Root-cause breakdown, LSEs, grouping

- **Section 2 (Resolved by Root Cause):** read `/tmp/kcli_oncall_resolved.jsonl` and split
  into **two tables, Sev2 first, then non-Sev2**, each grouped by `extensions.tt.rootCause`.
  Each row: `Root Cause | Count | Topic | Tickets` (list ALL ticket links by `display_id`).
  Drop the per-row severity column — the split makes it redundant.

  Severity here is the ticket's **current** severity, so a ticket that paged at Sev2 and was
  later downgraded lands in the non-Sev2 table. State that caveat under the heading (the
  template does), because otherwise it reads as a contradiction against Section 6: in the
  2026-08-03 week `D499857327` showed 3 Sev2 pages in Section 6 while sitting in the
  non-Sev2 resolved table, which is correct but needs one clause of explanation.

  **Every resolved ticket ID in `/tmp/kcli_oncall_resolved.jsonl` MUST appear in exactly one
  row of exactly one of the two tables** — do not silently drop tickets whose `rootCause` is
  empty or ambiguous; derive their bucket per the "Root cause & descriptions" rule below and
  add them. Each table's `Total` MUST equal both its row-count sum and its count of distinct
  ticket links; the two Totals MUST sum to the Section 1 Resolved number; and no ticket may
  appear in both tables. Step 8 validates all of these. If any fails, add the missing tickets
  (never remove rows to make a count match). A genuinely ticketless item the oncall resolved
  (e.g. a backend-capacity issue with "no explicit ticket") may be added as an extra row with
  an empty Tickets cell — but only in addition to, never in place of, a resolved ticket. When
  `rootCause` is empty, derive it per the rule below — do NOT write "Unknown" unless truly
  nothing is found. Record `sev2_resolved` and `non_sev2_resolved` in `metrics.json`.

  **Two buckets exist for administrative closures.** Do not assign a substantive engineering
  root cause to a ticket that was simply closed out:
  - **Stale Ticket Cleanup** — closures reading "No longer relevant", "Old ticket", or any
    bulk/aged cleanup where no root cause was ever established. Say so explicitly in the
    Topic cell rather than inventing a cause.
  - **Working as Intended** — reports confirmed as documented, by-design behavior.

  Skipping these inflates real signal. In the 2026-08-03 week two "Old ticket." / "No longer
  relevant" closures had been filed under Downstream Dependency Issue and Defect - Alarm
  Configuration, and a third closure about our own metric misclassification was filed as a
  backend dependency, overstating Downstream Dependency Issue as 6 when the real figure was 4.
  A backend-reliability number that is 50% administrative noise misleads the meeting.
  Also keep `Release` and `Stale Ticket Cleanup` visible as their own buckets so the share of
  purely administrative closures is legible (6 of 37 that week).

  Note the unit mismatch to guard against: the `Release` bucket counts release **tickets
  closed** during the window, which is not the same as releases **shipped** during it
  (Section 1). In the 2026-08-03 week 4 release tickets closed while 3 releases shipped,
  because `v2.15.0` was tagged before the window opened. State the distinction in the Topic
  cell so the two numbers do not read as a contradiction.
- **Section 4 (LSEs):** an LSE is a **formally declared Large Scale Significant Event** —
  not an internal alarm, not a transient backend hiccup, not a single-customer support
  ticket. Include an event ONLY if at least one of these is true:
  1. The ticket / linked ticket carries an explicit LSE designation
     (`extensions.tt.tags` contains `LSE`, has an LSE ticket ID linked in its worklog,
     is referenced from an LSE COE, or the correspondence explicitly says an LSE was
     declared / an LSE ticket was cut).
  2. A prod MCM/COE labeled as an LSE points at the incident.
  3. The team leadership explicitly declared it an LSE in worklog/announcements.

  If none of these apply, Section 4 is `* None` and `lse_count = 0`. Broad customer
  impact, cross-team coordination, or "big incident this week" ALONE do NOT qualify —
  those go in Section 2 / Section 10 as normal tickets. When in doubt, leave it out.
  Update `metrics.lse_count` to the number of qualifying events.
- **Grouping** (for Sections 2/4/6/7): group by same `extensions.tt.dedupeString` prefix, same
  alarm across regions, explicitly linked tickets, or same root cause. Never group
  unrelated tickets; every ticket ID stays individually traceable.

### Root cause & descriptions (applies to Sections 2, 6, 7, 10)

When you need a root cause or a description for a ticket and the structured field is empty
or unhelpful, you MUST try, in order, before falling back to "Unknown":

1. **Read the ticket's `description` field.** This is the highest-yield source and the one
   most often skipped. Automated security findings (Heartwork, Cataphract, Delphi, AppSec)
   embed a complete root-cause analysis, a categorized CWE, a remediation plan and a
   deadline. Detailed customer reports frequently include version-by-version testing, exact
   error codes, and the customer's own diagnosis. Both mischaracterizations in the
   2026-08-03 report came from summarizing the title instead of reading the description.
2. **Treat the structured `rootCause` field as a claim, not ground truth.** It is often set
   early and never revised, and it can contradict the description outright — `V2277483794`
   was labeled `Downstream Dependency Issue` while its description documented our own MCP
   OAuth defect. When they disagree, prefer the description and state the evidence.
3. **Use the ticket's correspondence** already captured in `/tmp/kcli_oncall_sev2.jsonl` —
   read the human comments/worklog, the `resolution` text, and `closureCode`. The cause is
   very often stated by the oncall in a resolve/worklog comment (e.g. P454617284's resolve
   comment "Cut a Sev2 to the toolkit telemetry team").
4. **Re-fetch the ticket** with `@builder-mcp/TicketingReadActions action=get-ticket` to
   read the full comment thread if the cached extract is insufficient.
5. **Infer from the alarm/title + linked artifacts** — the alarm name, region, model, or a
   linked CR/MCM/PR in the comments usually makes the cause clear (e.g.
   `…CacheHitRate…Critical` → "Cache-hit-rate alarm, backend-owned"; a linked hotfix PR →
   the defect it fixed).
6. **Briefly investigate** if still unclear — read a referenced CR/PR/COE/wiki via
   `@builder-mcp/ReadInternalWebsites` or search with `@builder-mcp/InternalSearch`.

Only write `Unknown` (or a one-line "no root cause recorded; <what you checked>") if all of
the above genuinely yield nothing. Never default to "Unknown" just because the structured
`rootCause` field was blank.

**Do not soften a severe finding into a generic one.** If the ticket documents a critical
risk, name it. "Automated security scan findings requiring triage" for a supply-chain
compromise path is an accuracy failure, not brevity, and it hides the single most important
item in the report. Preserve the stated severity, the affected asset, and any deadline.

**Attribute to the right party.** Do not record our own defect as a downstream dependency,
and do not assert a mechanism the owning team has not confirmed. For incidents owned by
another team, source the root cause from what that team publicly stated.

### Open queue composition (Section 8)

This is a **snapshot of what is in the queue right now**, not a trend. The queue is
re-classified from scratch every week, so category counts are not comparable week over week
and the section must not state deltas or "up/down from last week".

**(a) Fetch every open ticket.** All severities, open statuses only. Paginate on `start`
(100, 200, …) until you have `totalCount` rows, and write one JSON line per ticket to
`/tmp/kcli_oncall_queue.jsonl`. Report only the count you actually paginated to.

```
assignedGroup: ["Amazon Q for CLI"]
status: ["Assigned","Researching","Work In Progress","Pending"]
rows: 100
sort: "createDate desc"
responseFields: ["aliases","title","extensions.tt.status","createDate"]
```

Do NOT pass `currentSeverity` in `responseFields` here — it is not present on these documents
and every row returns a `field_not_present` error alongside the data, which is pure noise.

**(b) Classify each ticket into exactly one category.** Delegate this to a subagent so the
titles do not land in the main context: hand it `/tmp/kcli_oncall_queue.jsonl` and the
definitions below, and have it write `category<TAB>display_id` lines to
`/tmp/kcli_oncall_queue_cats.tsv` and return only the per-category counts.

| Category | Covers |
|---|---|
| Security and trust boundary | Anything filed by a security org (AppSec, Bug Bounty, SOC, SIRT, CorpSec, Delphi/Cataphract) plus permission-engine and trust-boundary defects: command trust matching, permission derivation, tool parameter validation, symlink and sandbox bypass, prompt injection, supply-chain and artifact integrity, container base images |
| MCP servers and tools | Server registration and discovery, transport reliability, tool-surface consistency, tool-name handling, `tools/call` lifecycle, governance gating of MCP |
| TUI and terminal | Rendering and input handling: themes and colour, viewport and scroll, key decoding, cursor, tmux behaviour, what the UI shows or hides at approval time |
| Auth and connectivity | Login, token and transport failures: SSO, OAuth, IdP, `profileArn`, bearer tokens, proxy and firewall blocking, `dispatch failure`, headless and service identity |
| Platform and packaging | Install, update and OS integration: distro support, SSH config generation, auto-update, shell integration, per-architecture bundles, runtime provisioning |
| Stability and crashes | Process death and payload limits: signals, panics, oversized tool results or responses, media dimension limits, transcript and `tool_use`/`tool_result` invariant damage |
| Context and model behaviour | Compaction, context-window accounting and visibility, effort and thinking controls, multimodal accounting, session export integrity, fabricated tool calls |
| Subagents and hooks | Orchestration of subagents and lifecycle hooks: context injection, hook-induced transcript corruption, per-stage overrides, agent composition and inherited config |
| Compliance and measurement | Externally committed obligations and reporting: accessibility remediation deadlines, certification and security questionnaires, usage and efficiency reporting |
| Release infra and access | Release pipeline and repo/org access: deploy Lambdas, artifact pathing, GitHub org and permission sync |
| Other | Anything that genuinely does not fit a category above. **List the ticket IDs in this row.** One or two tickets here is normal. A cluster of three or more sharing a theme means the taxonomy has stopped describing the product |

Tie-breaks, applied in this order:

1. **Security wins.** If a security org filed it, or it describes an escape from an intended
   permission or trust boundary, it is Security regardless of which subsystem it lives in. A
   permission bypass reached through a subagent is Security, not Subagents.
2. **Obligation wins over subsystem.** A committed external deadline or a certification
   response is Compliance even when the underlying defect is, say, TUI.
3. Otherwise classify by **the subsystem that owns the fix**, not where the symptom appeared.
   When two still apply, take the more specific.

One primary category per ticket, so the counts sum to the open total. Cross-cutting themes
belong in **Observations**, not in a second category.

**Taxonomy staleness — the skill flags it, a human fixes it.** `Other` is both the escape
hatch and the health check on these ten buckets, which were derived from a point-in-time
triage and will age as the product moves. Never force a ticket into a category it does not
belong in just to keep `Other` empty; a mis-filed ticket hides the signal that a new bucket is
needed.

If `Other` exceeds **10% of the open queue**, Section 8's Observations MUST include a bullet
beginning `**New category needed:**` that, for each cluster of three or more `Other` tickets
sharing a theme, gives the proposed category name, a one-line scope description, and the
ticket IDs. Step 8 enforces this. Do NOT invent a category and start using it — propose it and
let a human promote it into the table above, so the vocabulary stays stable and the validation
keeps working. Clusters of one or two are not a proposal; say nothing.

**(c) Derive the Observations.** These are the part with review value, so they must be
re-derived each week and not copied from a previous report — clusters resolve, accounts churn,
duplicate pairs get determinations. Look for: several tickets sharing one root cause, the same
customer or account across multiple tickets, unresolved duplicate pairs, and tickets aged well
beyond the rest of the queue. Name the ticket IDs and say what single action would clear each
group. If none of these hold this week, say the queue is genuinely disjoint rather than
inventing a theme.

## Step 7 — Assemble the Markdown report

Start from the canonical template at **`.ops/weekly-reviews/TEMPLATE.md`** — read it,
fill in every `{placeholder}`, and write the result to `/tmp/kcli_oncall_report.md`. The
template defines exactly these 11 sections; do not add, remove, or reorder them.

Formatting rules: use Markdown tables (header + `|---|` row, no blank lines inside tables).
Bullet lists use `*` with a blank line between a label and its first item. Ticket links
MUST use the human-readable **display ID** (`display_id` — `V…`/`P…`/`D…`), never the
internal UUID: `[<display_id>](https://t.corp.amazon.com/<display_id>)` (e.g.
`[V2239674541](https://t.corp.amazon.com/V2239674541)`). Throughout the template, `{id}`
means the display ID. Never leave auto-populated fields blank — use real data, `None`, or
`Unknown`.

Sections 3, 5, 9 and 11 keep their standing placeholders/links (filled live during the
meeting). **Section 8 (Open Queue by Category)** is filled from
`/tmp/kcli_oncall_queue_cats.tsv` per Step 6: one row per category with a non-zero count,
sorted by count descending, omitting empty categories, and a bold `Total` row equal to the
number of open tickets you paginated to. Shares are whole percentages of that total. The
provenance note states the open count, that classification was done fresh this week, and why
the total may differ from the Section 1 week-ending queue figure (Section 1 is measured at the
handoff boundary, Section 8 when the report runs). Grouped tickets must list ALL their IDs.

Section 2 is TWO tables (Sev2 then non-Sev2). Section 6 is a SINGLE wide table with **one row
per paged ticket**, a `Pages` count column, `First Page (PT)`, `Page Sequence`, the six
synopsis columns, and a bold `Total` row stating `metrics.pages`, all from
`/tmp/kcli_oncall_pages.jsonl` sorted by page count descending.

Write the release chart to `.ops/weekly-reviews/artifacts/{end_date}-release-rate.svg` before
assembling, since Section 1 references it by relative path and a missing file renders a broken
image in the PR.

## Step 8 — Validate

```bash
REPORT=/tmp/kcli_oncall_report.md
END_DATE={end_date}
CHART=.ops/weekly-reviews/artifacts/${END_DATE}-release-rate.svg

# Section headings. Section 8's heading carries a dynamic count, so match the PREFIX only.
for s in "## 1. Summary" "## 2. Graphs" "## 3. Operational Pain Level" \
         "## 4. Large Scale Significant Events" "## 5. Previous Week's Action Items" \
         "## 6. Page Log" "## 7. Open Sev2s" "## 8. Open Queue by Category" \
         "## 9. Security Risks" "## 10. Tickets Cut to Other Teams" \
         "## 11. Dashboard Review"; do
  grep -qF "$s" "$REPORT" || echo "MISSING SECTION: $s"
done
[ "$(grep -c '^## ' "$REPORT")" = "11" ] || echo "SECTION COUNT != 11"

while read id; do [ -z "$id" ] && continue; grep -q "$id" "$REPORT" || echo "MISSING ID: $id"; done < /tmp/kcli_oncall_sev2_ids.txt
grep -q "^# \[Kiro-CLI\] Weekly Ops Review - " "$REPORT" || echo "MISSING TITLE"

# Section 1 must carry the release line, and the referenced chart must exist on disk.
grep -q "^\* Releases Shipped: " "$REPORT" || echo "SECTION 1 MISSING 'Releases Shipped'"
grep -qF "artifacts/${END_DATE}-release-rate.svg" "$REPORT" || echo "SECTION 1 MISSING CHART REFERENCE"
[ -s "$CHART" ] || echo "MISSING OR EMPTY CHART FILE: $CHART"

# Section 7 must include the Next Step column.
grep -q "^| # | Ticket | Description | Next Step | ETA To Resolve |" "$REPORT" \
  || echo "SECTION 7 MISSING 'Next Step' COLUMN"

# Section 6 must be the per-ticket table with the Pages count column.
grep -q "^| # | Ticket | Pages | First Page (PT) | Page Sequence | Impact Summary | Root Cause | Mitigation | Action Items | Risk of Recurrence | Related Tickets |" "$REPORT" \
  || echo "SECTION 6 WRONG HEADER (expected per-ticket table with Pages column)"

python3 - "$REPORT" <<'PY'
import json, re, sys
report = open(sys.argv[1]).read()
m = json.load(open('/tmp/kcli_oncall_metrics.json'))

def section(n):
    mm = re.search(rf'^## {n}\..*?(?=^## |\Z)', report, re.S | re.M)
    return mm.group(0) if mm else ''

# --- Section 2: two tables, Sev2 then non-Sev2 ---
s2 = section(2)
if '**Sev2 (' not in s2 or '**Non-Sev2 (' not in s2:
    print('SECTION 2 MISSING Sev2/Non-Sev2 SPLIT')
else:
    sev2_tbl, non_tbl = s2.split('**Non-Sev2 (')[0], s2.split('**Non-Sev2 (')[1]
    sev2_tbl = sev2_tbl.split('**Sev2 (')[1]
    ids = lambda b: re.findall(r'\(https://t\.corp\.amazon\.com/([A-Z0-9]+)\)', b)
    a, b = ids(sev2_tbl), ids(non_tbl)
    if len(a) != len(set(a)): print('SECTION 2 DUPLICATE IDS IN Sev2 TABLE')
    if len(b) != len(set(b)): print('SECTION 2 DUPLICATE IDS IN Non-Sev2 TABLE')
    both = set(a) & set(b)
    if both: print('SECTION 2 ID IN BOTH TABLES:', *sorted(both))
    if len(a) != m['sev2_resolved']:
        print(f"SECTION 2 Sev2 LINK COUNT {len(a)} != metrics.sev2_resolved {m['sev2_resolved']}")
    if len(b) != m['non_sev2_resolved']:
        print(f"SECTION 2 Non-Sev2 LINK COUNT {len(b)} != metrics.non_sev2_resolved {m['non_sev2_resolved']}")
    if m['sev2_resolved'] + m['non_sev2_resolved'] != m['resolved']:
        print('SECTION 2 SPLIT DOES NOT SUM TO metrics.resolved')
    # Per-table Count column must sum to that table's Total.
    for name, blk, exp in (('Sev2', sev2_tbl, m['sev2_resolved']), ('Non-Sev2', non_tbl, m['non_sev2_resolved'])):
        rows = [l.split('|') for l in blk.splitlines() if l.startswith('|')]
        n = sum(int(c[2].strip()) for c in rows
                if len(c) > 3 and c[2].strip().isdigit() and 'Total' not in c[1])
        if n != exp: print(f'SECTION 2 {name} COUNT COLUMN SUMS TO {n}, EXPECTED {exp}')
    # Every resolved ticket appears in exactly one of the two tables.
    seen = set(a) | set(b)
    for line in open('/tmp/kcli_oncall_resolved.jsonl'):
        t = json.loads(line)
        did = t.get('display_id') or t.get('id')
        if did and did not in seen: print('MISSING FROM SECTION 2:', did)

# --- Section 6: one row per ticket, Pages column sums to metrics.pages ---
s6 = section(6)
rows = [l for l in s6.splitlines() if re.match(r'^\|\s*\d+\s*\|', l)]
if len(rows) != m['paged_tickets']:
    print(f"SECTION 6 ROW COUNT {len(rows)} != metrics.paged_tickets {m['paged_tickets']}")
try:
    pg = [int(l.split('|')[3].strip()) for l in rows]
except (ValueError, IndexError):
    print('SECTION 6 NON-NUMERIC Pages CELL'); pg = []
if pg and sum(pg) != m['pages']:
    print(f"SECTION 6 Pages COLUMN SUMS TO {sum(pg)} != metrics.pages {m['pages']}")
if pg and pg != sorted(pg, reverse=True):
    print('SECTION 6 NOT SORTED BY Pages DESCENDING')
if not re.search(r'\|\s*\*\*Total\*\*\s*\|\s*\*\*%d\*\*\s*\|' % m['pages'], s6):
    print(f"SECTION 6 MISSING/WRONG Total ROW (expected **{m['pages']}**)")

# Every paged ticket present, exactly once, with no blank synopsis cell.
for line in open('/tmp/kcli_oncall_pages.jsonl'):
    r = json.loads(line)
    if r['display_id'] not in s6:
        print('SECTION 6 MISSING PAGED TICKET:', r['display_id'])
    elif len([x for x in rows if r['display_id'] in x]) != 1:
        print('SECTION 6 TICKET NOT EXACTLY ONE ROW:', r['display_id'])
    if r.get('notifications') and r.get('page_count') != len(r['notifications']):
        print('page_count DISAGREES WITH notifications[]:', r['display_id'])
    for f in ['impact_summary','root_cause','mitigation','action_items','risk_of_recurrence','related_tickets']:
        if not str(r.get(f,'')).strip(): print('SECTION 6 BLANK CELL:', r['display_id'], f)

# --- Section 8: composition must account for every open ticket, exactly once ---
CATS = {'Security and trust boundary','MCP servers and tools','TUI and terminal',
        'Auth and connectivity','Platform and packaging','Stability and crashes',
        'Context and model behaviour','Subagents and hooks',
        'Compliance and measurement','Release infra and access','Other'}
open_n = sum(1 for _ in open('/tmp/kcli_oncall_queue.jsonl'))
assigned = [l.split('\t') for l in
            open('/tmp/kcli_oncall_queue_cats.tsv').read().splitlines() if l.strip()]
ids = [a[1] for a in assigned]
if len(assigned) != open_n:
    print('SECTION 8 CLASSIFIED %d != %d OPEN TICKETS' % (len(assigned), open_n))
if len(set(ids)) != len(ids):
    print('SECTION 8 TICKET CLASSIFIED TWICE')
for a in assigned:
    if a[0] not in CATS:
        print('SECTION 8 UNKNOWN CATEGORY:', a[0])

s8 = section(8)
rows = [l for l in s8.splitlines()
        if l.startswith('|') and '---' not in l and 'Category' not in l]
body = [r for r in rows if '**Total**' not in r]
if not re.search(r'\|\s*\*\*Total\*\*\s*\|\s*\*\*%d\*\*\s*\|' % open_n, s8):
    print('SECTION 8 MISSING/WRONG Total ROW (expected **%d**)' % open_n)
try:
    counts = [int(r.split('|')[2].strip()) for r in body]
    shares = [int(r.split('|')[3].strip().rstrip('%')) for r in body]
except (ValueError, IndexError):
    print('SECTION 8 NON-NUMERIC Count/Share CELL'); counts = shares = []
if counts and sum(counts) != open_n:
    print('SECTION 8 Count COLUMN SUMS TO %d != %d' % (sum(counts), open_n))
if counts and counts != sorted(counts, reverse=True):
    print('SECTION 8 NOT SORTED BY Count DESCENDING')
if shares and not 98 <= sum(shares) <= 102:
    print('SECTION 8 Share COLUMN SUMS TO %d%%, expected ~100' % sum(shares))
for r in body:
    name = r.split('|')[1].strip()
    if name not in CATS:
        print('SECTION 8 UNKNOWN CATEGORY ROW:', name)
if re.search(r'(?i)(last week|week over week|up from|down from)', s8):
    print('SECTION 8 CLAIMS A TREND (re-classified weekly, not comparable)')

# An Other bucket over 10% means the taxonomy is stale and must be called out.
other_n = sum(1 for a in assigned if a[0] == 'Other')
if other_n and other_n > 0.10 * open_n and 'New category needed' not in s8:
    print('SECTION 8 Other IS %d/%d (>10%%) BUT NO "New category needed:" PROPOSAL'
          % (other_n, open_n))
other_row = [r for r in body if r.split('|')[1].strip() == 'Other']
if other_row and 'https://t.corp.amazon.com/' not in other_row[0]:
    print('SECTION 8 Other ROW DOES NOT LIST TICKET IDS')
PY
```

Fix anything reported. A paged ticket that is still open legitimately appears in both Section
6 and Section 7 — that is not a duplicate and the checks above do not flag it.

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
git add .ops/weekly-reviews/{end_date}.md .ops/weekly-reviews/artifacts/{end_date}-release-rate.svg
git commit -m "ops: weekly ops review {end_date}"
git push -u origin "$branch"
gh pr create \
  --title "[Kiro-CLI] Weekly Ops Review - {MM/DD/YYYY}" \
  --label no-changelog \
  --body "Auto-generated weekly ops review for the {start_date} → {end_date} oncall week.

Review during the ops meeting by leaving PR comments — Sections 3 (Pain Level), 5 (Action
Items), 9 (Security Risks) and 11 (Dashboard Review) are filled live this way. Run
\`@apply-ops-review-comments {pr_number}\` to fold comments back in, then merge when finalized."
```

Push with the branch named **literally** on the command line (`git push -u origin
ops/weekly-review-{end_date}`). Passing it via a shell variable or chaining verification onto
the same command with `$(...)` substitutions can trip push-protection filters, which then
looks like a denial rather than a syntax problem. Verify the push separately with
`git ls-remote origin <branch>` and compare to local `HEAD` — do NOT rely on
`git rev-parse origin/<branch>`, which fails when no local tracking ref exists.

Capture the PR URL from `gh pr create`. Commit only the report file and its chart artifact
(`.ops/weekly-reviews/{end_date}.md` and
`.ops/weekly-reviews/artifacts/{end_date}-release-rate.svg`) — never stage unrelated changes.
If the branch already exists (a regenerate of the same week), `git checkout "$branch"`,
overwrite the files, and add a new commit rather than amending or creating a duplicate branch.

## Step 10 — Report completion

Print a short summary (do NOT paste the full report): the PR URL (or local file path on
`no_pr`, or "dry run — not written" on `dry_run`); oncall week + current oncall; Pages
(`{pages}` across `{paged_tickets}` tickets, plus escalations if any), Queue `x→y`, Incoming,
Resolved (with the Sev2/non-Sev2 split), Releases Shipped, LSE count; counts of paged tickets
/ open Sev2s / cross-team items; **which paging source was used** (`builder-insights` or the
degraded `engagements-fallback`, and if the latter, that per-ticket counts are unavailable);
any approximate (`~`) figures, lossy `lastUpdatedAt` tickets, or warnings; and a reminder that
Sections 3, 5, 9, 11 are filled during the meeting via PR comments
(`@apply-ops-review-comments`). For Section 8, state the open-ticket count classified, the
top three categories, and that counts are a fresh snapshot and not comparable to last week.

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
   - Inline comments map to the file/line they anchor to (often Sections 3/5/9/11 or a
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
- Do NOT commit or stage anything other than `.ops/weekly-reviews/{end_date}.md` and its
  `.ops/weekly-reviews/artifacts/{end_date}-*.svg` chart — never bundle unrelated
  working-tree changes into the report commit.
- Do NOT merge the review PR — the team merges it when the report is finalized.
- Do NOT resolve reviewers' comment threads on their behalf when applying comments.
- Do NOT push to `main`/`master`; the report always lives on its `ops/weekly-review-*` branch.
- Do NOT leave auto-populated fields blank; do NOT assume ticket details without fetching.
- Do NOT group unrelated tickets or use a representative ticket for a group — list every ID.
- Do NOT hold raw ticket responses in context; do NOT skip any Sev2 ID; do NOT produce a
  partial report.
- Do NOT hardcode the window boundary as `17:00:00Z` or assume the handoff is 09:00 — it is
  09:30 `America/Los_Angeles` and DST-aware.
- Do NOT reconstruct pages by scanning ticket threads; they contain no paging records.
- Do NOT use `currentSeverity` for the Step 3 paged-ticket candidate set — it drops tickets
  downgraded after paging. Use `minimumSeverity: 2`.
- Do NOT filter Builder Insights page rows on `is_primary` — it lags the handoff. Use
  `recipient`.
- Do NOT hardcode `leaderLogin`, the paging aliases, or the Builder Insights `skillPath`.
- Do NOT fold manager-escalation pages into the `pages` headline figure.
- Do NOT write "pending root cause" or a generic scan summary without first reading the
  ticket's `description`.

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
- Oncall week is the 09:30 `America/Los_Angeles` handoff to the next one, and it **IS**
  DST-sensitive: `16:30Z` in PDT, `17:30Z` in PST. Use the `iso()` helper in Parameters and
  confirm against `shiftStart` from Step 1 rather than converting by hand.
- Submit the Builder Insights Silver paging query first and poll it while collecting ticket
  data; it is async and has run past 7 minutes.
- Run `git fetch --tags origin` before counting releases — a stale clone undercounts.
- Provide `previous_report_url` for an accurate starting-queue figure when the previous
  week's report hasn't been generated yet.
- The review PR is the meeting surface: drop comments during the ops review, run
  `@apply-ops-review-comments` to fold them in, then merge when finalized. Use `no_pr` to
  skip the PR and just drop the file on the current branch.
- Numbers that legitimately differ and get questioned every week, so state the distinction
  inline: `Pages` counts notifications while `paged_tickets` counts tickets; `Incoming`
  collapses alarm storms and only counts tickets *created* in the window, so it is routinely
  lower than `paged_tickets` (in the 2026-08-03 week only 3 of the 10 paged tickets were
  created that week); and the `Release` root-cause bucket counts release tickets *closed*,
  not releases *shipped*.
