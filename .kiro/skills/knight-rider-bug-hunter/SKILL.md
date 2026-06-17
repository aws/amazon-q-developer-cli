---
name: knight-rider-bug-hunter
description: Fuzz and exploratory testing of any KAS agent tool via Knight Rider. Use when asked to test a feature end-to-end, fuzz tool schemas, validate bug fixes, explore edge cases, or regression test against the real KAS engine. Triggers on bug hunting, fuzz testing, exploratory testing, schema validation, feature validation, or Knight Rider testing.
---

# Knight Rider Bug Hunter

Systematic fuzz-testing and exploratory testing of KAS agent tools through the Knight Rider HTTP harness. Drives the real TUI + KAS engine to find schema validation bugs, edge cases, and regressions in any tool.

## When to Use

- Testing any new or modified KAS tool end-to-end
- Fuzzing tool input schemas (null fields, missing optionals, type coercion, extra fields)
- Validating a bug fix by reproducing the failure then confirming the fix
- Regression testing after refactoring shared infrastructure (sync-tool, tool parsing, ACP)
- Exploring edge cases in tool behavior under real LLM interaction

## Setup

### 1. Start Knight Rider with the Branch Under Test

```bash
cd packages/tui

# If kiro-agent source changed, rebuild the KAS server first
cd /path/to/kiro-agent/packages/kiro-agent && node esbuild.server.mjs && cd -

# Start Knight Rider (--system uses installed kiro-cli binary)
pkill -f "knight-rider.*4002" 2>/dev/null; sleep 2
KIRO_KAS_SERVER_PATH=/path/to/kiro-agent/packages/kiro-agent/dist/server/acp-server.js \
  bun run e2e_tests/knight-rider.ts --kas --system --port 4002 &

# Wait for TUI to be ready
curl -s -X POST http://localhost:4002/api/wait-for-text \
  -H "Content-Type: application/json" \
  -d '{"text":"question","timeout":60000}'
```

### 2. Prerequisites

- **Auth**: `kiro-cli login` (KAS needs valid OIDC tokens)
- **Rebuild**: After kiro-agent code changes, always run `node esbuild.server.mjs` and restart Knight Rider
- **Port**: Default 4002; change with `--port N`

### 3. Define Helpers

```bash
KR="http://localhost:4002/api"

send_prompt() {
  curl -s -X POST $KR/keys -H "Content-Type: application/json" -d "{\"keys\":\"$1\"}"
  sleep 0.3
  curl -s -X POST $KR/enter
}

screen() {
  curl -s $KR/screen | python3 -c "
import sys,json
for l in json.load(sys.stdin)['lines']:
    s = l.strip()
    if s: print(s)
"
}

wait_idle() {
  curl -s -X POST $KR/wait-for-text -H "Content-Type: application/json" \
    -d '{"text":"ask a question","timeout":90000}'
}

frame() {
  curl -s -X POST $KR/frame -H "Content-Type: application/json" -d "{\"label\":\"$1\"}"
}
```

## Troubleshooting KAS

### Blank Screen

| Symptom | Cause | Fix |
|---------|-------|-----|
| Blank screen, `ready: true` | React hook mismatch (source mode) | Use `--system` flag |
| Blank screen with `--system` | Auth failure | Run `kiro-cli login` |
| No `acp-server` process | Bad `KIRO_KAS_SERVER_PATH` | `ls $KIRO_KAS_SERVER_PATH` |

### Agent Not Responding

| Symptom | Cause | Fix |
|---------|-------|-----|
| Prompt accepted, no output | KAS still initializing | Wait for model name in status bar |
| "Initializing..." forever | KAS crashed | `ps aux \| grep acp-server` |
| Tool fails silently | Schema validation error | Set `KIRO_TUI_LOG_FILE=/tmp/tui.log` and grep for errors |

### Getting Logs

```bash
# Set before starting Knight Rider
export KIRO_TUI_LOG_FILE=/tmp/kr-tui.log
export KIRO_TUI_LOG_LEVEL=trace

# After test
grep -i "error\|fail\|reject\|invalid" /tmp/kr-tui.log | tail -20
```

## Bug Hunting Methodology

### Phase 1: Happy Path

Confirm the tool works with fully valid input.

```bash
send_prompt 'Call <tool_name> with: {"field1":"value1","field2":"value2"}'
wait_idle
frame "happy-path-result"
screen
```

**Pass criteria**: Tool executes, returns expected output, no errors on screen.

### Phase 2: Schema Fuzzing

Systematically test what happens when input deviates from the happy path. Apply each variant and check if the tool (a) works correctly, (b) returns a clear error, or (c) crashes/hangs.

#### Null Value Variants

