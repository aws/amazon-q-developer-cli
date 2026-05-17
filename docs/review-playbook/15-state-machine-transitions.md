# Review 15 — State machine transitions and flag lifecycle

**Why this class matters.** Boolean flags like `isProcessing`, `isCompacting`, and `isShellEscape` act as state-machine guards: they block concurrent operations, gate queue drains, and signal UI mode. When a flag is set `true` on entry but not cleared on every exit path (success, error, cancel, timeout), the system deadlocks. The prompt queue stops draining, the UI shows a permanent spinner, or the next user action is silently rejected. This pattern caused #25e3930a8 (cancel didn't clear `isProcessing`, next prompt rejected), #34556e8b5 (compaction set `isProcessing:false` but never called `processQueue()`, starving queued messages), and #667ba719f (`pending_prompt_response` not cleared on cancel, blocking all future prompts). The hard rule: **every state entry must have a matching exit on ALL paths** — the Asymmetric Lifecycle pattern applied to state.

**Scope.** Any boolean or enum flag that gates control flow in the application: prompt processing state, compaction state, shell-escape mode, agent-switching locks, session-management guards, approval-pending flags, and voice-recording state. This also covers queue-drain triggers that must fire after a state transition, and Zustand/store selectors that derive "busy" state from these flags.

Concrete starting points: the prompt-processing pipeline (where `isProcessing` gates `processQueue`), the compaction handler (`handleCompactionEvent`), shell-escape entry/exit, agent-switch locks, and the cancel/abort paths in `acp-client.ts`. Any future state machine — for example a multi-agent orchestrator lock, a background-indexing flag, or a file-sync busy guard — must be reviewed the same way.

## Techniques

1. **[code] Flag census.** Grep for all boolean state flags that gate control flow: `rg -n "isProcessing|isCompacting|isShellEscape|isPending|isLocked|isBusy|isWaiting|isStreaming" packages/`. For each flag, identify every call site that sets it `true`. Each set-true must have a corresponding set-false reachable on success, error, cancel, and timeout paths. A missing reset on any path is a finding.

2. **[code] Finally-block audit.** Every async function that sets a flag `true` before an `await` must clear it in a `finally` block (or Rust equivalent `defer`/drop guard). Grep for flag assignments followed by `await` without a surrounding `try/finally`. Pattern: `rg -B2 -A10 "= true" packages/ | grep -A10 await` then verify `finally` exists. A bare `try/catch` without `finally` that sets the flag false only in the success path is a finding.

3. **[code] Queue-drain coupling.** After every state transition from busy→idle, the associated queue must be drained. Grep for flag resets (`isProcessing = false`, `set.*isProcessing.*false`) and verify the next statement (or same block) calls the queue-drain function (`processQueue`, `drainPending`, `flushQueue`). A flag reset without a drain call is a finding — it leaves queued work stranded.

4. **[code] Cancel-path completeness.** Trace every cancel/abort handler (`cancelMessage`, `abortController.abort`, `handleCancel`, Ctrl+C handlers). Each must reset ALL flags that the cancelled operation could have set. Grep for cancel handlers and diff the flags they clear against the flags the operation sets. A cancel that clears `isProcessing` but not `pendingToolCalls` or `isStreaming` is a finding.

5. **[code] Timeout-path completeness.** Grep for timeout handlers (`setTimeout`, `AbortSignal.timeout`, deadline checks). Each timeout callback must reset the same flags as the success path. A timeout that fires but leaves `isCompacting = true` is a finding — the next compaction request will be rejected.

6. **[code] Enum-state exhaustiveness.** For enum-based state machines (not just booleans), verify every match/switch is exhaustive. In Rust, non-exhaustive `match` without `_ =>` that resets state is a finding. In TypeScript, verify discriminated unions have a `default` or `never` assertion that prevents silent no-ops on unexpected states.

7. **[blackbox] Rapid-cancel recovery.** Send a prompt, cancel within 50–200 ms (random), immediately send another prompt. Repeat 100 times. The second prompt must always be accepted and produce a response. Any instance where the UI shows "processing" indefinitely or rejects the prompt is a finding.

8. **[blackbox] Timeout-during-compaction.** Trigger a compaction (by filling context to the limit), then force a timeout (kill the backend connection mid-compaction or inject a delay). The next user prompt must be accepted within 2 s. If the UI remains in "compacting" state or the prompt queue is stuck, it is a finding.

9. **[blackbox] Shell-escape interrupt.** Enter shell-escape mode (`!command`), then Ctrl+C before the command completes. The TUI must return to normal prompt mode within 1 s. Repeat 50 times. Any instance where the TUI remains in shell-escape mode (input not echoed, prompt not shown) is a finding.

10. **[blackbox] Agent-switch during processing.** While a response is streaming, switch agents. The old agent's processing flag must be cleared and the new agent must accept prompts. If the switch hangs or the new agent shows "busy", it is a finding.

11. **[blackbox] Error-during-streaming.** Inject a network error mid-stream (kill the backend socket). The UI must transition to idle within 5 s, show an error message, and accept the next prompt. A stuck spinner or unresponsive input is a finding.

## What to record

Flag name, file:line where set true, all paths that reset it (success/error/cancel/timeout), missing reset path if any, queue-drain call present after reset?.

## Done criteria

Every boolean/enum state flag has a verified reset on all exit paths (success, error, cancel, timeout). Every flag reset is paired with a queue-drain call where applicable. All five blackbox probes pass with zero stuck-state occurrences across 100 iterations each.
