---
name: tcorp-tickets
description: SOP for querying TCORP/TT tickets for Kiro CLI oncall. Use when checking active Sev-2 tickets, searching ticket history, or cross-referencing bug reports with support tickets. Triggers on "tcorp", "tt tickets", "sev-2", "severity", "oncall tickets".
---

# TCORP Tickets SOP

Query and triage TCORP tickets for the Kiro CLI team. CTI: `Kiro / CLI / Intake`, Resolver Group: `Amazon Q for CLI`.

## Search active tickets

```
@builder-mcp/TicketingReadActions action=search-tickets
assignedGroup: ["Amazon Q for CLI"]
status: ["Assigned", "Researching", "Work In Progress", "Pending"]
sort: "currentSeverity asc"
rows: 20
responseFields: ["id", "extensions", "title", "createDate", "lastUpdatedDate"]
```

Note: severity is inside `extensions.tt.impact`, not a top-level field.

## Filter by severity

For Sev-2 only:
```
@builder-mcp/TicketingReadActions action=search-tickets
assignedGroup: ["Amazon Q for CLI"]
status: ["Assigned", "Researching", "Work In Progress", "Pending"]
currentSeverity: ["2", "2.5"]
sort: "currentSeverity asc"
rows: 10
```

## Mandatory Sev-2 customer-impact investigation

Finding or acknowledging a Sev-2/Sev-2.5 ticket is not sufficient triage. For every high-severity event:

1. Fetch the full ticket, including correspondence, worklogs, holds, and linked incidents.
2. Establish the exact alarm/degradation window and any broader service-incident window.
3. Identify the production metric or log source and reproduce its population filters, including version gates, dimensions, excluded failure reasons, sampling, and deduplication.
4. Quantify impacted requests and a deduplicated customer proxy (`userId`, unique `clientId`, or equivalent). State whether the proxy represents people, accounts, or installations and whether cardinality is approximate.
5. Break down impact by the dimensions that identify scope, such as model, version, engine, client type, account type, and internal/external population.
6. Reconcile the calculation against the alarm datapoints or backend metrics. Do not infer customers from metric sample count or count paired telemetry records as separate customer actions.
7. Record recovery time, the evidence source, and the distinction between exact ticket-window impact and broader linked-incident impact.

Do not recommend or perform downgrade/resolution until the impact is quantified. If credentials, retention, or schema quality blocks measurement, document the checks attempted, the blocker, and the precise evidence still needed instead of guessing.

## Search by keyword

```
@builder-mcp/TicketingReadActions action=search-tickets
assignedGroup: ["Amazon Q for CLI"]
query: "KEYWORD"
rows: 10
```

## Format

```
:rotating_light: *Active Sev-2 Tickets*
• <ticket_id> <title> — <status>, <age>d old (<link>)
```

If none: `:white_check_mark: No active Sev-2 tickets`

## Ticket links

Format: `https://t.corp.amazon.com/<ticket_id>`
