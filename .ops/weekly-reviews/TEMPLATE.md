# [Kiro-CLI] Weekly Ops Review - {MM/DD/YYYY}

**On Call:** {current_alias} | **Next:** {next_alias} | **Previous:** {previous_alias}

## 1. Summary
* Pages: {pages} across {paged_tickets} tickets
* [Ticket Queue](https://t.corp.amazon.com/issues/?q=extensions.tt.status%3A%28Assigned%20OR%20Researching%20OR%20%22Work%20In%20Progress%22%20OR%20Pending%29%20AND%20extensions.tt.assignedGroup%3A%22Amazon%20Q%20for%20CLI%22): {queue_start} → {queue_end}
* Incoming Tickets: {incoming}
* Resolved Tickets: {resolved} ({sev2_resolved} Sev2, {non_sev2_resolved} lower)
* Large Scale Significant Events: {lse_count}
* Releases Shipped: {release_count} ({release_versions}) | Avg {release_avg} releases/week (trailing 4 weeks)

![Stable release rate per week](artifacts/{end_date}-release-rate.svg)

_{release_caveat}_

## 2. Graphs
### Ticket Resolved Count By Root Cause (Previous week)

_Split by severity, Sev2 first. Severity is the ticket's **current** severity, so a ticket
that paged at Sev2 and was later downgraded appears in the Non-Sev2 table._

**Sev2 ({sev2_resolved})**

| Root Cause | Count | Topic | Tickets |
|---|---|---|---|
| {root_cause} | {n} | {topic} | {ticket links} |
| Total | {sev2_resolved} |  |  |

**Non-Sev2 ({non_sev2_resolved})**

| Root Cause | Count | Topic | Tickets |
|---|---|---|---|
| {root_cause} | {n} | {topic} | {ticket links} |
| Total | {non_sev2_resolved} |  |  |

{optional_unpaged_sev2_impact}

## 3. Operational Pain Level

_Pick one (1–10) during the ops review._

{suggested 1–10 with a one-line justification, clearly marked as a suggestion}

## 4. Large Scale Significant Events
* {LSE bullet}

## 5. Previous Week's Action Items

During the meeting, go over open action items here https://tiny.amazon.com/1auvbeoty/taskamazdevroom7c22task

{optional carried-forward items from previous_report}

## 6. Page Log

_One row per paged ticket. Pages = notifications delivered to the primary oncall ({oncall_alias}) during the week, sourced from the paging system. Sorted by page count._

| # | Ticket | Title | Pages | First Page (PT) | Page Sequence | Customer Impact | Root Cause | Mitigation | Action Items | Risk of Recurrence | Related Tickets |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | [{id}](https://t.corp.amazon.com/{id}) | {title} | {page_count} | {first_paged_at} | {page_sequence} | {customer_impact} | {root_cause} | {mitigation} | {action_items} | {risk_of_recurrence} | {related_tickets} |
| | **Total** | | **{pages}** | | | | | | | | |

{optional escalation note: pages delivered to escalation tiers beyond the primary oncall}

## 7. Open Sev2s

| # | Ticket | Description | Customer Impact | Next Step | ETA To Resolve |
|---|---|---|---|---|---|
| 1 | [{id}](https://t.corp.amazon.com/{id}) | {description} | {customer_impact or documented measurement blocker} | {next_step} | {eta or TBD} |

## 8. Open Queue by Category ({open_count} open as of {MM/DD})

<!-- Generated per the skill's "Open queue composition" procedure: classify every open
     ticket into exactly one of the ten canonical categories, fresh each week. One row per
     non-zero category, sorted by count descending; omit empty categories. This is a
     snapshot of current composition, NOT a trend - do not state week-over-week deltas. -->

| Category | Count | Share | Tickets | What it covers |
|---|---|---|---|---|
| {category} | {n} | {pct}% | {all ticket links assigned to category} | {scope description} |
| **Total** | **{open_count}** | **100%** | **{open_count} linked tickets** | |

{provenance note: the open-ticket count classified and that classification was done fresh this week; and why this total may differ from the Section 1 week-ending queue figure - Section 1 is measured at the handoff boundary, this section when the report runs}

**Observations**

* {cross-cutting observation: shared root causes, duplicate pairs, per-account clusters, aged tickets}
* {only when Other exceeds 10% of the queue -- **New category needed:** {proposed name} -- {one-line scope} -- {ticket IDs}}

## 9. Security Risks

_To be reviewed during the meeting._

* Acknowledge High/Critical risks https://policyengine.amazon.com/dashboard/{oncall_alias}
* OS Patching https://mirador.security.aws.dev/#/insights/patching?viewingAs={oncall_alias}
* AppSec findings https://mirador.security.aws.dev/#/findings?viewingAs={oncall_alias}
* SAS risks https://sas.corp.amazon.com/summary/all/{oncall_alias}

## 10. Tickets Cut to Other Teams

| # | Team - Ticket | Description |
|---|---|---|
| 1 | {team} - [{id}](https://t.corp.amazon.com/{id}) | {description} |

## 11. Dashboard Review

Add dashboard spikes related findings here [[Kiro-CLI] Weekly Dashboard Investigation Notes](https://quip-amazon.com/umwaAzDXcFo1)
