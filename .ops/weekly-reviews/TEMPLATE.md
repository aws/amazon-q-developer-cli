# [Kiro-CLI] Weekly Ops Review - {MM/DD/YYYY}

**On Call:** {current_alias} | **Next:** {next_alias} | **Previous:** {previous_alias}

## 1. Summary
* Pages: {pages}
* [Ticket Queue](https://t.corp.amazon.com/issues/?q=extensions.tt.status%3A%28Assigned%20OR%20Researching%20OR%20%22Work%20In%20Progress%22%20OR%20Pending%29%20AND%20extensions.tt.assignedGroup%3A%22Amazon%20Q%20for%20CLI%22): {queue_start} → {queue_end}
* Incoming Tickets: {incoming}
* Resolved Tickets: {resolved}
* Large Scale Significant Events: {lse_count}

## 2. Graphs
### Ticket Resolved Count By Root Cause (Previous week)

| Root Cause | Count | Topic | Tickets |
|---|---|---|---|
| {root_cause} | {n} | {topic} | {ticket links} |
| Total | {resolved} |  |  |

## 3. Operational Pain Level

_Pick one (1–10) during the ops review._

{suggested 1–10 with a one-line justification, clearly marked as a suggestion}

## 4. Large Scale Significant Events
* {LSE bullet}

## 5. Previous Week's Action Items

During the meeting, go over open action items here https://tiny.amazon.com/1auvbeoty/taskamazdevroom7c22task

{optional carried-forward items from previous_report}

## 6. Page Log

_One row per page event; the synopsis columns repeat for a ticket that paged multiple times._

| # | Ticket | Page / Announcement | Impact Summary | Root Cause | Mitigation | Action Items | Risk of Recurrence | Related Tickets |
|---|---|---|---|---|---|---|---|---|
| 1 | [{id}]({page_or_ticket_url}) | {page_announcement} | {impact_summary} | {root_cause} | {mitigation} | {action_items} | {risk_of_recurrence} | {related_tickets} |

## 7. Open Sev2s

| # | Ticket | Description | Next Step | ETA To Resolve |
|---|---|---|---|---|
| 1 | [{id}](https://t.corp.amazon.com/{id}) | {description} | {next_step} | {eta or TBD} |

## 8. Security Risks

_To be reviewed during the meeting._

* Acknowledge High/Critical risks https://policyengine.amazon.com/dashboard/{oncall_alias}
* OS Patching https://mirador.security.aws.dev/#/insights/patching?viewingAs={oncall_alias}
* AppSec findings https://mirador.security.aws.dev/#/findings?viewingAs={oncall_alias}
* SAS risks https://sas.corp.amazon.com/summary/all/{oncall_alias}

## 9. Tickets Cut to Other Teams

| # | Team - Ticket | Description |
|---|---|---|
| 1 | {team} - [{id}](https://t.corp.amazon.com/{id}) | {description} |

## 10. Dashboard Review

Add dashboard spikes related findings here [[Kiro-CLI] Weekly Dashboard Investigation Notes](https://quip-amazon.com/umwaAzDXcFo1)
