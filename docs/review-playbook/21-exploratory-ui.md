# Review 21 — Exploratory UI interaction testing

**Why this class matters.** The TUI has a rich surface area of slash commands, keyboard shortcuts, interactive pickers, panels, and agent interactions that combine in ways unit tests cannot cover. Regressions in command handling, state transitions, and input processing often manifest only when features are used in combination — e.g. switching agents mid-stream, cancelling during tool approval, or using slash commands immediately after a resize. These bugs are invisible to static analysis and require driving the real TUI through realistic multi-step workflows.

**Scope.** The full interactive surface of the V2 TUI: slash commands, prompt input, tool approvals, keyboard shortcuts (Ctrl+C, Escape, Ctrl+X), agent/model switching, panels (tools, context, settings), session management, and shell escape (`!command`). The focus is on **combinations and transitions** — not individual features in isolation.

**Time budget.** Each review cycle spends exactly 10 minutes of wall-clock time on exploratory testing. The agent drives the TUI through varied scenarios, captures frames at every step, and records the full trace of actions taken. When an issue is found, the trace provides exact reproduction steps.

**Harness.** All techniques use Knight Rider (HTTP API at `http://localhost:3001`) as the probe harness. The agent sends keystrokes via `/api/keys`, reads the screen via `/api/screen`, and captures evidence via `/api/frame`. **Do not defer to runbook** — Knight Rider is available as a CI sidecar and these techniques must be executed directly by the worker agent driving the TUI through HTTP calls. No separate probe script is needed; the worker IS the probe.

## Techniques

All techniques below are `[ui-test]` with `harness: knight-rider` and `defer: null`. The worker agent executes them directly by making HTTP calls to the Knight Rider sidecar at `localhost:3001`. No probe script is required — the agent reads the screen, sends keystrokes, captures frames, and emits findings inline.

1. **[ui-test] Slash command exerciser.** Via Knight Rider, exercise each slash command in sequence: `/help`, `/agent`, `/model`, `/tools`, `/context`, `/compact`, `/settings`, `/theme`, `/clear`. For each: type the command, verify the expected UI response (panel opens, list appears, confirmation shown), then dismiss with Escape. Capture a frame before and after each command. Flag: crashes, hangs (no response within 10s), unexpected error messages, or panels that don't close on Escape.

2. **[ui-test] Agent workflow: prompt → tool approval → completion.** Send a prompt that triggers tool use (e.g. "create a file called /tmp/kiro-probe-test.txt with the text 'hello'"). Wait for the tool approval prompt. Approve it. Wait for completion. Verify: approval UI appeared, tool executed, completion message shown, prompt returned to idle. Capture frames at each transition.

3. **[ui-test] Cancel and interrupt scenarios.** Test Ctrl+C at various points: (a) while agent is streaming a response, (b) during tool approval prompt, (c) while a tool is executing. Verify: cancellation is acknowledged, TUI returns to idle prompt, no error messages, no hung state. Also test Escape to close panels/overlays without cancelling.

4. **[ui-test] Rapid command switching.** Type a slash command, then immediately type another before the first completes. Switch agents mid-conversation. Send a prompt, cancel it, send another immediately. Verify no state corruption, no duplicate responses, no orphaned UI elements.

5. **[ui-test] Code generation and file operations.** Ask the agent to write code to a temporary location (`/tmp/kiro-exploratory-*`). Verify the tool calls appear correctly, approvals work, and the file is actually created. Then ask a follow-up question about the file. This exercises the full agent loop including tool execution.

6. **[ui-test] Keyboard shortcut coverage.** Test all documented shortcuts in context: Ctrl+C (cancel), Escape (close panel), Up/Down (scroll/navigate), Enter (submit/select). Verify each behaves correctly in the current UI state (prompt idle, panel open, streaming, approval pending).

7. **[ui-test] Error recovery.** Trigger edge cases: submit an empty prompt (should be rejected), type an unknown slash command (should show error or treat as message), send very long input (should not crash or truncate silently). Verify graceful handling with no crashes or hung state.

## Execution protocol

The blackbox agent driving Knight Rider must:

1. **Record every action** — maintain a numbered trace log of every HTTP call made (keys sent, commands issued, frames captured).
2. **Capture frames liberally** — before and after every significant action, and immediately when something unexpected appears on screen.
3. **Time-box to 10 minutes** — stop after 10 minutes regardless of progress. Emit findings for anything broken, and a done marker summarizing what was covered.
4. **Vary the path** — do not follow the same sequence every run. Randomize which techniques to start with. The goal is to explore different combinations each cycle.
5. **Check expectations** — after each action, read the screen and verify the result matches what the command/action should produce. Reference the documented behavior (slash command descriptions, panel behavior, approval flow).

## What to record

For each finding: the full numbered trace of actions leading to the issue (every keystroke, every command, every frame label), the expected behavior, the actual behavior, and the frame evidence. The trace must be detailed enough for a human to reproduce the issue step-by-step using the Knight Rider shell helpers.

## Done criteria

10 minutes of exploratory testing completed. All slash commands exercised at least once across review cycles. No crashes, hangs, or unexpected errors encountered (or all encountered issues have findings filed). The trace log documents exactly what was tested.
