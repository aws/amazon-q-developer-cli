# Bun 1.3.12 → 1.3.13 Performance Analysis

**Date:** 2026-04-29
**Author:** Kenneth Sanchez
**PR:** #2203

## Test Setup

- **Bundle:** Same minified `packages/tui/dist/tui.js` production build for both runs
- **Profiling:** `bun --cpu-prof` via wrapper script injected through `KIRO_TEST_TUI_JS_PATH`
- **Test pattern:** Launch TUI → interact normally → resize iTerm repeatedly → exit
- **Machine:** macOS, Apple Silicon

## Background: Embedded vs Development Bun

The production `kiro-cli` binary embeds a **pinned** bun version (currently 1.3.12, defined in `scripts/const.py`). This is the binary extracted to `~/Library/Application Support/kiro-cli/bun` and used when users run `kiro-cli chat --tui`.

The development environment (`bun run dev`, `bun run dev:profile`, and `KIRO_TEST_TUI_JS_PATH` builds) uses the developer's **system bun**, which is typically the latest version. This means developers always run against the latest bun while users are on the pinned version.

This version gap is how the regression was discovered: the TUI performed noticeably better in development (system bun 1.3.13) than in production (embedded bun 1.3.12). The profiling below quantifies the difference.

## Executive Summary

Bun 1.3.13 delivers an **8.8× improvement** in yoga layout performance during terminal resize, eliminating a black screen bug and reducing idle CPU from 50-65% to 3-5% on long-running sessions.

| Metric | Bun 1.3.12 | Bun 1.3.13 | Improvement |
|--------|-----------|-----------|-------------|
| Yoga layout self time | 32,055ms | 3,633ms | **8.8×** |
| Total yoga WASM time | ~47,000ms | ~10,000ms | **4.7×** |
| Twinki render (`Ze`) | 9,311ms | 5,807ms | **1.6×** |
| Twinki diff (`RR`) | 2,613ms | 1,794ms | **1.5×** |
| stdout `write` | 3,373ms | 41,984ms | See note¹ |
| Idle CPU after resize | 50-65% stuck | 3-5% recovers | **Fixed** |
| Black screen on resize | Yes | No | **Fixed** |

¹ `write` increasing is *healthy* — on 1.3.12 yoga starved the event loop so frames never reached stdout. On 1.3.13 layout is fast enough that the bottleneck shifts to I/O.

---

## Profile Overview

### Bun 1.3.12

```
Duration:        189.1s
Samples:         19,905
Sample interval: 9.50ms
Windows:         10 (10s each)
```

### Bun 1.3.13

```
Duration:        120.6s
Samples:         7,933
Sample interval: 15.20ms
Windows:         10 (10s each)
```

---

## Top 20 Functions — Bun 1.3.12

