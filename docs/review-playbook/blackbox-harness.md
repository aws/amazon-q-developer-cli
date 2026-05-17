# Blackbox harness guide

This file describes *how* to actually run blackbox techniques from the review playbook against this project. The reviews tell you what to probe; this file tells you which tools to use, how to run them across operating systems, and how to wire kiro-cli as its own exploratory-test driver.

If a review asks you for a "blackbox probe" and you are not sure where to start, start here.

## Contents

- [Existing harnesses](#existing-harnesses)
- [Which harness for which job](#which-harness-for-which-job)
- [Writing ad-hoc probes](#writing-ad-hoc-probes)
- [Running cross-platform](#running-cross-platform)
- [Self-driving: kiro-cli reviewing kiro-cli](#self-driving-kiro-cli-reviewing-kiro-cli)
- [Capturing evidence](#capturing-evidence)
- [Budget gating in CI](#budget-gating-in-ci)

## Existing harnesses

These harnesses already exist in the project. Prefer extending them over writing something new from scratch.

### Unit (`bun test`, vitest)

- Location: colocated `__tests__/` directories, plus any `*.test.ts` in a package.
- Entry point: `bun run test` (bun's test runner), `bun run test:vitest` (vitest).
- Good for: pure-function properties, bounded-memory unit probes (call N times in a loop, assert RSS), regex timeout fuzz, per-function allocation budgets.
- Not good for: anything that needs a real PTY, multi-process coordination, or real terminal capabilities.

### Integration (`integ_tests/`)

- Location: `packages/tui/integ_tests/*.test.ts`.
- Entry point: `bun run test:integ` (`bun test ./integ_tests/`).
- Good for: input-editing logic, keybindings, reverse-search, word-movement, welcome-message, lifecycle — anything above a unit test but below a full e2e binary.
- These tests usually mock the backend.

### E2E (`e2e_tests/` + `E2ETestCase`)

- Location: `packages/tui/e2e_tests/*.test.ts`.
- Entry point: `bun run test:e2e` (runs `packages/tui/scripts/run-e2e.ts`, which builds the Rust backend and the TUI bundle first).
- Driver: `E2ETestCase` (spawns a real `kiro-cli chat` process, attaches dual IPC connections — one to the TUI Zustand store, one to the Rust backend — and gives you a snapshottable PTY).
- Helper: `AcpTestHelper` (spawns a second `chat_cli acp` process and injects mock stream responses via IPC).
- Good for: any probe that needs the real binary running in a real PTY — memory-leak tests (`memory-leak.test.ts` is the canonical example), incremental flush, cancel-state recovery, resume-session.
- Supports: scripted input, mock assistant responses (via `pushResponse`), inspection of store state, PTY snapshots (text + cell attributes).

### Knight Rider (exploratory, LLM-drivable)

- Location: `packages/tui/e2e_tests/knight-rider.ts`.
- Entry point: `bun run knight-rider` (or `bun run knight-rider --system` to drive the installed binary, `--v1` for legacy, `--cmd "..."` for anything else).
- What it gives you: an HTTP server with `/api/keys`, `/api/enter`, `/api/ctrlc`, `/api/wait-for-text`, `/api/sleep`, `/api/frame`, `/api/screen`, `/api/screen/html`. It captures named frames to disk as HTML + plain text. Also exposes `/ws` for live raw-PTY streaming and `/report` for a self-contained offline evidence report.
- Good for: anything exploratory. LLM-driven tests (an agent can POST keys and read the screen back), demonstrations, reproducing user-reported bugs, manually driving the TUI while scripting observations.
- Output: self-contained HTML report that replays frames as a video — useful for attaching to issues and COE docs.

### Profiling (`bun --cpu-prof`, `--heap-snapshot`)

- Location: `packages/tui/scripts/start-dev-profile.ts`, `analyze-profile.ts`, `diff-profiles.ts`.
- Entry point: `bun run dev:profile` produces a `.cpuprofile`; `bun run analyze:profile --html` turns it into an HTML report.
- Good for: any Review 6 technique (profile-driven review, streaming-throughput benchmark, idle-CPU soak). `docs/bun-performance-analysis.md` is the canonical example of how to use these outputs.

### Microbenchmarks (`scripts/benchmark-*.ts`)

- Location: `packages/tui/scripts/benchmark-*.ts`, invoked via `bun run bench:*` scripts.
- Entry point example: `bun run bench:markdown`.
- Good for: regression-catching on hot pure functions. Extend this pattern to any new hot function identified by a cpuprofile.

### PTY driver (`PtyManager`)

- Location: `packages/tui/src/test-utils/shared/pty-manager.ts`.
- Used by: Knight Rider, E2ETestCase, most ad-hoc probes.
- What it gives you: programmatic PTY (real terminal semantics), terminal snapshots (text grid + attribute grid + cursor), keystroke injection, resize control, and deterministic timing hooks. Critical for testing anything width- or resize-sensitive.

### IPC helpers (`TuiIpcConnection`, `AcpTestHelper`)

- Location: `packages/tui/src/test-utils/shared/tui-ipc-connection.ts`, `packages/tui/e2e_tests/AcpTestHelper.ts`.
- What they give you: in-process visibility into the running binary. Snapshot the Zustand store, push mock ACP responses, invoke extension methods directly.
- Good for: anything that needs to assert internal state (store invariants) or inject failures at the protocol layer (malformed payloads, out-of-order notifications).

## Which harness for which job

Map each review's blackbox techniques to the cheapest harness that can do the job:

| Review | Technique kind | Suggested harness |
|--------|----------------|-------------------|
| 1 Async render | Resize storm, backed-up PTY, long soak | `PtyManager` ad-hoc + `process.memoryUsage()` sampling, or extend `memory-leak.test.ts` |
| 2 Yoga | Bounded-memory unit probes, fuzz, ANSI injection | `bun test` for unit, `fast-check` for fuzz, PtyManager for live-squeeze |
| 3 Unbounded growth | Session-lifetime, heap-snapshot diff, subscription stress | `E2ETestCase` + `--heap-snapshot`; `memory-leak.test.ts` is the template |
| 4 Dead FDs | Suspend/resume, SSH-disconnect, FD-exhaustion, terminal-close | Shell-scripted probes around `bun run release` + Knight Rider for observability |
| 5 Recursion | Adversarial corpus, stack-size probe, regex timeout fuzz | `bun test` with `fast-check` or a handwritten corpus |
| 6 Hot path | Streaming-throughput benchmark, idle-CPU soak | `dev:profile` → `analyze:profile`, or extend `bench:markdown` pattern |
| 7 Concurrency | Delivery-delay fuzz, concurrent-op stress, rapid-cancel | `AcpTestHelper` with randomised delays; extend `cancel-state-recovery.test.ts` |
| 8 Dependency | Contract replay, per-method smoke, startup-cost | Fixture replay test via `AcpTestHelper`; `time` around the release binary |
| 9 Terminal | No-TTY, color round-trip, headless, real-terminal matrix | Scripted probes for no-TTY; manual real-terminal matrix with Knight Rider evidence |
| 10 Cross-platform | Smoke on each OS, full e2e matrix, clipboard, filesystem corners | CI matrix + Knight Rider on each OS |
| 11 Type drift | Wire-payload replay, contract test, malformed-payload fuzz | `AcpTestHelper` + fixture files under `e2e_tests/fixtures/` |
| 12 Exception | Fault-injection, disk-full, network-fault matrix, process-death | `E2ETestCase` + `/tmp`-scoped chaos (`ulimit`, `chmod 000`, `kill -9`) |
| 13 Test coverage | Smoke-run in CI, chaos day, property-based | All of the above, plus `.github/workflows/` matrix |

## Writing ad-hoc probes

Not every probe deserves a permanent test. Use this template for a throwaway run:

```ts
// packages/tui/scripts/probe-<topic>.ts
import { PtyManager } from "../src/test-utils/shared/pty-manager";
import { spawn } from "node:child_process";

const pty = await PtyManager.spawn({
  command: "bun",
  args: ["run", "dist/tui.js"],
  cols: 120,
  rows: 40,
});

// Drive the probe.
await pty.write("hello\r");
await pty.waitForText("ready", { timeout: 5000 });

// Measure what you care about.
// const rssMb = ... ; // from /proc or ps
// const snap = pty.snapshot();

// Report.
console.log(JSON.stringify({ rssMb, lineCount: snap.lines.length }));

await pty.kill();
```

Run with `bun run scripts/probe-<topic>.ts`. If the probe turns into a regression catcher, promote it into `e2e_tests/` or `integ_tests/`.

If the probe needs to drive a long sequence of user actions, prefer starting a Knight Rider server and scripting it from a separate file — that way you get the HTML evidence report for free.

## Running cross-platform

The same probe may behave differently on Linux, macOS, and Windows. Every blackbox technique should state which platforms it has been verified on. The three baseline environments:

### Linux

- GitHub Actions `ubuntu-latest` is the default CI target.
- For local runs, a headless server works: `bun run test:e2e` inside a container.
- SIGWINCH, SIGPIPE, SIGHUP all behave as documented. Use as the reference POSIX environment.
- Filesystem is case-sensitive by default.

### macOS

- GitHub Actions `macos-latest` (M-series) is the CI target.
- iTerm2, Terminal.app, Ghostty, Kitty are the realistic real-terminal targets.
- Default filesystem is case-insensitive but case-preserving — different from Linux; guards against case-dependency bugs here pay off.
- `osascript` for notifications; `pbcopy` / `pbpaste` for clipboard probes.

### Windows

- GitHub Actions `windows-latest` is the CI target.
- Windows Terminal, PowerShell, `cmd.exe`, ConHost are the realistic real-terminal targets.
- SIGPIPE / SIGHUP do not exist; SIGINT semantics differ; `CTRL_BREAK_EVENT` is the closest equivalent for "cancel-like" signals.
- Paths use backslash, line endings default to CRLF, case is insensitive.
- Many probes that rely on `ulimit`, `chmod`, or POSIX signals need a Windows-specific path.

### Minimum viable cross-platform run

For any blackbox probe that could be platform-sensitive, before declaring "done":

1. Run on Linux (CI or local).
2. Run on macOS (local or CI).
3. Run on Windows (CI or a Windows VM / Parallels / UTM image).
4. Record results in the finding table's "platforms verified" column.

### CI matrix template

```yaml
jobs:
  probe:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
        with: { bun-version: <pinned> }
      - run: bun install
      - run: bun run test:e2e
```

Every new blackbox probe committed to the repo should either (a) pass on all three runners, or (b) gate itself on `runner.os` with a dated comment explaining which platforms it intentionally skips.

## Self-driving: kiro-cli reviewing kiro-cli

kiro-cli can drive its own tests and reviews. This is the highest-leverage pattern in the playbook: put the model itself on the far side of the harness, and let it do the exploratory work a human would otherwise have to do by hand. Three concrete patterns:

### Pattern 1 — Agent-driven Knight Rider session

Knight Rider already exposes an HTTP API that does not require a human in the loop. Run it headless, point an agent (either kiro-cli itself through its own ACP agent, or a subagent via kiro-cli's multi-agent infra) at the API, and feed it a prompt like:

> Drive the TUI through the following scenarios, capturing named frames for each. Report any visual glitch, any hang, any error. Scenarios: (1) open a large markdown file, (2) paste 10 000 lines, (3) trigger a tool call that fails, (4) resize the window rapidly, (5) press Ctrl-C during streaming.

The agent uses the Knight Rider API (POST keys, GET screen, POST frame) to explore. The output is the HTML report and a natural-language review. This is equivalent to hiring a tireless QA tester — and is exactly the shape of blackbox review that the playbook calls for but is too expensive to run by hand.

### Pattern 2 — Subagent-per-review

kiro-cli supports subagents (see the multi-agent RFC and `subagent.md`). One subagent per review works well:

- One subagent per review file (13 + the test-harness review = 14 subagents, or batch them).
- Each subagent receives: the review file, the current codebase path, the finding-table template, and a budget (wall-clock and tokens).
- Each subagent runs the [code] techniques first (cheap, always terminates), then the [blackbox] techniques it has infrastructure for.
- Outputs roll up into a single finding table.
- The orchestrator (kiro-cli itself) reconciles duplicates, ranks by severity, and produces the report.

Rough orchestration:

```text
kiro-cli chat --prompt "Run review 4 from docs/review-playbook/04-dead-fds.md.
  Scope: packages/tui/, packages/twinki/packages/twinki/.
  Output: findings table in docs/review-playbook/findings/review-04.md.
  Budget: 60 min wall-clock, 500k tokens.
  For blackbox techniques, use the harnesses in docs/review-playbook/blackbox-harness.md.
  If a probe is not feasible in this environment, note it explicitly."
```

Run the same prompt 13 times with different review numbers. For truly long-running probes (4-hour soak tests), have the subagent produce a runbook instead of running the probe itself, and run the probe separately on a dedicated machine.

### Pattern 3 — LLM as fault-injection fuzzer

For Review 11 (type drift) and Review 12 (exception handling), the most effective blackbox technique is often "malformed input fuzzing where the fuzzer knows the grammar". An LLM is better at this than a dumb fuzzer because it can generate inputs that are *almost* valid — the hard cases. Prompt:

> Generate 50 ACP session-update payloads that are schematically valid JSON but contain subtle shape violations (wrong types, missing required fields, null where undefined, enum variants that do not exist). For each, note the expected failure mode.

Feed the 50 payloads into `AcpTestHelper.pushResponse` and assert the TUI surfaces each failure cleanly without crashing. Capture any crash as a finding.

### Where to put self-driving outputs

Suggested layout:

```
docs/review-playbook/
  findings/
    review-01.md        # output of subagent for review 1
    review-02.md
    ...
    rollup.md           # orchestrator roll-up
  evidence/
    knight-rider-<date>/  # Knight Rider report directories
    cpuprofile-<date>.html
    heap-snapshot-<date>.heapsnapshot
```

Findings live under `findings/`; raw evidence that proves the finding lives under `evidence/`. Both are intentionally outside the code tree so they can be committed per-review cycle without polluting the build.

## Capturing evidence

Every blackbox finding should have evidence attached. The four canonical evidence types:

1. **HTML report** — Knight Rider produces these out of the box. Also produced by `bun run analyze:profile --html`.
2. **Raw profile** — `.cpuprofile` (open in Chrome DevTools) or `.heapsnapshot`.
3. **Terminal snapshot** — `PtyManager.snapshot()` yields a structured text+attribute grid. Serialise as JSON.
4. **Metrics table** — CSV or Markdown. Columns recommended: `probe`, `platform`, `metric`, `baseline`, `measured`, `budget`, `status`.

For anything that will become a regression test, the evidence file becomes the test fixture.

## Budget gating in CI

Blackbox probes that produce a number (RSS, CPU%, render count, latency ms) should check that number against a budget. Three rules:

1. **Record a baseline once.** First successful run establishes the baseline. Store in a JSON file under `docs/perf-baselines/` (which already exists).
2. **Fail if a probe exceeds budget.** The budget is usually `baseline * 1.2` (20% headroom) or a hard absolute cap (for example "RSS must stay under 100 MB").
3. **Fail if a probe silently improves a lot** — a 50% improvement is almost always a test that stopped actually exercising the code path. Investigate before rebaselining.

See `docs/bun-performance-analysis.md` for a real example of baseline-vs-measured comparison.

## Cross-reference

- Main index: [README.md](README.md)
- Cross-cutting static-analysis tools: [cross-cutting.md](cross-cutting.md)
- Every review links to the techniques here by number — e.g. "Review 3 technique 7 uses `E2ETestCase`".
