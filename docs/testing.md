# Testing Architecture

Last updated: 2026-06-26

## Overview

Kiro CLI uses a layered test architecture. Each layer tests a different boundary,
from isolated functions to full-process behavioral verification across platforms.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CI Gate (PR-blocking)                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  Rust Unit Tests                   ~2,650 #[test]                    │   │
│  │  agent (1,499) · chat-cli-v2 (575) · chat-cli (583)                 │   │
│  │  Logic, tool execution, MCP protocol, session, model routing         │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  TUI Unit Tests                    ~3,000 tests                      │   │
│  │  bun:test (2,978) · vitest (149) · twinki (1,331)                   │   │
│  │  Component logic, store, parsers, ACP client, renderer internals     │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  Integration Tests                 32 tests                          │   │
│  │  Real PTY, no mock LLM. Pure TUI input/rendering.                    │   │
│  │  Engine-agnostic — works identically on Rust ACP and KAS.            │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  ACP Integration Tests             34 tests                          │   │
│  │  ACP wire-to-UI. Scripted ACP messages → verify rendered output.     │   │
│  │  The definitive V3 (KAS) protocol compliance tests.                  │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  E2E Tests                         67 tests (31 true E2E + 27 UI)    │   │
│  │  Full chat_cli binary in real PTY with mock LLM responses.           │   │
│  │  Tests the complete agent → ACP → TUI → rendered output pipeline.    │   │
│  │  27 UI-layer tests don't use mock LLM — planned move to integ.       │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                       Release Gate (on-demand)                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  Smoke Tests                       56 scenarios × 6 configs          │   │
│  │  Full binary, real PTY, real agent, real LLM.                        │   │
│  │  3 platforms × 2 engines = 6 configurations.                         │   │
│  │  Currently: LLM-driven exploratory (Knight Rider).                   │   │
│  │  Future: mechanical predicate executor (PR-blocking).                │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## What "E2E" Means to Us

We use "E2E" specifically to mean: **the full compiled binary runs in a real PTY with
scripted (mock) LLM responses, exercising the complete agent → ACP protocol → TUI
rendering pipeline.** This is distinct from integration tests which only exercise
the TUI layer, and from smoke tests which use a real LLM.

```
                    What each layer exercises
                    ─────────────────────────

  Unit:          ┌─────────┐
                 │ function │  (isolated logic)
                 └─────────┘

  Integration:   ┌─────────────────────────────────┐
                 │ TUI + PTY + input + rendering    │  (no agent, no LLM)
                 └─────────────────────────────────┘

  ACP Integ:     ┌───────────────────────────────────────────┐
                 │ ACP messages → TUI → rendered screen       │  (scripted ACP, no LLM)
                 └───────────────────────────────────────────┘

  E2E:           ┌────────────────────────────────────────────────────────┐
                 │ chat_cli binary → agent → mock LLM → ACP → TUI → PTY  │
                 └────────────────────────────────────────────────────────┘

  Smoke:         ┌──────────────────────────────────────────────────────────────┐
                 │ chat_cli binary → agent → REAL LLM → ACP → TUI → PTY → eyes │
                 └──────────────────────────────────────────────────────────────┘
```

### The E2E boundary

