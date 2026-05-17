---
kind: runbook
id: 03-session-heap-blackbox
platform: any
harness: ad-hoc
commit: 437912b66b026f9c97a6c8885a273ebbebbb023c
bun-version: 1.3.9
techniques: [7, 8]
duration: long
---

# Runbook: Unbounded growth — Session-lifetime & heap probes (blackbox)

## Prerequisites

1. **Build the TUI bundle and Rust binary:**

   ```bash
   cd packages/tui && bun install && bun run build && cd ../..
   cargo build -p chat_cli --bin chat_cli
   ```

2. **Environment variables (optional, for CI traceability):**

   ```bash
   export KIRO_PROBE_COMMIT=$(git rev-parse HEAD)
   export KIRO_PROBE_REF=$(git symbolic-ref --short HEAD)
   export KIRO_PROBE_BUILD=$(date -u +%Y%m%dT%H%M%SZ)
   export KIRO_PROBE_PLATFORM=$(uname -s | tr '[:upper:]' '[:lower:]')
   export KIRO_PROBE_BUN_VERSION=$(bun --version)
   ```

3. **Tools required:**
   - `bun` ≥ 1.1 (heap snapshot support)
   - A PTY scripting tool (e.g. `expect`, the `terminal-harness` from `packages/terminal-harness`, or a custom bun script)
   - Chrome DevTools or `heapsnapshot-parser` for heap diff analysis

---

## Technique 7 — Session-lifetime probe (60 min)

### Steps

1. **Start the TUI with logging enabled:**

   ```bash
   KIRO_TUI_LOG_LEVEL=debug \
   KIRO_TUI_LOG_FILE=/tmp/kiro-probe-session.log \
   KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
   cargo run -p chat_cli --bin chat_cli -- chat --tui
   ```

2. **Attach a scripted input driver.** Use `terminal-harness` or an expect script that performs the following loop every 60 seconds for 60 minutes:

   - Send a user message (paste a 500-line file content).
   - Wait for assistant response to complete.
   - Invoke `/context add` on a local file.
   - Cancel mid-stream (Ctrl+C or ESC) on every 5th iteration.
   - Resume with a follow-up message.

3. **Collect memory snapshots every 60 seconds.** Inject a sampling hook or use a sidecar script that queries the bun process:

   ```bash
   # In a separate terminal, every 60s:
   while true; do
     PID=$(pgrep -f "bun.*tui")
     if [ -n "$PID" ]; then
       # Log RSS from OS
       ps -o rss= -p "$PID" >> /tmp/kiro-probe-rss.csv
       # If the TUI exposes process.memoryUsage() via a debug endpoint or log:
       # parse from KIRO_TUI_LOG_FILE
     fi
     sleep 60
   done
   ```

4. **After 60 minutes, trigger GC and take a final measurement:**

   ```bash
   # Send SIGUSR1 if the process handles it for GC, or just record final RSS
   kill -USR1 $(pgrep -f "bun.*tui") 2>/dev/null || true
   sleep 5
   ps -o rss= -p $(pgrep -f "bun.*tui") >> /tmp/kiro-probe-rss.csv
   ```

5. **Shut down the TUI gracefully** (type `/exit` or Ctrl+D).

### What to measure

- RSS at each 60s interval (from `/tmp/kiro-probe-rss.csv`).
- Any `Map.size` or array `.length` values logged by the TUI (grep the log file).
- Trend: fit a linear regression to the RSS samples after minute 10 (warm-up).

### Pass / fail budgets

| Metric | Pass | Fail |
|--------|------|------|
| RSS growth rate after warm-up | < 1 MB/min | ≥ 1 MB/min sustained |
| Any single container (Map/Array) | Bounded or periodic shrink | Monotonically increasing with turns |
| Final post-GC RSS vs minute-10 RSS | < 50 MB delta | ≥ 50 MB delta |

---

## Technique 8 — Heap snapshot diff (30 min)

### Steps

1. **Start the TUI with heap snapshot support:**

   ```bash
   KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
   cargo run -p chat_cli --bin chat_cli -- chat --tui
   ```

2. **Take baseline heap snapshot** (within the first minute, after initial load):

   ```bash
   # Option A: bun built-in (if supported)
   kill -USR2 $(pgrep -f "bun.*tui")
   # Snapshot saved to cwd as Heap.<timestamp>.heapsnapshot

   # Option B: Programmatic (add to a debug probe script)
   # Bun.generateHeapSnapshot() called via injected code
   ```

   Record the snapshot filename as `BASELINE`.

3. **Run scripted activity for 30 minutes** (same driver as Technique 7, but 30 min).

4. **Take post-activity heap snapshot:**

   ```bash
   kill -USR2 $(pgrep -f "bun.*tui")
   ```

   Record as `POST_ACTIVITY`.

5. **Compare snapshots:**

   ```bash
   # Using Chrome DevTools:
   # 1. Open chrome://inspect → Memory tab
   # 2. Load both .heapsnapshot files
   # 3. Select "Comparison" view between BASELINE and POST_ACTIVITY
   # 4. Sort by "Size Delta" descending

   # Using CLI tool (if available):
   npx heapsnapshot-parser diff $BASELINE $POST_ACTIVITY --top=20
   ```

6. **Record the top-20 constructors by retained-size delta.**

### What to measure

- Retained size delta per constructor between baseline and post-activity.
- Focus on: `ToolCall`, `Message`, `String`, `Array`, `Map`, `Set`, `Object`.

### Pass / fail budgets

| Metric | Pass | Fail |
|--------|------|------|
| Any single constructor retained-size growth | < 10 MB | ≥ 10 MB |
| Total heap growth (post-GC) | < 50 MB | ≥ 50 MB |
| Unexpected large String retention | < 5 MB in new Strings | ≥ 5 MB |

---

## Where to record results

- Emit one finding file per failed metric at:
  `docs/review-playbook/runner/findings/03-session-heap-blackbox-<YYYYMMDD>-<HHMM>-<kebab-slug>.md`
- Use severity `spiral` for monotonic growth, `slowdown` for borderline, `smell` for fragile assumptions.
- Include raw CSV data or screenshot of heap diff in the Evidence section.
- Emit done marker when all measurements are recorded.