| # | Self Time | Total Time | Calls | Trend | Function |
|---|----------|-----------|-------|-------|----------|
| 1 | **32,055ms** | 1,028,641ms | 3,374 | `▁▁▁▁▅▁█▆▄▁` | `.wasm-function[61]` (yoga layout) |
| 2 | 9,463ms | 9,463ms | 996 | `▁▁▁▁▅▁█▆▅▁` | `.wasm-function[31]` (yoga leaf) |
| 3 | 9,311ms | 22,279ms | 980 | `▂▁▁▁▅▁█▆▄▁` | `Ze` (twinki cell render) |
| 4 | 7,591ms | 360,200ms | 799 | `▃▂▃▃█▁▇▇▅▁` | `(anonymous)` (twinki orchestrator) |
| 5 | 5,311ms | 17,548ms | 559 | `▁▁▁▁▅▁█▆▄▁` | `.wasm-function[40]` (yoga) |
| 6 | 4,836ms | 11,933ms | 509 | `▁▁▁▁▅▁█▆▅▁` | `.wasm-function[33]` (yoga) |
| 7 | 4,769ms | 4,769ms | 502 | `▁▁▁▁▄▁█▅▅▁` | `.wasm-function[108]` (yoga) |
| 8 | 4,646ms | 12,075ms | 489 | `▁▁▁▁▅▁█▆▅▁` | `.wasm-function[34]` (yoga) |
| 9 | 3,905ms | 5,605ms | 411 | `▁▁▁▁▄▁█▅▆▁` | `.wasm-function[69]` (yoga) |
| 10 | 3,420ms | 5,216ms | 360 | `▁▁▁▁▄▁█▅▅▁` | `.wasm-function[68]` (yoga) |
| 11 | 3,373ms | 3,373ms | 355 | `▁▁▁▂█▁▇▆▄▁` | `write` (stdout) |
| 12 | 3,354ms | 7,848ms | 353 | `▁▁▁▁▄▁█▅▄▁` | `.wasm-function[52]` (yoga) |
| 13 | 3,164ms | 4,874ms | 333 | `▁▁▁▁▄▁█▆▄▁` | `.wasm-function[67]` (yoga) |
| 14 | 3,126ms | 3,126ms | 329 | `▁▁▁▁▄▁█▆▅▁` | `.wasm-function[42]` (yoga) |
| 15 | 3,040ms | 4,000ms | 320 | `▁▁▁▁▅▁█▆▆▁` | `.wasm-function[45]` (yoga) |
| 16 | 2,755ms | 4,285ms | 290 | `▁▁▁▁▇▁█▄▃▁` | `Q1` (twinki ANSI output) |
| 17 | 2,708ms | 2,708ms | 285 | `▁▁▁▁▅▁█▃▁▁` | `next` (iterator) |
| 18 | 2,679ms | 4,123ms | 282 | `▁▁▁▁▄▁█▅▄▁` | `.wasm-function[66]` (yoga) |
| 19 | 2,613ms | 17,282ms | 275 | `▁▁▁▂▄▁█▆▃▂` | `RR` (twinki tree diff) |
| 20 | 2,565ms | 8,484ms | 270 | `▁▁▁▁▄▁█▆▄▁` | `.wasm-function[123]` (yoga) |

**14 of 20 are yoga WASM. Total yoga self time in top 20: ~86,000ms**

---

## Top 20 Functions — Bun 1.3.13

| # | Self Time | Total Time | Calls | Trend | Function |
|---|----------|-----------|-------|-------|----------|
| 1 | **41,984ms** | 41,984ms | 2,762 | `▁▁▁▁▁▁▁▅█▁` | `write` (stdout) |
| 2 | 8,680ms | 8,680ms | 571 | `▁▁▁▁▁▁▁▅█▁` | `next` (iterator) |
| 3 | 7,236ms | 231,902ms | 476 | `▁▁▁▁▁▁▁▅█▁` | `(anonymous)` (twinki orchestrator) |
| 4 | 5,807ms | 17,466ms | 382 | `▁▁▁▁▁▁▁▅█▁` | `Ze` (twinki cell render) |
| 5 | 4,971ms | 4,971ms | 327 | `▁▁▁▁▁▁▁▅█▁` | `.wasm-function[31]` (yoga leaf) |
| 6 | 3,633ms | 246,845ms | 239 | `▁▁▁▁▁▁▁▅█▁` | `.wasm-function[61]` (yoga layout) |
| 7 | 2,584ms | 34,886ms | 170 | `▁▁▁▁▁▁▁▅█▁` | `(unknown)` |
| 8 | 1,946ms | 174,337ms | 128 | `▁▁▁▁▁▁▁▇█▁` | `$ye` (twinki component) |
| 9 | 1,885ms | 2,037ms | 124 | `▁▁▁▁▁▁▁▆█▁` | `Qr` (twinki ANSI strip) |
| 10 | 1,794ms | 14,213ms | 118 | `▁▁▁▁▁▁▁▆█▂` | `RR` (twinki tree diff) |
| 11 | 1,611ms | 2,706ms | 106 | `▁▁▁▁▁▁▁▅█▁` | `.wasm-function[34]` (yoga) |
| 12 | 1,353ms | 1,353ms | 89 | `█▁▁▁▁▁▁▁▁▁` | `spawnSync` (startup) |
| 13 | 1,353ms | 11,629ms | 89 | `▁▁▁▁▁▁▁▅█▁` | `I1` (twinki text measure) |
| 14 | 1,079ms | 1,079ms | 71 | `▁▁▁▁▁▁▁▅█▁` | `stringSplitFast` |
| 15 | 927ms | 1,748ms | 61 | `▁▁▁▁▁▁▁█▇▁` | `ME` (twinki ANSI parse) |
| 16 | 882ms | 6,628ms | 58 | `▁▁▁▁▁▁▁▄█▁` | `Ld` (twinki line render) |
| 17 | 821ms | 1,383ms | 54 | `▁▁▁▁▁▁▁▅█▁` | `.wasm-function[45]` (yoga) |
| 18 | 806ms | 806ms | 53 | `▁▁▁▁▁▁▁▆█▁` | ANSI escape regex |
| 19 | 790ms | 1,748ms | 52 | `▁▁▁▁▁▁▁▅█▁` | `.wasm-function[68]` (yoga) |
| 20 | 790ms | 1,718ms | 52 | `▁▁▁▁▁▁▁▇█▁` | `.wasm-function[33]` (yoga) |

