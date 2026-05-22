# Finding: TUI Render Loop — PieSpinner Without Region Isolation

**Date**: 2026-05-15  
**Severity**: High (100% CPU on idle)  
**Version**: kiro-cli 2.4.1-nightly.2-nightly, Bun 1.3.13 (arm64)  
**Playbook**: Review 17 (Render Loop Guards), Review 06 (Hot-path Allocation), Review 01 (Async Render Path)

---

## Observed Symptoms

A user's kiro-cli TUI process (bun, pid 51466) was consuming 98% CPU while completely idle — no user input, no agent activity, Rust backend at 0% CPU.

```
  PID  PPID  %CPU %MEM COMMAND
51466 51448  98.2  0.4 /Users/.../kiro-cli/bun /Users/.../kiro-cli/tui.js chat --resume-id f2ec3399-...
```

The parent Rust process (pid 51448) was at 0% CPU:
```
  PID  %CPU %MEM STAT COMMAND
51448   0.0  0.0 S+   .../kiro-cli-chat chat --resume-id f2ec3399-...
```

Session had been running since 07:54 AEST. CPU spike started around 08:24 AEST. Process accumulated **58 minutes of user CPU time** in ~1h47m of wall time.

## Render Metrics (KIRO_DEV=1)

The metrics chip showed:

```
72.2ms · 8620n · 0MB · #33788 · r17
```

| Metric | Value | Interpretation |
|--------|-------|----------------|
| 72.2ms | Render time per frame | Extremely high for idle — full yoga layout pass |
| 8620n | Yoga node count | Large tree (long resumed conversation) |
| 0MB | Heap growth | Not a memory leak |
| #33788 | Total render count | ~5.3 renders/sec average over session lifetime |
| r17 | Full redraws | Only 17 full redraws — this is incremental re-renders |

**Derived**: At 72ms/render and 98% CPU → ~14 renders/sec currently. This matches a 150ms setInterval (PieSpinner) triggering full-tree re-renders throttled only by render cost.

## Call Stack Analysis

`sample 51466 1` captured 801 samples on the main thread (100% CPU utilization):

```
Thread_52413105 (main thread)
  start (dyld)
    bun event loop
      _op_call_ignore_result_return_location (790 samples, recursive)
        _op_call_return_location (776 samples, recursive)
          _wasm_ipint_call_return_location_wide32 (663 samples, deeply recursive 10+ levels)
```

### Symbolicated (via bun-v1.3.13 profile build, UUID 4A17CD88-9E15-3FED-B870-A9FE3ED3C3C8)

| Address (offset) | Symbol | Samples | Meaning |
|-------------------|--------|---------|---------|
| `0x3199c5c` | `_op_call_ignore_result_return_location` | 790 | JSC LLInt: JS function call (result discarded) |
| `0x3198724` | `_op_call_return_location` | 776 | JSC LLInt: JS function call return |
| `0x31cae58` | `_wasm_ipint_call_return_location_wide32` | 663 | JSC LLInt: interpreted call dispatch trampoline |

**Interpretation**: The JS engine is spending 100% of CPU time executing deeply recursive JavaScript function calls in the **interpreter** (not JIT-compiled). The deep recursion pattern (10+ nested levels of the same address) is characteristic of:
- React reconciler tree walk (`reconcileChildren` → `beginWork` → recursive)
- Yoga layout calculation (`calculateLayout` recursing through 8620 nodes)

Both run on every render. At ~14 renders/sec, this saturates the CPU.

## Log Timeline

| Time (UTC) | Time (AEST) | Event |
|------------|-------------|-------|
| 21:54 May 14 | 07:54 | Session launched with `--resume-id` |
| 22:20 | 08:20 | Last `[store] sendMessage` TUI log entry |
| **22:24** | **08:24** | **MCP server re-initialization** (`failed to list prompts during server initialization`) |
| 22:24 | 08:24 | rust-analyzer `PrimeCaches` (1.4s) — confirms system was active |
| 23:38 | 09:38 | Telemetry `TooManyRequestsException` |
| 23:39 | 09:39 | Another MCP server re-init |
| 23:42 | 09:42 | rust-analyzer `PrimeCaches` (3.4s) — last Rust log entry |

The TUI log stopped writing after 22:20 UTC — the event loop was saturated with render work and couldn't process log writes.

**Trigger correlation**: The MCP server re-initialization at 22:24 UTC is the last significant event before the CPU spike. The `agent::agent::mcp::service` error (`failed to list prompts`) indicates an MCP server reconnection attempt.

## Root Cause Analysis

### The Mechanism (code-traced, not runtime-proven)

1. **Trigger**: MCP server re-initialization leaves a tool call stuck with `isFinished=false` in the active turn tail (tool was executing when MCP server died)

2. **Render loop**:
   - `ToolUseMessage` with `isFinished=false` renders `statusIcon='executing'` (see `packages/tui/src/components/ui/ToolUseMessage.tsx:86`)
   - `StatusBar` mounts a `PieSpinner` when `status === 'executing'` (see `StatusBar.tsx:156`)
   - `PieSpinner` has a 150ms `setInterval` calling `setFrameIndex()` (see `PieSpinner.tsx:26`)
   - `setFrameIndex()` triggers React re-render → twinki `commitUpdate` → `requestRender()`
   - **PieSpinner is NOT inside a twinki `<Region>`** — so every frame update triggers a full-tree render
   - twinki has `frameBudgetMs=0` (no frame pacing) — every `requestRender()` executes on next tick
   - Full yoga layout of 8620 nodes takes 72ms
   - Net: 150ms interval → render starts → 72ms render → next interval fires → repeat at ~14Hz

