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