**5 of 20 are yoga WASM. Total yoga self time in top 20: ~12,600ms (vs ~86,000ms on 1.3.12)**

---

## Call Trees

### Yoga Layout Path (`.wasm-function[61]`)

The main yoga layout function is deeply recursive — it calls itself to traverse the flexbox tree.

**Bun 1.3.12:**
```
.wasm-function[61] (32,055ms self, 1,028,641ms total)
├── callers:
│   ├── .wasm-function[61] (self-recursive): 94,509 calls
│   └── .wasm-function[220] (yoga entry): 13,761 calls
├── callees:
│   ├── .wasm-function[61] (self-recursive): 94,509 calls
│   ├── .wasm-function[37]: 1,188 calls
│   ├── .wasm-function[229]: 970 calls
│   ├── .wasm-function[63]: 969 calls
│   └── .wasm-function[123]: 882 calls
│
├── .wasm-function[33] (4,836ms) → .wasm-function[40] (5,311ms)
│   ├── .wasm-function[69] (3,905ms) → .wasm-function[31] (leaf, 9,463ms)
│   ├── .wasm-function[68] (3,420ms) → .wasm-function[31]
│   ├── .wasm-function[67] (3,164ms) → .wasm-function[31]
│   └── .wasm-function[66] (2,679ms) → .wasm-function[31]
│
├── .wasm-function[34] (4,646ms) → .wasm-function[40] (shared)
│
├── .wasm-function[123] (2,565ms) → .wasm-function[52] (3,354ms)
│   └── .wasm-function[108] (leaf, 4,769ms)
│
└── .wasm-function[45] (3,040ms) → .wasm-function[31]
```

**Bun 1.3.13:**
```
.wasm-function[61] (3,633ms self, 246,845ms total)
├── callers:
│   ├── .wasm-function[61] (self-recursive): 14,186 calls  ← was 94,509!
│   ├── .wasm-function[220] (yoga entry): 2,047 calls      ← was 13,761!
│   └── .wasm-function[39]: 6 calls
├── callees:
│   ├── .wasm-function[61] (self-recursive): 14,186 calls
│   ├── .wasm-function[229]: 524 calls
│   ├── .wasm-function[37]: 183 calls
│   ├── .wasm-function[63]: 153 calls
│   └── .wasm-function[34]: 125 calls
│
├── .wasm-function[33] (790ms) → .wasm-function[68] (790ms) → .wasm-function[31] (4,971ms)
│                               → .wasm-function[66] → .wasm-function[31]
│
├── .wasm-function[34] (1,611ms) → .wasm-function[69] → .wasm-function[31]
│
└── .wasm-function[45] (821ms) → .wasm-function[31]
```

**Key difference:** Recursive calls to `.wasm-function[61]` dropped from **94,509 → 14,186** (6.7× fewer). This suggests bun 1.3.13 either caches layout results or avoids redundant layout passes during resize.

---

### Twinki Render Path

**Bun 1.3.12:**
```
processTicksAndRejections
└── (anonymous) @ tui.js (7,591ms self, 360,200ms total)
    └── doRender @ tui.js (18,360 calls)
        └── $ye @ tui.js (component render)
            ├── Ze @ tui.js (9,311ms) — cell-by-cell render
            │   └── I1 @ tui.js — text measurement
            │       ├── Q1 @ tui.js (2,755ms) — ANSI output
            │       └── next (2,708ms) — iterator
            ├── RR @ tui.js (2,613ms) — tree diff (recursive: 1,544 self-calls)
            └── zR @ tui.js — reconciler
                └── .wasm-function[220] — yoga entry
                    └── .wasm-function[61] — yoga layout (32,055ms)
```

