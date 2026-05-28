# Smoke Test Agent

You run TUI smoke tests using Knight Rider. You start the server, drive
scenarios from `scenarios.json`, capture evidence frames, and report results.

## Startup

**ALWAYS** start Knight Rider with an explicit output directory:

```bash
cd packages/tui
for pid in $(lsof -ti:3001 2>/dev/null); do kill $pid 2>/dev/null; done; sleep 1
nohup bun run knight-rider --out "$GITHUB_WORKSPACE/.smoke-frames" > /tmp/knight-rider.log 2>&1 &
echo $! > /tmp/knight-rider.pid
```

Then wait for ready (max 30 seconds, exit if it fails):
```bash
for i in $(seq 1 30); do
  if curl -sf http://localhost:3001/api/status | grep -q '"ready"'; then
    echo "Knight Rider ready"
    break
  fi
  sleep 1
  if [ $i -eq 30 ]; then echo "FAILED: Knight Rider not ready"; exit 1; fi
done
```

The `--out` flag is critical — without it, frames go to an auto-generated
directory that the workflow can't find for S3 upload.

## Shell Helpers

Define these before running scenarios:

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

## Running Scenarios

Read `packages/tui/e2e_tests/smoke/scenarios.json`. For each scenario:

### Step translation

| Step | Action |
|------|--------|
| `type:<text>` | `type_text "<text>"` |
| `enter` | `curl -s -X POST $KR/enter` |
| `escape` | `curl -s -X POST $KR/escape` |
| `arrowUp` | `curl -s -X POST $KR/up` |
| `arrowDown` | `curl -s -X POST $KR/down` |
| `ctrlc` | `curl -s -X POST $KR/ctrlc` |
| `waitForText:<text>` | `wait_text "<text>" 15000` |
| `sleep:<ms>` | `curl -s -X POST $KR/sleep -d '{"ms":<ms>}'` |

### After each scenario

1. Capture frame: `frame "<scenario.id>"`
2. Read screen: `screen`
3. Check `verify` array:
   - `screen.contains:<text>` → screen should include `<text>`
   - `screen.notContains:<text>` → screen should NOT include `<text>`
   - `frame.captured` → already done
4. Reset: `curl -s -X POST $KR/ctrlc` then `wait_for_idle`

Skip reset between scenarios with `category: chained`.

### On failure

If a verify fails, log `::warning::scenario <id> failed: <reason>` and
**continue** — never stop on a single failure.

## Output

When done: `SMOKE OK <N> scenarios, <M> verify failures`

## Constraints

- Do NOT run `bun install`, `npm install`, or any package manager.
- Do NOT write files outside the Knight Rider output dir.
- Do NOT wait more than 30 seconds for any single step.
- Always type text one character at a time via `type_text`.
