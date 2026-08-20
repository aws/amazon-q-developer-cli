# Smoke Test — LLM-Driven Exploratory Testing

Scenario-driven exploratory testing of the Kiro CLI TUI. The LLM reads the
scenario manifests, drives the TUI through Knight Rider, observes every frame,
compares with previous runs, and flags subtle visual differences.

## When to Use

When asked to "run smoke tests", "run the scenarios", or "exploratory test".

## Prerequisites

- Knight Rider running (use `~/workplace/kiro_reviewer/scripts/knight-rider.sh start`)
- `scenarios/` in this directory (the scenario manifests)
- Previous run reports in `~/workplace/kiro_reviewer/reports/smoke-*/` for comparison

## Where scenarios live

A scenario's directory says which backend runs it:

| Directory | Runs under |
|---|---|
| `scenarios/shared/` | whichever backend the lane selects |
| `scenarios/live/` | real services only |
| `scenarios/acp-mock/` | a recorded ACP-wire fixture |
| `scenarios/krs-mock/` | a real KAS answered by the fake Kiro Runtime Service |

A `krs-mock` scenario carries the `turns` that answer its prompts. A shared
scenario may carry them too, which is what makes it eligible for that lane; one
without them is skipped there.

```bash
bun run-smoke.ts                      # every scenario, each under its own backend
bun run-smoke.ts --backend acp-mock   # only the mocked ones, plus shared
bun run-smoke.ts --backend krs-mock   # needs: cargo build -p mock-krs-server
```

## Flow

### 1. Start Knight Rider

```bash
~/workplace/kiro_reviewer/scripts/knight-rider.sh start --dir <worktree>
```

Wait for `curl -s http://localhost:3001/api/status` to return `ready: true`.

### 2. Define shell helpers

```bash
KR="http://localhost:3001/api"

type_text() {
  local msg="$1"
  for (( i=0; i<${#msg}; i++ )); do
    local c="${msg:$i:1}"
    case "$c" in '"') c='\\"' ;; '\\') c='\\\\' ;; esac
    curl -s -X POST $KR/keys -d "{\"keys\":\"$c\"}" > /dev/null
    sleep 0.04
  done
  sleep 0.3
}

screen() {
  curl -s $KR/screen | python3 -c "
import sys,json
lines = json.load(sys.stdin)['lines']
for l in lines:
    s = l.strip()
    if s: print(s)
"
}

frame() { curl -s -X POST $KR/frame -d "{\"label\":\"$1\"}"; }
wait_text() { curl -s -X POST $KR/wait-for-text -d "{\"text\":\"$1\",\"timeout\":${2:-15000}}"; }
```

### 3. For each scenario in `scenarios/`

Read the scenario manifests. For each scenario:

#### a. Execute steps

Translate each step to Knight Rider API calls:
- `type:<text>` → `type_text "<text>"`
- `enter` → `curl -s -X POST $KR/enter`
- `waitForText:<text>` → `wait_text "<text>"`
- `ctrlc` → `curl -s -X POST $KR/ctrlc`
- `arrowUp` → `curl -s -X POST $KR/up`

#### b. Observe — READ THE SCREEN

After each step, **always** read the screen:
```bash
screen
```

Look at the output. Ask yourself:
- Does this look correct for this scenario?
- Are there rendering artifacts, misaligned text, broken borders?
- Is the content what the docs say should appear?
- Does anything look different from the last time this was run?

#### c. Capture evidence

Capture a frame at every interesting moment:
```bash
frame "scenario-id-step-description"
```

Capture at minimum:
- After boot (before first scenario)
- After each slash command executes
- When panels/overlays open (BEFORE pressing ESC)
- After tool approval dialogs appear
- Any time something looks wrong

#### d. Compare with previous run

If previous reports exist in `~/workplace/kiro_reviewer/reports/smoke-*/`:
- Read the previous `results.json` for pass/fail status
- Compare the current screen text with previous frame text files
- Flag any differences: new text, missing text, layout changes, color changes

