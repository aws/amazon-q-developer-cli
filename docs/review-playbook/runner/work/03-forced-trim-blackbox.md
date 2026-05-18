---
id: 03-forced-trim-blackbox
review: 03-unbounded-growth
kind: blackbox
partition: feature
scope:
  include:
    - "packages/tui/scripts/probes/forced-trim.ts"
  exclude: []
platform: any
harness: ad-hoc
probe: packages/tui/scripts/probes/forced-trim.ts
duration: medium
techniques: [10]
budget:
  input-tokens: 6000
  output-tokens: 4000
  wall-clock: 30m
findings-prefix: 03-forced-trim-blackbox
depends-on: ["03-store-growth-code"]
defer: null
knight-rider: true
---

# Work item: Unbounded growth — Forced-trim probe — blackbox

## Scope in one line

Drive the TUI past `MAX_SESSION_MESSAGES=50` with 250 turns and verify
trim mechanisms engage, using RSS-over-turn-count as a proxy for
array length (absent a direct IPC for store sizes).

## What to do

1. Read technique 10 from `docs/review-playbook/03-unbounded-growth.md`.
2. Build the TUI bundle and the Rust binary:

   ```bash
   (cd packages/tui && bun install && bun run build)
   cargo build -p chat_cli --bin chat_cli
   ```

3. Execute the probe:

   ```bash
   PROBE_OUTPUT_DIR=./probe-output/03-forced-trim \
     bun run packages/tui/scripts/probes/forced-trim.ts
   ```

   Defaults: 250 turns (`FORCED_TRIM_TURNS`), threshold 50
   (`FORCED_TRIM_THRESHOLD`), 50 ms inter-turn pause (`INTER_TURN_MS`).

4. The probe emits findings per breached budget:
   - Post-threshold RSS slope is > 10 % of pre-threshold slope — `spiral`
     (trim did not engage)
   - RSS at 2×threshold > 2× RSS at threshold — `spiral`
     (unbounded growth past threshold)
   - Process crashed during the probe — `crash`

5. Copy finding / metrics / done-marker files from
   `$PROBE_OUTPUT_DIR` into the worker's findings scratchpad.

## Measurement caveat

Neither `stores/session-conversations.ts` nor `utils/command-history.ts`
currently logs trim events, and the test-mode IPC does not expose
array sizes. The probe therefore measures RSS slope as a proxy. A
lower-noise future upgrade: add `logger.debug('trim fired', …)` at
the `msgs.slice(-MAX_SESSION_MESSAGES)` site, or add an `ARRAY_SIZES`
command to `TestModeProvider.tsx`. The probe documents both paths in
its proposed-fix text.

## Platform and harness notes

- `harness: ad-hoc` — runs locally on macOS/Linux. Windows via CI matrix.
- Runtime is typically 1–2 minutes for 250 turns at 50 ms spacing.
- Requires a live `chat_cli` binary because this probe goes through
  the full stack (not mock ACP).
