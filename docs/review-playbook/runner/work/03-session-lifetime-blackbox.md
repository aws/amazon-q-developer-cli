---
id: 03-session-lifetime-blackbox
review: 03-unbounded-growth
kind: blackbox
partition: feature
scope:
  include:
    - "packages/tui/scripts/probes/session-lifetime.ts"
  exclude: []
platform: any
harness: ad-hoc
probe: packages/tui/scripts/probes/session-lifetime.ts
duration: long
techniques: [7]
budget:
  input-tokens: 6000
  output-tokens: 4000
  wall-clock: 90m
findings-prefix: 03-session-lifetime-blackbox
depends-on: ["03-store-growth-code"]
defer: null
knight-rider: true
---

# Work item: Unbounded growth — Session-lifetime probe — blackbox

## Scope in one line

Run `session-lifetime.ts` for 60 minutes to sample RSS/FD over a driven
turn loop and flag sustained memory growth via linear regression.

## What to do

1. Read technique 7 from `docs/review-playbook/03-unbounded-growth.md`.
2. Build the TUI bundle and the Rust binary:

   ```bash
   (cd packages/tui && bun install && bun run build)
   cargo build -p chat_cli --bin chat_cli
   ```

3. Execute the probe:

   ```bash
   PROBE_OUTPUT_DIR=./probe-output/03-session-lifetime \
     bun run packages/tui/scripts/probes/session-lifetime.ts
   ```

   Default duration is 60 min (`SOAK_DURATION_MS=3600000`). Set `CI=1`
   for an abbreviated 5-min run only when smoke-testing.

4. The probe emits one finding per breached budget:
   - RSS trend slope ≥ 1 MB/min (post 10-min warmup) — `spiral`
   - Total RSS delta ≥ 50 MB — `spiral`
   - FD growth ≥ 5 over baseline — `spiral`
   - Process unresponsive at end — `crash`

5. Copy finding / metrics / done-marker files from
   `$PROBE_OUTPUT_DIR` into the worker's findings scratchpad. Filenames
   already carry the `03-session-lifetime-blackbox` prefix.

## Platform and harness notes

- `harness: ad-hoc` — runs locally. Works on macOS and Linux; Windows
  requires a real PTY via the CI matrix.
- Holds a live `chat_cli` process for 60 minutes. Do not run on a
  shared workstation unless prepared to abandon the shell.
- If `PROBE_OUTPUT_DIR` is unset the probe writes to `./probe-output/`.