#### e. Record result

For each scenario, record:
- `pass` — looks correct, matches docs, no visual issues
- `fail` — broken behavior, rendering bug, or regression from previous run
- `observation` — something subtle worth noting but not a failure

Include a comment with what you observed, referencing the `observe` field in the
scenario as a checklist. Example:

```
✅ slash-tools-panel: Tools panel shows 14 tools with Name/Source/Status/Description
   columns. All default to 'approval required'. Search filter present. ESC hint visible.
   No visual glitches. Matches previous run.
```

```
⚠️ slash-changelog: Shows 'Unknown command: /changelog'. Docs list this command but
   it's not in the current build. Regression or docs-vs-reality gap.
```

### 4. Reset between scenarios

After each scenario that modifies state:
```bash
# /clear to reset conversation
type_text "/clear"
curl -s -X POST $KR/enter
sleep 1
```

For scenarios that exit the TUI (`/quit`, Ctrl+C twice), restart Knight Rider.

### 5. Generate report

After all scenarios, the Knight Rider evidence report is at:
```bash
DIR=$(curl -s $KR/status | python3 -c "import sys,json; print(json.load(sys.stdin)['outputDir'])")
echo "Report: $DIR/index.html"
```

### 6. Publish report

```bash
bash e2e_tests/smoke/publish-smoke-report.sh "$DIR"
```

This copies the report to `~/workplace/kiro_reviewer/reports/` and makes it
available on the shared server at port 3002.

### 7. Post summary

Post a summary to Slack with:
- Total scenarios run, passed, failed, observations
- Any regressions from previous run
- Link to the evidence report
- Specific findings with frame references

## Scenario Step Reference

| Step | Knight Rider API | Notes |
|------|-----------------|-------|
| `type:<text>` | `type_text "<text>"` | Char-by-char, 40ms delay |
| `enter` | `POST /api/enter` | Wait 500ms after |
| `waitForText:<text>` | `POST /api/wait-for-text` | 15s timeout default |
| `waitForIdle` | Poll screen for "ask a question" | Up to 90s |
| `ctrlc` | `POST /api/ctrlc` | Wait 500ms after |
| `ctrlc-twice` | `POST /api/ctrlc` × 2 | Process exits |
| `arrowUp` | `POST /api/up` | Wait 300ms after |
| `ctrlj` | Send `\x0a` via `/api/keys` | Inserts newline |
| `ctrls` | Send `\x13` via `/api/keys` | Opens fuzzy search |
| `mock:*` | N/A — skip in exploratory mode | Mocks are for CI only |
| `prompt:<text>` | `type_text` + `POST /api/enter` | Real LLM response |

## Scenarios that need special handling

- **`/editor`, `/reply`** — open external editor, skip in headless
- **`/paste`** — needs clipboard, skip in headless
- **`/quit`** — exits TUI, run last or restart Knight Rider after
- **`keyboard-ctrlc-exit`** — exits TUI, run last or restart after
- **`mock:*` steps** — skip mock steps; in exploratory mode the real agent responds
- **`tool-use-*`** — send a real prompt that triggers tool use (e.g., "list files in this directory")
- **`tool-cancel`** — send a prompt, then Ctrl+C while agent is responding

## What to look for

### Visual issues
- Broken box-drawing characters or borders
- Text overflow or truncation
- Misaligned columns in panels (/tools, /mcp)
- Color contrast issues (text invisible against background)
- Flickering or rendering artifacts between frames

### Behavioral issues
- Command not recognized (shows "Unknown command")
- Panel doesn't open or close properly
- State not reset after /clear
- Autocomplete not showing expected commands
- Keyboard shortcuts not working

### Regressions from previous run
- Text that was present before but is now missing
- Layout changes (columns shifted, borders moved)
- New error messages or warnings
- Different command list in autocomplete
- Changed behavior for the same input
