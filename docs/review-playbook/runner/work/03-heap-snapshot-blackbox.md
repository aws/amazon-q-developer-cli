---
id: 03-heap-snapshot-blackbox
review: 03-unbounded-growth
kind: blackbox
partition: feature
scope:
  include:
    - "packages/tui/scripts/probes/heap-snapshot.ts"
  exclude: []
platform: any
harness: ad-hoc
probe: packages/tui/scripts/probes/heap-snapshot.ts
duration: medium
techniques: [8]
budget:
  input-tokens: 6000
  output-tokens: 4000
  wall-clock: 20m
findings-prefix: 03-heap-snapshot-blackbox
depends-on: ["03-store-growth-code"]
defer: null
---

# Work item: Unbounded growth — Heap-snapshot diff — blackbox

## Scope in one line

Take pre/post heap snapshots via the TUI's test-mode IPC around a
workload, diff retained `self_size` by constructor, and flag any
constructor growing by more than 100 % or total heap delta ≥ 50 MB.

## What to do

1. Read technique 8 from `docs/review-playbook/03-unbounded-growth.md`.
2. Build the TUI bundle:

   ```bash
   (cd packages/tui && bun install && bun run build)
   ```

   Rust binary is not required — the probe launches
   `bun run packages/tui/src/index.tsx` in `KIRO_TEST_MODE` with
   `KIRO_MOCK_ACP` so the TUI exposes its test IPC socket (see
   `packages/tui/src/test-utils/TestModeProvider.tsx`).

3. Execute the probe:

   ```bash
   PROBE_OUTPUT_DIR=./probe-output/03-heap-snapshot \
     bun run packages/tui/scripts/probes/heap-snapshot.ts
   ```

   Default workload is 5 min (`HEAP_WORKLOAD_MS=300000`). CI defaults
   to 30 s. Override with `HEAP_WORKLOAD_MS=1800000` for the 30-min
   deep run originally described in the technique.

4. The probe emits findings per breached budget:
   - Total heap growth > 50 MB — `spiral`
   - Any constructor grew > 100 % (and > 64 KB in absolute terms) — `spiral`

   It retains both `.heapsnapshot` files in `$PROBE_OUTPUT_DIR` so a
   reviewer can load them in Chrome DevTools → Memory → Comparison view.

5. Copy finding / metrics / done-marker + both `.heapsnapshot` files
   from `$PROBE_OUTPUT_DIR` into the worker's findings scratchpad.

## Platform and harness notes

- `harness: ad-hoc` — runs locally on macOS/Linux. Windows via CI matrix.
- Requires `bun` ≥ 1.1 (snapshot generation + test IPC).
- Does **not** use `kill -USR2`. The probe calls `Bun.generateHeapSnapshot()`
  inside the child over IPC, which is deterministic and avoids signal
  handler race conditions. If the IPC socket fails to connect within
  15 s the probe exits 2 with an error log pointing at
  `src/test-utils/TestModeProvider.tsx`.
