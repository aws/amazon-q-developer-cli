---
program: testing-certification
doc_type: lld
id: "10"
title: Scenario Framework (schema, runner, assertions) — Low-Level Design
owner: Kenneth Sanchez
status: implementing
depends_on: ["F1"]
unblocks: ["12", "13", "14", "16"]
foundations_used: ["F1", "F3"]
hld: 00-hld.md
plan_phase: 0
---

# Scenario Framework (schema, runner, assertions) — Low-Level Design

Status: Implementing · Owner: Kenneth Sanchez · Date: 2026-08-10
Audience: TUI maintainers, testing-certification workstream owners
Companion documents: `00-hld.md`, `01-foundation-architecture.md`, `02-implementation-plan.md`
Related existing designs: `scenario-runner-v2-architecture.md`

---

## 1. Scope

This workstream defines the reusable scenario-runner substrate that every higher-level testing tier consumes. It covers the scenario manifest shape, shared runner loop, step execution, verify predicates, backend contract, and failure reporting. It does not define the scenario corpus itself, which belongs to workstream 12, and it does not define the KAS fake-model strategy, which belongs to workstream 11.

## 2. Foundation dependencies

The framework is the concrete implementation of F1 and is designed so live-driving improvements from F3 slot in without changing scenario authorship. The current implementation uses the existing E2E harness for live backends and does not require the act-react extension to be complete before deterministic execution lands.

| Foundation | What you consume | Assumption you are making |
|---|---|---|
| F1 scenario schema | `Scenario` shape, predicate vocabulary, tags, priority, runtime applicability | `scenarios.json` remains the single shared corpus and can express engine and backend allowlists without forking manifests |
| F2 KAS mock-LLM | None in the core runner contract | Mock strategy is a backend concern; the runner must not care whether responses come from ACP fixtures or a fake model |
| F3 Knight Rider driver | Live execution harness shape | Existing live harness APIs are sufficient for step execution now; richer goal-seeking behavior can replace the live backend later without changing scenario files |
| F4 design-system rubric | None | UX review consumes frame output from this runner but does not affect runner behavior |

## 3. Design

The framework lives under `packages/tui/e2e_tests/scenario-runner/` and is split into small contracts:

- `types.ts`: `Scenario`, `ScenarioBackend`, `TestHarness`, `RunOptions`, `ScenarioResult`, `RunReport`
- `runner.ts`: load, filter, execute, verify, emit frames, classify failures
- `steps.ts`: declarative step interpreter
- `predicates.ts`: machine assertions against snapshots, process state, and harness state
- `failure-context.ts`: post-failure markdown and JSON artifacts
- `backends/acp-mock.ts`: deterministic ACP-wire replay backend
- `backends/live.ts`: real TUI + real engine backend for `v2` and `kas`

The runner contract is backend-agnostic: a backend launches a `TestHarness`, the runner executes declarative steps, then assertions run over final state. Scenario filtering happens before execution and currently includes:

- engine allowlist via `scenario.engine`
- backend allowlist via `scenario.backend`
- explicit scenario ids
- categories
- tags
- priority tiers

The current shared manifest is `packages/tui/e2e_tests/smoke/scenarios.json`. The runner defaults to that file but accepts `scenariosPath` so other consumers can reuse the framework without copying logic.

Current backend split:

- `acp-mock`: deterministic, fixture-driven, currently KAS-only
- `live`: real PTY execution, supports `v2` and `kas`

Data flow:

```text
scenarios.json
  -> loadScenarios()
  -> filterScenarios()
  -> backend.launch()
  -> executeStep()
  -> assertVerify()
  -> RunReport + failure context
```

## 4. Interfaces you expose

The framework exposes these contracts to other workstreams:

- Scenario metadata fields in `scenarios.json`: `id`, `category`, `steps`, `verify`, `engine`, `backend`, `tags`, `priority`
- `ScenarioBackend` interface in `types.ts`
- `runAll()` and `runScenario()` from `runner.ts`
- CLI-facing filtering semantics consumed by `run-smoke.ts`
- Failure artifact layout produced by `failure-context.ts`

The most important stable contract is that scenario authors describe behavior declaratively and do not know which backend is in use.

## 5. Test strategy for this workstream

The framework is tested at the backend level, because the main failure mode is false green from a harness that never exercised real assertions. The deterministic ACP-mock path is the primary proof point because it runs the full runner loop without LLM variability. Failure-path coverage includes stale fixtures, missing fixtures, assertion failures, step timeouts, and backend startup failures.

## 6. Machine-checkable acceptance criteria

1. `bun packages/tui/e2e_tests/smoke/run-smoke.ts --backend acp-mock --engine kas --no-capture-frames` exits `0` and runs the full deterministic KAS corpus through the shared runner.
2. `bun -e "import { loadScenarios, filterScenarios } from './packages/tui/e2e_tests/scenario-runner/runner.ts'; const scenarios = loadScenarios(); const v2 = filterScenarios(scenarios, { backend: { id: 'live', engine: 'v2', launch: async ()=>{ throw new Error('noop'); } }, captureFrames: false }); const kas = filterScenarios(scenarios, { backend: { id: 'live', engine: 'kas', launch: async ()=>{ throw new Error('noop'); } }, captureFrames: false }); console.log(JSON.stringify({ total: scenarios.length, v2: v2.length, kas: kas.length }, null, 2));"` prints counts consistent with the manifest metadata.
3. A deliberately stale or missing fixture causes `run-smoke.ts` to exit non-zero with `fixture-missing` or `crash`, not false green.

## 7. Rollout and gating

This framework is not itself a gate; it is the substrate that gates attach to. Today it feeds the smoke consumer only. In the current branch that consumer has:

- deterministic PR gating on KAS via the ACP-mock backend
- live parity execution for `v2` and `kas`
- scenario selection driven by manifest runtime metadata

The next stacked PR extends this same framework with deterministic `v2` support. The escape hatch for backend-specific failures is to disable the consumer workflow lane, not to special-case logic in the runner.

## 8. Risks

- The runner can become smoke-specific again if consumers reach into implementation details instead of using the backend contract. Mitigation: keep all backend-specific logic behind `ScenarioBackend`.
- Schema churn after corpus tagging would create silent drift between scenario authoring and CI behavior. Mitigation: treat `scenarios.json` plus its schema as the only source of truth for filtering metadata.
- Live harness evolution from F3 may pressure the current `TestHarness` shape. Mitigation: keep the interface minimal and additive.

## 9. Open questions

- 2026-08-10: Should the schema be made normative through the PR-3905 scaffold docs in this branch, or remain documented in the ad hoc design notes until the follow-up doc stack lands?
- 2026-08-10: Do we want a dedicated regression consumer under `e2e_tests/regression/` now, or wait until the first non-smoke corpus exists?

## 10. AI-native notes

Entry points:

- `packages/tui/e2e_tests/scenario-runner/runner.ts`
- `packages/tui/e2e_tests/scenario-runner/types.ts`
- `packages/tui/e2e_tests/smoke/run-smoke.ts`
- `packages/tui/e2e_tests/smoke/scenarios.json`

Invariants:

- Scenario filtering must be metadata-driven, not hardcoded in workflows.
- Backends choose execution strategy; scenarios do not.
- Deterministic failures must fail closed.

Useful commands:

- `bun packages/tui/e2e_tests/smoke/run-smoke.ts --backend acp-mock --engine kas --no-capture-frames`
- `bun packages/tui/e2e_tests/smoke/run-smoke.ts --backend live --engine kas --scenario boot`