```
┌─────────────────────────── E2E test harness ──────────────────────────────┐
│                                                                            │
│  ┌─────────────────── chat_cli process (real binary) ──────────────────┐   │
│  │                                                                      │  │
│  │  ┌──────────┐    ┌───────────┐    ┌─────────────────┐               │  │
│  │  │  Agent   │───▶│  Mock LLM │    │  MCP servers    │               │  │
│  │  │  crate   │◀───│  (JSONL)  │    │  (if needed)    │               │  │
│  │  └────┬─────┘    └───────────┘    └─────────────────┘               │  │
│  │       │                                                              │  │
│  │       │ ACP (stdio)                                                  │  │
│  │       ▼                                                              │  │
│  │  ┌──────────────────────────────────────────────┐                    │  │
│  │  │  TUI (TypeScript)                            │                    │  │
│  │  │  ┌────────┐  ┌──────────┐  ┌────────────┐   │                    │  │
│  │  │  │ Store  │  │ Renderer │  │ Components │   │                    │  │
│  │  │  └────────┘  └──────────┘  └────────────┘   │                    │  │
│  │  └──────────────────────────────────────────────┘                    │  │
│  │       │                                                              │  │
│  │       │ ANSI escape sequences                                        │  │
│  │       ▼                                                              │  │
│  │  ┌──────────┐                                                        │  │
│  │  │   PTY    │                                                        │  │
│  │  └──────────┘                                                        │  │
│  │                                                                      │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│       │                                           ▲                        │
│       │ terminal output                           │ keystrokes (IPC)       │
│       ▼                                           │                        │
│  ┌──────────┐                                ┌────────────┐                │
│  │ Snapshot │  ◀── assertions ──────────────▶│ Test logic │                │
│  │ capture  │                                └────────────┘                │
│  └──────────┘                                                              │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

The test injects mock LLM responses via JSONL files. The agent processes them exactly
as it would real responses. The ACP protocol carries structured messages to the TUI.
The TUI renders to a PTY. The harness captures terminal snapshots and asserts on
rendered content.

## Layer Details

### Rust Unit Tests

| Crate | Tests | Focus |
|-------|-------|-------|
| `agent` | 1,499 | Tool execution, MCP protocol, model routing, session state, error handling |
| `chat-cli-v2` | 575 | ACP agent, command parsing, config, telemetry |
| `chat-cli` | 583 | V1 agent, CLI parsing, session persistence |

Run with: `cargo test -p agent`, `cargo test -p chat_cli_v2`, `cargo test -p chat_cli`

CI: `rust.yml` — Linux + macOS. PR-blocking.

### TUI Unit Tests

| Runner | Tests | Focus |
|--------|-------|-------|
| `bun:test` | 2,978 | Component logic, store, parsers, ACP client, input handling |
| `vitest` | 149 | Hook/selector coverage (bun can't attribute reconciler execution) |
| `twinki` | 1,331 | Renderer internals, layout engine, box drawing, diff algorithm |

Run with: `cd packages/tui && bun test` / `bun run test:vitest` / `cd packages/twinki && bun test`

CI: `tui.yml` — Linux. PR-blocking.

### Integration Tests (32 tests)

Real PTY, no agent, no LLM. Tests the TUI input/rendering layer in isolation.
Engine-agnostic — works identically whether the backend is Rust ACP or KAS.

These tests validate that the TUI correctly handles:
- Keyboard input (shortcuts, word movement, word deletion, undo)
- Multiline editing (logical lines, zero-width characters, shift+enter)
- Session lifecycle (boot, resume, display settings)
- UI elements (welcome message, exit hint, interrupt mode, status bar)
- Commands (slash autocomplete, reverse search, transcript export)

Run with: `cd packages/tui && bun run test:integ`

CI: `tui.yml` — Linux + macOS + Windows. PR-blocking.

### ACP Integration Tests (34 tests)

ACP wire-to-UI. Scripted ACP protocol messages are fed directly to the TUI — no real
agent process. Tests that the TUI correctly renders every ACP message type.

This is the definitive V3 (KAS) compliance layer. If a KAS behavior renders wrong,
the failure shows up here.

Tests cover: protocol handshake, agent lifecycle notifications, tool rendering,
permission flows, markdown rendering (basic, nested, spacing), model switching,
MCP status/OAuth/transforms, slash commands (/spec, /knowledge, /code, /context,
/compact, /effort, /usage, /prompts, /rewind, /chat), session resume, search
rendering, pipeline orchestration, interaction hints, terminal banner.

Run with: `cd packages/tui && bun run test:acp-integ`

CI: `tui.yml` — Linux + macOS + Windows. PR-blocking.

### E2E Tests (67 tests — 31 true E2E + 27 UI-layer + 9 hybrid)

Full `chat_cli` binary running in a real PTY. The **true E2E** tests inject mock LLM
responses via IPC, exercise the complete agent → ACP → TUI pipeline, and assert on
rendered output. The **UI-layer** tests don't mock the LLM at all — they only test
TUI behavior and should eventually move to `integ_tests/`.

**True E2E (31 tests — mock LLM, test agent→UI flow):**

These inject mock responses via `pushMockSendMessageResponse()` and verify the full
pipeline from agent processing through to rendered terminal output:

- Tool approval flows (drill-in, keybindings, failed status)
- Streaming (incremental flush, scroll, turn summary)
- Cancel/recovery (mid-stream cancel, timeout, queue-during-init)
- Session features (compact, rewind, chat switching, copy, goal)
- Rendering (markdown wrap, write-diff, paste-image, disable-wrap)
- Performance (input latency, memory leak detection)
- Subagent lifecycle (stage failure)
- Settings (show-thinking)

**UI-layer tests (27 tests — no mock LLM, engine-agnostic):**

These spawn the binary but never inject LLM responses. They test TUI-layer behavior
that doesn't depend on the agent engine. They belong in `integ_tests/` — the current
placement is historical:

| Test | Feature |
|------|---------|
| `context-breakdown` | Context breakdown panel |
| `context-skill-files` | Skill files in context |
| `crew-monitor` | Crew monitor UI |
| `effort-status-bar` | Effort indicator in status bar |
| `fast-typing` | No dropped chars at 10ms/5ms intervals |
| `greeting-setting` | Welcome screen on/off |
| `init-failure-notifications` | Init failure notifications |
| `interactive-session-chat` | Session interaction static checks |
| `mcp-and-tools-panel` | MCP/tools panel navigation |
| `multi-agent-notifications` | Multi-agent notification routing |
| `persist-by-default` | Session auto-persistence |
| `session-archive` | Export/import session |
| `settings-cli-interrupt-behavior` | CLI interrupt behavior setting |
| `settings-interrupt-behaviour` | Interrupt behavior setting |
| `settings-theme` | Theme panel → apply → persist |
| `shadow-text-autocomplete` | Autocomplete ghost text |
| `shell-escape` | `!command` execution |
| `shell-escape-paste` | Paste after shell escape |
| `shell-output-collapse` | Long output collapse |
| `signal-exit` | Clean exit on signal |
| `slash-commands` | `/clear` and slash command execution |
| `stats-panel` | Stats panel display |
| `subagent-visibility` | Subagent panel visibility |
| `terminal-title` | Terminal title setting |
| `timeout-cancel-recovery` | Timeout + cancel recovery |
| `unknown-agent-fallback` | Fallback when agent not found |
| `v2-skills-advertise` | Skills advertisement |

**Planned reorganization:** Move these 27 tests to `integ_tests/`. This makes the
boundary clear — `e2e_tests/` = full pipeline with mock LLM, `integ_tests/` = TUI
behavior only. Both run on all 3 platforms in CI.

Run with: `cd packages/tui && bun run test:e2e`

CI: `tui.yml` — Linux + macOS + Windows. PR-blocking.

### Smoke Tests (56 scenarios)

All user-facing features exercised against the **real binary with a real LLM** —
no mocks. Tests real authentication, real model responses, real MCP servers.

```
smoke-tests.yml
├── Platforms: Linux, macOS, Windows
├── Engines: Rust ACP, KAS
└── = 6 configurations per run
```

**Current state:** LLM-driven exploratory via Knight Rider. An LLM agent reads
`scenarios.json`, drives the TUI through the Knight Rider HTTP API, observes rendered
frames, and flags visual/behavioral regressions.

**Future state:** Mechanical predicate executor. See "Planned: Mechanical Smoke Tests"
below.

Scenarios by category:

| Category | Count | Examples |
|----------|-------|---------|
| basic | 1 | Boot — TUI launches, prompt ready |
| slash-commands | 38 | Every `/command` (autocomplete, help, model, agent, tools, context, etc.) |
| tool-use | 5 | Read, shell, write-approval, cancel, cancel-recovery |
| conversation | 5 | Basic prompt, context retention, clear-and-resume, long response, markdown |
| keyboard | 4 | Ctrl-C exit, history-up, Ctrl-J newline, Ctrl-S search |
| permissions | 2 | Trust flow, trust-all reset |
| subagents | 1 | Spawn and monitor |

CI: `smoke-tests.yml` — on-demand + callable from release pipeline. Not PR-blocking
(requires real LLM access).

---

## Planned: Mechanical Smoke Tests

The 56 scenarios in `scenarios.json` each have a `verify` field with mechanical
predicates. The goal is to wire these into a deterministic executor that hard-fails
on every PR — no LLM needed.

```
┌──────────────────────────────────────────────────────────────────────────┐
│                    Mechanical Smoke Test Runner                           │
│                                                                          │
│  scenarios.json                                                          │
│  ┌──────────────────────────────────────────────────────────┐            │
│  │ { "id": "slash-help",                                    │            │
│  │   "steps": ["type:/help", "enter"],                      │            │
│  │   "verify": ["screen.contains:/<COMMAND>"] }             │            │
│  └──────────────────────────────────────────────────────────┘            │
│       │                                                                  │
│       ▼                                                                  │
│  ┌────────────────────────────────────────────────────────┐              │
│  │ Step executor                                          │              │
│  │  "type:/help" → send keystrokes to PTY                 │              │
│  │  "enter"      → send \r                                │              │
│  │  wait for settle (no new output for Nms)               │              │
│  └────────────────────────────────────────────────────────┘              │
│       │                                                                  │
│       ▼                                                                  │
│  ┌────────────────────────────────────────────────────────┐              │
│  │ Predicate evaluator                                    │              │
│  │  "screen.contains:/<COMMAND>" → getSnapshot() + match  │              │
│  │  "store.mode:plan"            → getStore() + assert    │              │
│  │  "exit.code:0"                → expectExit(0)          │              │
│  └────────────────────────────────────────────────────────┘              │
│       │                                                                  │
│       ▼                                                                  │
│  PASS / FAIL (deterministic, no LLM, <5s per scenario)                   │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

