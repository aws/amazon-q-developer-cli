---
id: 03-subscription-stress-blackbox
review: 03-unbounded-growth
kind: blackbox
partition: feature
scope:
  include:
    - "packages/tui/scripts/probes/subscription-stress.ts"
  exclude: []
platform: any
harness: ad-hoc
probe: packages/tui/scripts/probes/subscription-stress.ts
duration: medium
techniques: [9]
budget:
  input-tokens: 6000
  output-tokens: 4000
  wall-clock: 30m
findings-prefix: 03-subscription-stress-blackbox
depends-on: ["03-acp-lifecycle-code"]
defer: null
---

# Work item: Unbounded growth — Subscription stress test — blackbox

## Scope in one line

Spawn and tear down 100 `chat_cli` sessions in a row, measuring probe
process RSS/FD plus per-child peak RSS to detect subscription and
resource leaks across session lifecycle.

## What to do

1. Read technique 9 from `docs/review-playbook/03-unbounded-growth.md`.
2. Build the TUI bundle and the Rust binary:

   ```bash
   (cd packages/tui && bun install && bun run build)
   cargo build -p chat_cli --bin chat_cli
   ```

3. Execute the probe:

   ```bash
   PROBE_OUTPUT_DIR=./probe-output/03-subscription-stress \
     bun run packages/tui/scripts/probes/subscription-stress.ts
   ```

   Defaults: 100 cycles (`STRESS_CYCLES`; CI defaults to 20),
   500 ms dwell per child (`STRESS_DWELL_MS`).

4. The probe emits findings per breached budget:
   - Probe-process RSS grew > 20 MB total across cycles — `spiral`
   - Probe-process FD grew > 5 total across cycles — `spiral`
   - Child steady-state RSS drifted > 10 % from first iteration — `spiral`
   - Any child required SIGKILL to exit — `spiral`

5. Copy finding / metrics / done-marker files from
   `$PROBE_OUTPUT_DIR` into the worker's findings scratchpad.

## Measurement caveat

The probe measures **cross-process** resource leaks, not in-process
`.listenerCount()` values. This catches:

- FD leaks in the probe harness itself (bun-pty cleanup)
- Bundle-level state that persists between processes (module caches)
- Orphaned children not reaped by `kill()`

It does not catch in-process EventEmitter listener accumulation
inside a single child's lifetime. A future upgrade requires a
`LISTENER_COUNTS` IPC command in `TestModeProvider.tsx` that reports
`process._getActiveHandles().length` plus per-emitter listener
counts; the probe documents this path in its proposed-fix text.

## Platform and harness notes

- `harness: ad-hoc` — runs locally on macOS/Linux. Windows via CI matrix.
- Runtime is typically 2–3 minutes for 100 cycles at 500 ms dwell.
- Consumes file descriptors and transient memory at a steady rate —
  do not run on an already-loaded host.
