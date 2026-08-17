---
description: Review active Sev-2 tickets and recommend what to work on next
---

# Oncall Ticket Triage

Review active Sev-2 and Sev-2.5 tickets in the team's ticket queue using `TicketingReadActions`. For each ticket, fetch full details and present:

- **Status**: Current investigation state
- **Time to escalation**: Based on creation time and severity SLA
- **Summary**: One-line description of the issue
- **Customer impact**: Incident window, impacted requests, deduplicated customer/client proxy, affected cohort, and recovery status; identify the evidence source or document attempted checks and the measurement blocker
- **Next steps**: What action is needed, and by whom

Do not recommend downgrade or resolution when customer impact has not been quantified unless the ticket explicitly documents why measurement is blocked and what evidence remains necessary.

Then recommend what the oncall should work on first. Prioritize:
1. Tickets closest to escalation
2. Tickets with unknown or unverified customer impact
3. Tickets with pending action for the CLI team (not blocked on external teams)
4. Tickets with no assignee or stale investigation

If there are no active Sev-2s, say so and suggest checking the broader queue.
