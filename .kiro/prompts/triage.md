---
description: Review active Sev-2 tickets and recommend what to work on next
---

# Oncall Ticket Triage

Review active Sev-2 and Sev-2.5 tickets in the team's ticket queue using `TicketingReadActions`. For each ticket, fetch full details and present:

- **Status**: Current investigation state
- **Time to escalation**: Based on creation time and severity SLA
- **Summary**: One-line description of the issue
- **Next steps**: What action is needed, and by whom

Then recommend what the oncall should work on first. Prioritize:
1. Tickets closest to escalation
2. Tickets with pending action for the CLI team (not blocked on external teams)
3. Tickets with no assignee or stale investigation

If there are no active Sev-2s, say so and suggest checking the broader queue.
