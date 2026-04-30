# Performance Baselines

CPU profile analysis reports for tracking TUI rendering performance over time.

## How to generate a new baseline

```bash
# 1. Build the TUI bundle
cd packages/tui && bun run build && cd ../..

# 2. Create a profiling wrapper
echo '#!/bin/bash
exec /path/to/bun --cpu-prof --cpu-prof-dir=/tmp/kiro-profiles "$@"' > /tmp/bun-prof.sh
chmod +x /tmp/bun-prof.sh
ln -sf /tmp/bun-prof.sh /tmp/bun

# 3. Run with profiling
PATH="/tmp:$PATH" KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
  cargo run -p chat_cli --bin chat_cli -- chat --tui

# 4. Analyze the profile
cd packages/tui && bun run analyze:profile -- /tmp/kiro-profiles/<profile>.cpuprofile

# 5. Copy reports here with descriptive names
cp /tmp/reports/profile-*.json docs/perf-baselines/<description>-baseline.json
cp /tmp/reports/profile-*.html docs/perf-baselines/<description>-baseline.html
```

## Naming convention

`<component>-<version-or-date>-baseline.{json,html}`

Examples:
- `bun-1.3.13-baseline.json` — baseline after bun version bump
- `twinki-2026-05-15-baseline.json` — baseline after twinki render changes

## Current baselines

| File | Bun | Date | Notes |
|------|-----|------|-------|
| `bun-1.3.12-baseline` | 1.3.12 | 2026-04-29 | Pre-upgrade, yoga regression |
| `bun-1.3.13-baseline` | 1.3.13 | 2026-04-29 | Post-upgrade, 8.8× yoga improvement |

## Key metrics to compare

- Yoga layout self time (`.wasm-function[61]`)
- `write` (stdout I/O)
- `Ze` (twinki cell render)
- `RR` (twinki tree diff)
- Total samples and duration
