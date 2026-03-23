---
name: knight-rider
description: Guide for running LLM-driven exploratory tests on the Kiro CLI TUI. Use when asked to test TUI features, create demo recordings, verify bug fixes visually, or drive the real TUI through HTTP. Triggers on questions about knight-rider, TUI testing, visual evidence, or exploratory testing.
---

# Knight Rider — LLM-Driven TUI Test Harness

HTTP server that launches kiro-cli in a real PTY, exposes endpoints for driving it, and captures evidence frames to disk as colored HTML with video replay.

## Location

- Script: `packages/tui/e2e_tests/knight-rider.ts`
- Outputs: `packages/tui/e2e_tests/test-outputs/knight-rider-<timestamp>/`

## Starting the Server

```bash
cd packages/tui

# Kill stale instances
for pid in $(lsof -ti:3001 2>/dev/null); do kill $pid 2>/dev/null; done; sleep 1

# Start (runs TUI from source by default — picks up local TS changes)
nohup bun run knight-rider > /tmp/knight-rider.log 2>&1 &
echo $! > /tmp/knight-rider.pid

# Wait for boot (~20s for MCP servers)
sleep 20
curl -s http://localhost:3001/api/status  # check ready:true
```

**IMPORTANT**: The default mode runs `bun ./src/index.tsx` with `KIRO_AGENT_PATH=target/debug/chat_cli`.
This means local TypeScript changes are picked up immediately (no rebuild needed), but you need a
Rust binary at `target/debug/chat_cli`. Build it once with `cargo build -p chat_cli` from the repo root.

### CLI Flags

| Flag | Default | Description |
|------|---------|-------------|
| (none) | **local dev** | Run TUI from source + local Rust binary — picks up TS changes instantly |
| `--system` | off | Use system `kiro-cli chat --tui` instead of local source |
| `--v1` | off | Launch legacy V1 Rust TUI |
| `--cmd "bash"` | — | Launch any arbitrary command |
| `--port 4000` | `3001` | Custom port |
| `--out /tmp/test` | auto-timestamped | Custom output directory |

## Shell Helpers

Define these at the start of every test session:

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

wait_for_idle() {
  for i in $(seq 1 30); do
    if curl -s $KR/screen 2>/dev/null | python3 -c "import sys,json; exit(0 if 'ask a question' in '\n'.join(json.load(sys.stdin)['lines']).lower() else 1)" 2>/dev/null; then return 0; fi
    sleep 3
  done
  echo "TIMEOUT"
}

frame() {
  curl -s -X POST $KR/frame -d "{\"label\":\"$1\"}"
}

wait_text() {
  curl -s -X POST $KR/wait-for-text -d "{\"text\":\"$1\",\"timeout\":${2:-15000}}"
}
```

## Full API Reference

### Read State

| Method | Endpoint | Body | Description |
|--------|----------|------|-------------|
| GET | `/api/status` | — | `{ ready, error, frameCount, outputDir }` |
| GET | `/api/screen` | — | `{ lines: string[] }` — current terminal content |
| GET | `/api/screen/html` | — | HTML with ANSI colors rendered |
| GET | `/api/frames` | — | All captured frames with timestamps |

### Send Input

| Method | Endpoint | Body | Description |
|--------|----------|------|-------------|
| POST | `/api/keys` | `{ "keys": "x" }` | Send keystrokes (one or more chars) |
| POST | `/api/enter` | — | Press Enter (`\r`) |
| POST | `/api/escape` | — | Press Escape (close panels/overlays) |
| POST | `/api/up` | — | Arrow Up (navigate menus, scroll panels) |
| POST | `/api/down` | — | Arrow Down (navigate menus, scroll panels) |
| POST | `/api/ctrlc` | — | Ctrl+C (cancel in-progress request) |

### Synchronization

| Method | Endpoint | Body | Description |
|--------|----------|------|-------------|
| POST | `/api/wait-for-text` | `{ "text": "...", "timeout": 10000 }` | Block until text appears on screen |
| POST | `/api/sleep` | `{ "ms": 1000 }` | Sleep for N milliseconds |

### Evidence Capture

| Method | Endpoint | Body | Description |
|--------|----------|------|-------------|
| POST | `/api/frame` | `{ "label": "name" }` | Capture named screenshot (HTML + TXT) |
| GET | `/` | — | Live dashboard with xterm viewer |
| GET | `/report` | — | Self-contained evidence report (works offline) |

### WebSocket

| Protocol | Endpoint | Description |
|----------|----------|-------------|
| WS | `/ws` | Raw PTY data stream for live terminal viewer |

## V2 TUI Interaction Patterns

### Typing Text
Always use `type_text` (char by char with 40ms delay). The TUI processes keystrokes individually — sending a full string at once via `/api/keys` will drop characters.

```bash
type_text "hello world"
```

### Sending a Prompt
```bash
type_text "explain this codebase"
curl -s -X POST $KR/enter
wait_for_idle   # wait for agent to finish responding
frame "response-to-explain"
```

### Slash Commands
The `/` character triggers an autocomplete popup. Type the command name to filter, then Enter to select.

```bash
type_text "/"           # opens autocomplete menu
sleep 0.5
type_text "agent"       # filters to /agent
sleep 0.3
curl -s -X POST $KR/enter   # selects /agent
```

### Agent Picker (`/agent`)
After selecting `/agent`, an interactive picker appears. Type to filter agents, then Enter to select.

```bash
# Open agent picker
type_text "/"
sleep 0.5
type_text "agent"
sleep 0.3
curl -s -X POST $KR/enter
sleep 1
frame "agent-picker-open"