3. **Why it doesn't self-correct**: The tool call remains `isFinished=false` indefinitely because:
   - The MCP server died, so no `ToolCallFinished` event will ever arrive
   - `cancelMessage()` marks unfinished tools as finished, but it's only called on explicit user cancel
   - There's no timeout to auto-finish stuck tool calls

### What's Proven vs. Hypothesized

**Proven by forensic evidence:**
- ✅ Bun process at 98% CPU, Rust backend at 0% — problem is purely in TUI JS
- ✅ Call stack is deeply recursive JS function calls in JSC interpreter (React reconciler + yoga)
- ✅ 72ms/render, 8620 yoga nodes, ~14 renders/sec — matches PieSpinner 150ms interval throttled by render cost
- ✅ 0MB heap growth — not a memory leak, purely CPU
- ✅ MCP server re-init at 08:24 correlates with CPU spike onset
- ✅ Code path exists: unfinished tool → PieSpinner → no Region → full-tree render

**Not proven (would require reproduction):**
- ❓ Whether a tool call was actually stuck with `isFinished=false` (process died before store inspection)
- ❓ Whether PieSpinner specifically (vs. another animation) was the trigger
- ❓ Whether the MCP re-init was the causal trigger (vs. temporal correlation)

## Proposed Fixes

### Immediate (any one fixes the symptom)

1. **Wrap spinners in `<Region>`** in `StatusBar.tsx` — scopes re-renders to just the spinner character, not the full 8620-node tree
   ```tsx
   // StatusBar.tsx, around line 183
   <Region id={`spinner-${id}`}>
     <PieSpinner color={pieColor} />
   </Region>
   ```

2. **Set `targetFps: 30`** on the twinki `render()` call — adds frame pacing as a safety net (caps at 30 renders/sec regardless of setState frequency)

3. **Pause spinners in static scrollback** — the `paused` prop exists on PieSpinner/Spinner but is never passed from StatusBar

### Structural (prevents the trigger)

4. **Auto-finish stuck tool calls** — add a timeout (e.g. 120s) that marks tool calls as finished with a timeout error if no `ToolCallFinished` arrives

5. **Mark tools finished on MCP server death** — when `McpServerEvent::InitializeError` fires, mark any in-flight tool calls from that server as finished

6. **Ensure `cancelMessage()` fires on stream errors** — the `sendMessage` catch block marks tools finished, but verify it covers all MCP failure paths

## Reproduction Probe

Created at `packages/tui/scripts/probes/spinner-render-loop.ts`. Launches TUI in test mode, injects an unfinished tool call, measures idle CPU.

```bash
cd packages/tui && bun run build && cd ../..
bun run packages/tui/scripts/probes/spinner-render-loop.ts
```

Pass criteria: idle CPU ≤ 10% with an unfinished tool call mounted.

## CPU Profile Confirmation

A 19-minute CPU profile (`CPU.1813860883002.44612.cpuprofile`, 116,492 samples) was captured from a reproduction of the same issue:

### Top Functions by Self Time

| Samples | % Self | Function |
|---------|--------|----------|
| 31,329 | 26.9% | `OO` (twinki reconciler, tui.js:186) |
| 17,460 | 15.0% | `.wasm-function[42]` (yoga layout) |
| 14,335 | 12.3% | `.wasm-function[43]` (yoga layout) |
| 10,505 | 9.0% | `_doRenderInner` (twinki render entry, tui.js:164) |
| 7,836 | 6.7% | `.wasm-function[52]` (yoga layout) |
| 6,900 | 5.9% | `.wasm-function[119]` (yoga layout) |
| 4,378 | 3.8% | `.wasm-function[115]` (yoga layout) |
| 2,385 | 2.0% | `applyLineResets` (twinki terminal output, tui.js:164) |

### Breakdown by Source

| Source | % CPU | Role |
|--------|-------|------|
| **WASM (yoga layout)** | **49.4%** | `calculateLayout` recursing through 8620 nodes |
| **tui.js:186** (twinki reconciler) | **27.9%** | React reconciler tree diff (`OO`, `NIe`) |
| **tui.js:164** (twinki doRender) | **11.2%** | Render entry point (`_doRenderInner`, `doRender`) |
| Native | 3.9% | `spawnSync`, misc |
| tui.js:189 (components) | 1.7% | `Text` component rendering, `getMetrics` |

**Conclusion**: 88.5% of CPU is spent in yoga layout + React reconciliation + render dispatch. The TUI is continuously re-rendering the full 8620-node tree at ~14Hz while idle. This conclusively proves a render loop — not a memory leak, not I/O, not GC.

## Files Involved

| File | Role |
|------|------|
| `packages/tui/src/components/ui/spinner/PieSpinner.tsx` | 150ms setInterval, no Region |
| `packages/tui/src/components/ui/spinner/Spinner.tsx` | 100ms setInterval, same issue |
| `packages/tui/src/components/chat/status-bar/StatusBar.tsx:156` | Mounts PieSpinner when `status === 'executing'` |
| `packages/tui/src/components/ui/ToolUseMessage.tsx:86` | Sets `statusIcon='executing'` when `!isFinished` |
| `packages/tui/src/components/ui/ConversationView.tsx:210` | ActiveTurnTail passes `isFinished={message.isFinished}` |
| `packages/twinki/packages/twinki/src/renderer/tui.ts:203` | `frameBudgetMs = 0` (no frame pacing) |
| `packages/twinki/packages/twinki/src/renderer/tui.ts:884` | `requestRender()` fires on next tick when no budget |
| `crates/chat-cli-v2/src/agent/acp/acp_agent.rs:2478` | MCP InitializeError sends notification + re-advertises |