| # | Variant | What It Tests |
|---|---------|---------------|
| 1 | Omit entire optional field | Does the tool work without it? |
| 2 | Set optional field to `null` | Does null-stripping handle it? |
| 3 | Set required field to `null` | Does validation reject clearly? |
| 4 | Nested optional sub-field as `null` | Recursive null stripping |
| 5 | Optional object with all sub-fields `null` | Full-depth null handling |

#### Structural Variants

| # | Variant | What It Tests |
|---|---------|---------------|
| 6 | Empty object `{}` for nested optional | Missing required sub-fields |
| 7 | Extra unknown fields at top level | Passthrough vs. strict mode |
| 8 | Extra unknown fields in nested objects | Deep passthrough |
| 9 | Wrong type (string where number expected) | Coercion fallback |
| 10 | Array where object expected | Type mismatch handling |

#### Boundary Variants

| # | Variant | What It Tests |
|---|---------|---------------|
| 11 | Empty string for required string field | Min-length validation |
| 12 | Very long string (10K+ chars) | Truncation/limits |
| 13 | Number at min/max bounds | Range validation |
| 14 | Empty array for required array field | Min-items validation |
| 15 | Array with 100+ items | Unbounded array handling |

### Phase 3: Behavioral Edge Cases

Test tool-specific logic under unusual conditions:

#### For any tool:
- **Cancellation mid-execution** — Ctrl+C during tool run
- **Rapid re-invocation** — call same tool twice in quick succession
- **Conflicting inputs** — contradictory field values
- **Unicode/special chars** — emoji, newlines, null bytes in string fields

#### For multi-stage/orchestration tools:
- Single stage, no dependencies (minimal pipeline)
- Diamond dependency: A -> [B, C] -> D (parallel fan-out)
- Linear chain: A -> B -> C (sequential)
- Circular dependency (should fail validation)
- Duplicate names (should fail validation)
- Reference to non-existent dependency (should fail)

#### For tools with iteration/loops:
- Single iteration (no-op loop)
- Stop condition met immediately
- Stop condition never met (exhaustion behavior)
- Max iterations boundary (1, max value)
- Abort vs. continue on exhaustion

### Phase 4: Regression Testing

After fixing a bug:

1. **Reproduce** — send the exact input that triggered the original failure
2. **Capture** — `frame "regression-before-fix"` showing it now works
3. **Variant** — test nearby inputs (slightly different null patterns, field combos)
4. **Cross-tool** — if the fix was in shared infrastructure, test other tools too

## Handling Approval Prompts

Tools that invoke sub-agents or access files trigger approval prompts.

```bash
approve_if_needed() {
  local s=$(curl -s $KR/screen | python3 -c "import sys,json; print('\n'.join(json.load(sys.stdin)['lines']))")
  if echo "$s" | grep -qi "approval\|Allow\|Trust"; then
    curl -s -X POST $KR/down; sleep 0.3  # navigate to "Always allow" / "Trust"
    curl -s -X POST $KR/enter; sleep 1
    return 0
  fi
  return 1
}

# Poll loop — approve prompts until pipeline completes
for i in $(seq 1 60); do
  approve_if_needed
  if curl -s $KR/screen | python3 -c "import sys,json; exit(0 if 'ask a question' in '\n'.join(json.load(sys.stdin)['lines']).lower() else 1)" 2>/dev/null; then break; fi
  sleep 2
done
```

## Evidence Collection

Capture frames at every interesting moment:

```bash
frame "pre-test"           # baseline state
frame "input-sent"         # after sending prompt
frame "tool-executing"     # while tool is running
frame "result"             # final output
frame "bug-reproduced"     # if failure found
frame "fix-validated"      # after confirming fix works
```

Output directory: `curl -s $KR/status | python3 -c "import sys,json; print(json.load(sys.stdin)['outputDir'])"`

Open report: `open "$DIR/index.html"`

## Reporting Findings

For each bug found, document:

1. **Input** — exact JSON sent to the tool
2. **Expected** — what should happen
3. **Actual** — what happened (include screen text)
4. **Frame** — label of the captured evidence
5. **Root cause** — file, function, and why (if identified)
6. **Scope** — one tool only, or shared infrastructure affecting all tools?
7. **Fix verification** — frame label showing the fix works

### Common Root Cause Patterns

| Observed Behavior | Likely Cause | Where to Look |
|-------------------|-------------|---------------|
| Agent retries with rephrased input | Schema validation rejected LLM's input | `sync-tool.ts` parseInput / stripNullValues |
| Tool works in unit test, fails in KAS | Schema built differently at runtime | Tool's `buildSchema()` vs. what LLM receives |
| "does not match required schema" | Zod rejected null/undefined/wrong type | grep `InvalidSchemaError` in logs |
| Tool hangs, never returns | Awaiting something that never resolves | Check abort signal propagation |
| Approval prompt never appears | Tool not requiring approval for this input | Check tool's policy/tags |