# Select an agent
type_text "dev"         # filter to "kiro-dev" or similar
sleep 0.3
curl -s -X POST $KR/enter   # select it
sleep 1
frame "agent-selected"
```

### Tools Panel (`/tools`)
Opens a scrollable panel. Use ↑↓ to scroll, ESC to close.

```bash
type_text "/"
sleep 0.5
type_text "tools"
sleep 0.3
curl -s -X POST $KR/enter
sleep 1
frame "tools-panel"

# Scroll through tools
curl -s -X POST $KR/down
sleep 0.3
curl -s -X POST $KR/down
sleep 0.3
frame "tools-scrolled"

# ALWAYS capture frame BEFORE closing
curl -s -X POST $KR/escape
sleep 0.5
```

### Tool Approvals
When the agent uses a tool that requires approval, the screen shows "requires approval" with options. Navigate with arrows and Enter to approve.

```bash
# Watch for approval prompt
wait_text "requires approval" 30000
frame "tool-approval-prompt"

# Navigate to "Trust" and approve
curl -s -X POST $KR/up    # or down, depending on current selection
sleep 0.3
curl -s -X POST $KR/enter
sleep 0.5
frame "tool-approved"
```

### Cancel In-Progress Request
```bash
curl -s -X POST $KR/ctrlc
sleep 1
frame "cancelled"
```

## Core Testing Loop

Every test follows this cycle:

1. **Look** — `screen` to read the terminal state
2. **Think** — decide what to do next based on what's visible
3. **Act** — type, press keys, approve tools
4. **Capture** — `frame "descriptive-label"` at every interesting moment
5. **Repeat**

## Example: Full Test Workflow

```bash
# 1. Setup
cd packages/tui
for pid in $(lsof -ti:3001 2>/dev/null); do kill $pid 2>/dev/null; done; sleep 1
nohup bun run knight-rider > /tmp/knight-rider.log 2>&1 &
sleep 20

# 2. Define helpers (paste the shell helpers block above)

# 3. Verify boot
curl -s $KR/status | python3 -c "import sys,json; d=json.load(sys.stdin); print('ready' if d['ready'] else 'ERROR: '+str(d.get('error')))"
frame "boot"

# 4. Send a prompt
type_text "what files are in this directory?"
curl -s -X POST $KR/enter
wait_for_idle
frame "prompt-response"

# 5. Test slash command
type_text "/"
sleep 0.5
type_text "help"
sleep 0.3
curl -s -X POST $KR/enter
sleep 2
frame "help-output"

# 6. Open evidence report
DIR=$(curl -s $KR/status | python3 -c "import sys,json; print(json.load(sys.stdin)['outputDir'])")
open "$DIR/index.html"
```

## Timing Guidelines

| Action | Wait After |
|--------|-----------|
| `type_text` | built-in 0.3s at end |
| `/api/enter` (prompt) | `wait_for_idle` (up to 90s) |
| `/api/enter` (slash cmd) | `sleep 1` |
| `/api/escape` | `sleep 0.5` |
| `/api/up` / `/api/down` | `sleep 0.3` |
| Boot | `sleep 20` then check `/api/status` |
| Tool approval | `wait_text "requires approval" 30000` |

## Workflow Rules

1. Tell user: **"Live at http://localhost:3001"**
2. Keep server running — do NOT kill between steps
3. Capture frames at every interesting moment (generous labeling)
4. When done: `open "$DIR/index.html"` to show evidence report
5. Only kill when user says done or starting a new test

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `/api/status` returns `ready: false` | Check `/tmp/knight-rider.log` for boot errors |
| Characters dropped when typing | Use `type_text` helper, not raw `/api/keys` with full strings |
| Screen shows stale content | Add `sleep 0.5` after actions before reading screen |
| `wait_for_idle` times out | Agent may still be working — increase timeout or check screen manually |
| Port 3001 already in use | Kill stale: `for pid in $(lsof -ti:3001 2>/dev/null); do kill $pid 2>/dev/null; done` |
| Slash command autocomplete not appearing | Ensure you type `/` alone first, then wait 0.5s before typing command name |
