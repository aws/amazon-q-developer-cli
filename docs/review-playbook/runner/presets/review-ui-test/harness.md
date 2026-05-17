This is a UI test loop for a single `[ui-test]` work item.

The loop drives the TUI interactively via Knight Rider HTTP API at
`http://localhost:3001`. The tester sends keystrokes, reads the screen,
captures evidence frames, and emits findings when behavior deviates
from expectations.

**This is NOT a blackbox probe.** There is no external script to run.
The agent IS the test — it drives the TUI directly through HTTP calls.
**Never defer to runbook.** Knight Rider is available as a CI sidecar.

## Global rules

- Your output is a set of per-finding Markdown files under
  `{{STATE_DIR}}/findings/`.
- Execute the techniques listed in the work item by driving the TUI
  via Knight Rider HTTP API.
- **One file per finding.** Write to disk immediately.
- Capture a frame before and after every significant action.
- Maintain a numbered trace log of all HTTP calls made.
- Use `/api/wait-for-text` for synchronisation — do not rely on fixed sleeps.
- If the TUI becomes unresponsive (no response to `/api/status` within
  10s), emit a `crash` finding with the trace leading up to it.
- Do not kill or restart Knight Rider.
- Emit `task.complete` once the done marker is written.

## Severity rubric

- `crash` — TUI became unresponsive, process died, or unhandled error shown.
- `regression` — Feature behaves differently from documented/expected behavior.
- `smell` — Minor visual issue or unexpected but non-breaking behavior.

## Evidence requirements

Every finding includes:
- The numbered trace segment showing the actions that led to the issue.
- Frame labels captured before/after the issue.
- The expected behavior vs actual behavior.
- Screen content at the time of the issue.

## State directory

`{{STATE_DIR}}/` is the per-run scratchpad. Findings go under
`{{STATE_DIR}}/findings/`.