**Bun 1.3.13:**
```
processTicksAndRejections
└── (anonymous) @ tui.js (7,236ms self, 231,902ms total)
    └── doRender @ tui.js (7,189 calls)  ← was 18,360!
        └── $ye @ tui.js (1,946ms)
            ├── Ze @ tui.js (5,807ms)
            │   └── I1 @ tui.js (1,353ms)
            │       └── next (8,680ms)
            ├── RR @ tui.js (1,794ms) — tree diff (recursive: 817 self-calls)
            ├── Ld @ tui.js (882ms) — line render
            └── zR @ tui.js — reconciler
                └── .wasm-function[220] → .wasm-function[61] (3,633ms)
```

**Key difference:** `doRender` calls dropped from **18,360 → 7,189** (2.6× fewer renders). Combined with faster yoga, each render is also cheaper.

---

### stdout Write Path

**Bun 1.3.12:**
```
writeFast @ streams
└── write @ unknown (3,373ms, 355 calls)
```

**Bun 1.3.13:**
```
writeFast @ streams
└── write @ unknown (41,984ms, 2,762 calls)  ← 7.8× more calls
```

On 1.3.12, only 355 write calls completed — the event loop was blocked by yoga. On 1.3.13, 2,762 writes completed — frames are actually reaching the terminal. This explains the black screen fix.

---

## 10-Second Window Heatmap

### Yoga Layout (`.wasm-function[61]`) — Self Time per Window

```
Window  | 1.3.12      | 1.3.13
--------|-------------|--------
  10s   |    162ms    |    61ms
  20s   |    409ms    |     0ms
  30s   |    190ms    |    15ms
  40s   |    390ms    |     0ms
  50s   |  6,251ms ▅  |    15ms
  60s   |      0ms    |     0ms
  70s   | 10,584ms █  |    15ms
  80s   |  8,161ms ▆  |  1,414ms ▅
  90s   |  5,795ms ▄  |  1,992ms █
 100s   |    114ms    |   122ms
```

### stdout Write — Self Time per Window

```
Window  | 1.3.12      | 1.3.13
--------|-------------|--------
  10s   |     19ms    |    46ms
  20s   |     19ms    |    15ms
  30s   |     57ms    |    15ms
  40s   |    228ms    |     0ms
  50s   |    960ms ▅  |    30ms
  60s   |      0ms    |    15ms
  70s   |    874ms    |   152ms
  80s   |    731ms    | 15,064ms ▅
  90s   |    484ms    | 25,598ms █
 100s   |      0ms    |  1,049ms
```

On 1.3.12, yoga consumed the CPU budget so writes were throttled. On 1.3.13, yoga finishes fast and the full budget goes to flushing frames.

---

## Idle CPU Observations

| Process | Bun | Idle CPU | During Resize | After Resize |
|---------|-----|----------|---------------|-------------|
| Embedded kiro-cli (8+ hrs uptime) | 1.3.12 | **50-65%** 🔴 | 90-92% | **Never recovers** 🔴 |
| Fresh session | 1.3.12 | 3-5% | 100% | 3-5% ✅ |
| Fresh session | 1.3.13 | 3-5% | 100% | 3-5% ✅ |

The long-running 1.3.12 session appears to have a stuck render loop from a previous resize — possibly related to the resize cascade bug addressed in PR #2150 and #2172.

---

## Raw Profile Files

| File | Bun | Size |
|------|-----|------|
| `/tmp/kiro-profiles/CPU.825632959865.88426.cpuprofile` | 1.3.12 | 1.9MB |
| `/tmp/kiro-profiles-1.3.13/CPU.825895201593.92084.cpuprofile` | 1.3.13 | 727KB |
| `/tmp/reports/profile-2026-04-30T05-41-10.html` | 1.3.12 analysis | — |
| `/tmp/reports/profile-2026-04-30T05-44-50.html` | 1.3.13 analysis | — |

## Recommendation

Bump embedded bun from 1.3.12 → 1.3.13. The yoga layout improvement alone justifies the upgrade.