This turns 56 scenarios into 56 additional deterministic tests that:
- Run on every PR (fast — no LLM, no network)
- Cover multi-step user flows (type → autocomplete → select → panel → content)
- Use the same scenario definitions as the exploratory smoke tests
- Hard-fail the build if a user-facing feature breaks

The LLM-driven exploratory mode (Knight Rider) remains for visual verification and
regression detection that predicates can't catch (layout correctness, color contrast,
animation smoothness). Both modes share `scenarios.json` as the single source of truth.

---

## CI Workflow Map

```
┌─────────────────────────────────────────────────────────┐
│ PR opened / updated                                     │
│                                                         │
│  rust.yml ──────────────────────────────────────────┐   │
│  │  clippy + build + test (agent, v1, v2)           │   │
│  │  Linux + macOS                                   │   │
│  └──────────────────────────────────────────────────┘   │
│                                                         │
│  tui.yml ───────────────────────────────────────────┐   │
│  │  Unit tests (bun + vitest + twinki)    Linux     │   │
│  │  Integration tests          Linux+macOS+Windows  │   │
│  │  ACP integration tests      Linux+macOS+Windows  │   │
│  │  E2E tests                  Linux+macOS+Windows  │   │
│  └──────────────────────────────────────────────────┘   │
│                                                         │
│  ════════════════ All must pass to merge ═══════════════ │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│ Release (build-and-release.yml calls smoke-tests.yml)   │
│                                                         │
│  smoke-tests.yml ───────────────────────────────────┐   │
│  │  56 scenarios × 3 platforms × 2 engines          │   │
│  │  Real LLM, real binary, real MCP                 │   │
│  │  Evidence archived to S3                         │   │
│  └──────────────────────────────────────────────────┘   │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

---

## Running Tests Locally

```bash
# Rust unit tests
cargo test -p agent
cargo test -p chat_cli_v2
cargo test -p chat_cli

