
**Status:** Implemented (v1)  
**Author:** kennvene  
**Date:** 2026-05-14  
**Updated:** 2026-06-01  

---

## Summary

`/goal` introduces a goal-driven iterative loop where the user defines a desired outcome. The agent works toward the goal, and on each turn completion the system evaluates whether to continue or stop. The loop repeats up to N iterations or until the goal is achieved.

This enables mechanical enforcement of feature completion — the system doesn't stop until the goal criteria are met.

---

## What's Shipped (v1)

### Commands

| Command | Effect |
|---------|--------|
| `/goal "text" --max N` | Set a goal with optional iteration cap (default 5, max 50) |
| `/goal status` | Show current goal state in panel |
| `/goal clear` | Cancel active goal, emit telemetry |

### Core Loop

On each agent turn completion (`EndTurn`), if a goal is active:
1. Check if the agent called `GoalTool::Complete` — if yes, goal achieved
2. Otherwise, re-inject the goal prompt into the session
3. Repeat until max iterations exhausted or goal completed

The re-injection uses a single prompt template (`goal_initial.md`) that restates the goal, iteration count, completion contract, and guardrails.

### Rollout Gate

- `Feature::Goal` in both V1 (`crates/chat-cli/src/rollout.rs`) and V2 (`crates/chat-cli-v2/src/rollout.rs`)
- `rollout.json`: `segment: internal`, `channel: nightly`, `treatment_percent: 100`
- In test mode (`KIRO_TEST_MODE=1`) or debug builds, all rollout gates are bypassed

### Telemetry

Event: `kirocli_goalCompleted`

| Field | Type | Description |
|-------|------|-------------|
| `terminal_state` | string | `completed`, `exhausted`, `cancelled`, `reinjection_failed` |
| `iterations` | i64 | How many iterations ran |
| `max_iterations` | i64 | The configured cap |
| `duration_sec` | i64 | Wall-clock seconds from goal set to terminal state |

Emitted at four terminal sites in `acp_agent.rs`:
1. Max iterations exhausted
2. Re-injection failure (background task failed after retries)
3. Agent completed (called `GoalTool::Complete`)
4. User cancelled (`/goal clear`)

### TUI Integration

- `GoalPanel.tsx` — renders goal state from `kiro/goalStatus` extension notifications
- Status bar chip with 60s elapsed time tick
- Ghost text placeholder when goal is active
- `GoalArgs` type shared via `#[typeshare]`
- `/goal` hidden from command autocomplete when rollout is off

### Key Files

| File | Purpose |
|------|---------|
| `crates/agent/src/agent/goal.rs` | `GoalDefinition` type |
| `crates/agent/src/agent/tools/goal.rs` | GoalTool (complete/status) |
| `crates/chat-cli-v2/src/agent/acp/commands/goal.rs` | `/goal` command handler |
| `crates/chat-cli-v2/src/agent/acp/commands/goal_initial.md` | Goal prompt template |
| `crates/chat-cli-v2/src/agent/acp/goal.rs` | `GoalController` state machine |
| `crates/chat-cli-v2/src/agent/acp/acp_agent.rs` | EndTurn hook + telemetry |
| `packages/tui/src/components/ui/GoalPanel.tsx` | TUI panel component |
| `packages/tui/e2e_tests/goal-command.test.ts` | E2E tests |

---

## Follow-up: P1 — LLM Fast Judge

The current v1 loop relies on the agent self-assessing via `GoalTool::Complete`. The next iteration adds an **independent LLM judge** that evaluates completion externally:

```
Agent works → EndTurn → LLM Judge (fast model) → PASS/FAIL
                                                    ↓
                                          FAIL → re-inject feedback
                                          PASS → goal complete
```

The judge:
- Uses a fast/cheap model (e.g. Haiku-class)
- Sees: goal description + last N turns of transcript
- Does NOT use tools or inspect workspace
- Returns structured verdict: `{pass: bool, issues: [], summary: ""}`
- Cost: ~$0.001 per evaluation

---

## Follow-up: P2 — Full Validator Subagent

For mechanical verification (run tests, inspect files independently):

```
/goal "implement pagination" --agent semantic-reviewer
```

Spawns a full validator subagent with tool access, tracked in crew monitor.

---

## Follow-up: P3 — Stuck Detection

Hash-based repeated failure detection. When the same issues appear N times consecutively:
1. Mark goal as exhausted with "stuck" reason
2. Notify user to restate or break into sub-goals