# TUI unit tests
cd packages/tui && bun test

# Vitest (hooks/selectors coverage)
cd packages/tui && bun run test:vitest

# Twinki unit tests
cd packages/twinki && bun test

# Integration tests (TUI + PTY, no agent)
cd packages/tui && bun run test:integ

# ACP integration tests (ACP wire-to-UI)
cd packages/tui && bun run test:acp-integ

# E2E tests (full binary + mock LLM)
cd packages/tui && bun run test:e2e

# Specific E2E test
cd packages/tui && bun run test:e2e e2e_tests/cancel-state-recovery.test.ts

# Smoke tests (requires real LLM credentials)
# Via CI: gh workflow run smoke-tests.yml
# Via Knight Rider locally: bun run knight-rider
```

---

## Adding Tests

### New Rust unit test
Add `#[test]` in the relevant crate. Follow existing patterns in the same file.

### New integration test
Create `packages/tui/integ_tests/<name>.test.ts`. Tests must:
- Use `TestCase` from `../src/test-utils/shared`
- Spawn TUI in a PTY via `testCase.spawn()`
- Drive with `testCase.sendKeys()`, `testCase.type()`
- Assert with `testCase.waitForStore()`, `testCase.getSnapshot()`

### New ACP integration test
Create `packages/tui/acp_integ_tests/<name>.test.ts`. Tests must:
- Use `AcpTestCase` from test utilities
- Feed scripted ACP messages via `testCase.sendAcpMessage()`
- Assert rendered output via `testCase.getSnapshot()`

### New E2E test
Create `packages/tui/e2e_tests/<name>.test.ts`. Tests must:
- Use `E2ETestCase` from test utilities
- Provide mock LLM responses in `e2e_tests/mock-responses/<name>.jsonl`
- Spawn the real `chat_cli` binary
- Assert on rendered terminal output + agent state via IPC

### New smoke scenario
Add an entry to `packages/tui/e2e_tests/smoke/scenarios.json`:
```json
{
  "id": "my-feature",
  "name": "My feature description",
  "category": "slash-commands",
  "description": "What this scenario tests",
  "docRef": "/docs/cli/reference/...",
  "steps": ["type:/mycommand", "enter"],
  "verify": ["screen.contains:expected text"],
  "observe": "What the LLM should look for visually"
}
```

The `verify` field is for the mechanical executor. The `observe` field guides the
LLM-driven exploratory mode.

---

## Scenario Sync

Scenarios are derived from the public docs. To detect missing coverage:

```bash
cd packages/tui && bun run e2e_tests/smoke/sync-scenarios.ts
```

This compares `scenarios.json` against the docs slash-commands reference page and
reports commands without coverage. Use `--apply` to update `scenarios.json` with
stubs for missing scenarios.
