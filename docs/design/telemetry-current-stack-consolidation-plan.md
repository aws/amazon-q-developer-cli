# Telemetry current stack consolidation and follow-up plan

**Status:** Draft execution plan

## Summary

We will create one giant PR for the telemetry work that already exists in the open PR stack. That PR is the integration and review vehicle for the current implemented commits, including rebasing onto `origin/main`, resolving conflicts, documenting the architectural direction, and proving locally that the implemented V2 and V3/KAS-observed telemetry still emits to the local Grafana stack.

The giant PR is intentionally different from the later architecture rollout. It should contain the commits that exist today, plus only the minimum code and docs needed to make those commits build, test, and validate together on current main. New work that completes the host extraction and local collector architecture should land after the giant PR in focused phases.

The newer host-extraction/local-collector design is still the target architecture. It should guide conflict resolution and follow-up planning, but it should not cause us to expand the giant PR into every future extraction, packaging, TUI-direct-OTLP, or cleanup task.

Since the original planning pass, we changed the collector rollout direction: the first collector implementation should be a managed sidecar started by the existing Kiro installation, not a per-platform login service. The collector is still a separate process and still receives telemetry from every running Kiro-owned process, but startup, reuse, config generation, and policy checks live in Kiro-controlled code.

## Inputs

Include the existing open telemetry PR stack, in the stack order from earliest to latest:

- `kiro-team/kiro-cli#2992`
- `kiro-team/kiro-cli#2993`
- `kiro-team/kiro-cli#2994`
- `kiro-team/kiro-cli#2995`
- `kiro-team/kiro-cli#2996`
- `kiro-team/kiro-cli#2997`
- `kiro-team/kiro-cli#2998`
- `kiro-team/kiro-cli#2971`
- `kiro-team/kiro-cli#2972`
- `kiro-team/kiro-cli#2973`
- `kiro-team/kiro-cli#2974`
- `kiro-team/kiro-cli#2975`
- `kiro-team/kiro-cli#2976`
- `kiro-team/kiro-cli#2977`
- `kiro-team/kiro-cli#2978`
- `kiro-team/kiro-cli#3008`
- `kiro-team/kiro-cli#3009`
- `kiro-team/kiro-cli#3010`

Before applying or rebasing, audit the stack by commit SHA and patch-id. Several PRs may contain duplicated logical commits because the same telemetry slices were opened in alternate chains. Duplicates should be recognized and documented rather than replayed twice.

## Non-negotiable decisions

1. The current existing stacked commits land through one giant PR.
2. The giant PR may repurpose the existing `#2992` branch/PR so reviewers have one current target.
3. Future work that has not already been implemented lands after the giant PR in phases.
4. V3/KAS telemetry in our pipeline means client-side observation from the TUI and chat-cli ACP host. KAS internal OpenTelemetry remains owned by the KAS team and is not emitted through our pipeline.
5. Conflict resolution should move toward the newer host-extraction/local-collector design, but only within the scope needed to integrate the current commits.

## Target architecture context

The older migration direction was direct OTLP/CloudWatch-style telemetry from the client-side SDK. The newer design changes the end state:

- Shared schema, constructors, pricing, redaction, consent, and OTel provider setup live in cross-harness telemetry crates.
- Host-owned runtime pieces move out of `chat-cli-v2` into `kiro-telemetry-host`.
- Agent-event and ACP-event translation moves into a reusable observer layer.
- All Kiro-owned emitters eventually send OTLP/HTTP to one local user-scoped collector on `127.0.0.1:14318`.
- The collector is bundled with Kiro and spawned/reused on demand by enabled Kiro processes; it is not installed as a launchd/systemd/Scheduled Task login service by default.
- The local collector handles batching, retry, offline persistence, cardinality filtering, and collector self-health.
- The collector exports upstream to the Kiro-managed telemetry endpoint.
- KAS keeps its own separate telemetry pipeline; our V3 metrics observe the user-facing and ACP-host side of the interaction.

This means the giant PR should not add brand-new client-side queueing, WAL, direct CloudWatch/meta-meter, or runtime cardinality-limiter work unless that code is already part of the current stack and required for the stack to compile or validate. When old and new approaches conflict, prefer the newer architecture and call out any transitional code that remains.

### What changed in the refactoring plan

The collector decision changes the follow-up refactor shape, not the giant PR contents:

- `kiro-telemetry-collector` becomes a real lifecycle crate, not only a config/package placeholder. It should own `ensure_running`, `run`/`status`/`stop`, lock/PID files, health probes, config fingerprint checks, stale-process cleanup, and safe startup of the bundled collector helper.
- Kiro processes evaluate the effective telemetry gate before collector startup. Disabled telemetry means no collector startup, no loopback export, and no TUI fallback subprocess. The collector process also re-checks the gate on launch so stale/manual starts cannot bypass enterprise or user opt-out.
- Installers bundle the collector binary and config templates with the existing Kiro installation, but they do not register launchd, systemd-user, login-item, Windows Service, or Scheduled Task units in the first rollout.
- The Rust launcher passes the effective telemetry state and loopback endpoint to the TUI. Standalone TUI development may ask the installed `kiro` binary to ensure the collector is running only when telemetry is enabled and the binary path is available.
- The TUI/KAS bridge should move away from `chat _ emit-telemetry` subprocesses after the collector lifecycle exists. The intended final path is direct OTLP/HTTP from the TUI to `127.0.0.1:14318`, with collector-down events dropped and counted locally.
- Client-side durability, retry, and cardinality enforcement should not be rebuilt in each client. Those move to the collector, which sees the merged stream from all Kiro-owned processes for the current user.

## Giant PR execution plan

### 1. Prepare an integration branch

- Create a clean integration worktree from current `origin/main`.
- Fetch the latest PR heads and `origin/main`.
- Record the exact head SHA for each PR in the stack.
- Build a commit map that shows which PR contributed each commit.
- Detect duplicate logical commits with patch-id or equivalent diff comparison.

Acceptance:

- The integration branch starts from current `origin/main`.
- The source PR list and commit SHAs are documented.
- Duplicate commits are identified before conflict resolution starts.

### 2. Apply the existing stack

- Apply the PRs in stack order.
- Prefer cherry-pick ranges or a controlled rebase over ad hoc file copying.
- When a PR range is a duplicate of an already-applied logical change, skip it and record why.
- Keep host/collector scaffolding from `#3008`, `#3009`, and `#3010` if present in the current stack.
- Do not implement future `ensure_running`, installer packaging, TUI direct OTLP, or full observer extraction while assembling the giant PR.

Acceptance:

- The branch contains all non-duplicate existing stack changes.
- Skipped commits are explainable as empty or duplicate.
- The giant PR has no unresolved conflict markers.

### 3. Resolve conflicts with explicit rules

Use these rules consistently:

- Preserve schema YAML/catalog updates and typed metric constructors.
- Preserve pricing, redaction, consent, and model-class behavior.
- Preserve V2 telemetry behavior that existing tests exercise.
- Preserve V3/KAS-observed telemetry that is emitted by the TUI or chat-cli ACP host.
- Preserve local Grafana/dev telemetry stack files if they are part of the current stack.
- Keep `kiro-telemetry-host` and `kiro-telemetry-collector` scaffolding, but do not make them responsible for future behavior they do not yet implement.
- Avoid resurrecting obsolete client-side WAL, direct CloudWatch/meta-meter, queueing, or runtime cardinality code during conflict resolution unless a current tested path still depends on it.
- If old direct sinks or transitional V2-owned shims remain, document them as cleanup targets rather than expanding the PR.

Acceptance:

- Conflict choices are consistent with the newer architecture.
- Transitional decisions are called out in the PR notes.
- The final diff is integration plus necessary compatibility fixes, not a hidden architecture expansion.

### 4. Add docs to the giant PR

The giant PR should include:

- This consolidation plan.
- The host extraction and local collector design doc, or an updated equivalent.
- A short PR-body summary explaining what is implemented now versus what is planned next.

The docs should make the review model clear:

- Current PR: integrates and validates the existing telemetry stack.
- Follow-up phases: finish host extraction, collector implementation, packaging, TUI direct OTLP, multi-process semantics, and legacy cleanup.

Acceptance:

- Reviewers can tell which changes are already implemented.
- Reviewers can tell which architecture steps are intentionally deferred.
- The docs no longer imply that the giant PR should be split before landing the current stack.

## Validation plan for the giant PR

### Rust validation

Run the normal repository validation:

```bash
cargo +nightly fmt --check
cargo check --locked --workspace
cargo clippy --locked --workspace --color always -- -D warnings
```

Run targeted telemetry tests:

```bash
cargo test --locked -p kiro-telemetry --features test-support
cargo test --locked -p kiro-telemetry-schema
cargo test --locked -p kiro-telemetry-host
cargo test --locked -p kiro-telemetry-collector
cargo test --locked -p chat_cli_v2
```

Run broader tests if dependency access and time permit:

```bash
cargo test --locked --workspace
```

If a listed crate is not present before the stack is applied, treat that as expected pre-integration state, not a failure.

### TUI validation

From `packages/tui`:

```bash
bun run typecheck
bun test
bun run build
bun run lint
```

If KAS dependencies are needed:

```bash
./scripts/codeartifact-login.sh
bun install
```

### Local Grafana telemetry validation

Use the local telemetry stack from the integrated branch, expected under `dev/telemetry`.

1. Start the local telemetry stack.
2. Run the stack smoke tests, including catalog verification if present.
3. Build the TUI bundle.
4. Run the V2 path with local telemetry enabled and the OTLP endpoint pointed at the local stack.
5. Run the KAS/V3 launch path with local telemetry enabled and the same local endpoint.
6. Confirm the expected implemented metrics are visible in Grafana/Prometheus.

Example launch shape, adjusted to the exact README in `dev/telemetry`:

```bash
KIRO_TELEMETRY_OTEL=1 \
KIRO_TELEMETRY_OTLP_ENDPOINT=http://localhost:4318 \
KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
cargo run -p chat_cli --bin chat_cli -- chat --tui
```

For KAS:

```bash
KIRO_TELEMETRY_OTEL=1 \
KIRO_TELEMETRY_OTLP_ENDPOINT=http://localhost:4318 \
KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
cargo run -p chat_cli -- chat --agent-engine=kas
```

The exact local endpoint may change once the local collector path is implemented. For the giant PR, use whatever `dev/telemetry` documents for the current implemented stack.

### What to confirm in Grafana

Confirm the metrics that the current stack actually implements, including:

- V2 session, turn, tool, and error metrics.
- V2/V3 process-health metrics emitted from the TUI/chat-cli host observation path.
- KAS mode/agent-engine transition metrics if included in the stack.
- KAS user-turn completion or slash-command metrics if included in the stack.
- Schema/catalog-derived metric names and dimensions.

Do not require KAS internal model-call latency, sandbox timing, planner iteration, or KAS-owned OpenTelemetry data to appear in this dashboard. That data belongs to the KAS pipeline.

## PR packaging

After validation:

- Push the integrated branch over the chosen giant PR branch, likely `#2992`.
- Update the PR title to identify it as the consolidated telemetry stack PR.
- Update the PR body with:
  - included PR list,
  - skipped duplicate commit notes,
  - conflict-resolution notes,
  - validation commands and results,
  - local Grafana proof,
  - known transitional code,
  - follow-up phase list.
- Link or include the host-extraction/local-collector design doc.

Acceptance:

- Reviewers see one coherent giant PR for the current stack.
- Reviewers are not asked to review unimplemented future architecture as if it already exists.
- Future phases are visible and sequenced.

## Follow-up phases after the giant PR

### Phase 1: Host extraction completion

Move cross-harness host code out of `chat-cli-v2`.

Work:

- Move `TelemetryThread`, `HostConfig`, host event types, and shared event params into `kiro-telemetry-host`.
- Move event translation into `kiro-telemetry-observer`.
- Leave `chat-cli-v2/src/telemetry` as a thin shim or re-export layer while V2 still exists.
- Preserve current V1/V2 behavior.
- Keep KAS code unchanged; observe KAS through TUI and ACP-host boundaries.

Acceptance:

- V2 builds through the host crate boundary.
- Event serialization and test behavior remain stable.
- New clients can depend on the host crate without depending on `chat-cli-v2`.

### Phase 2: Local collector implementation

Make `kiro-telemetry-collector` own the local collector lifecycle.

Work:

- Implement `ensure_running`.
- Add PID/file lock coordination, stale PID cleanup, and config fingerprint checks.
- Add health probe and startup timeout behavior.
- Add `run`, `status`, and `stop` entrypoints for the managed sidecar.
- Generate collector config for `127.0.0.1:14318`.
- Configure batching, retry, `file_storage`, and cardinality processors.
- Add crash-signature ingestion through collector filelog where applicable.
- Respect the effective telemetry gate before startup: `KIRO_DISABLE_TELEMETRY`, `Q_DISABLE_TELEMETRY`, persisted `TelemetryEnabled`, `KIRO_TELEMETRY_ENABLED=false`, and `KIRO_TELEMETRY_OTEL=0` / `off`.

Acceptance:

- Multiple Kiro processes reuse one user-scoped collector.
- Offline persistence is handled by collector `file_storage`.
- Port conflicts fail predictably with actionable logs.
- Collector self-health exposes send/drop/queue signals.
- Disabled telemetry does not start the collector or emit to loopback.

### Phase 3: Packaging and gating

Ship the collector safely.

Work:

- Bundle `otelcol-contrib` in macOS, Linux, and Windows installers.
- Re-sign/notarize helper binaries where required.
- Do not install launchd, systemd-user, login-item, Windows Service, or Scheduled Task units in the first implementation.
- Start/reuse the bundled collector on demand from the existing Kiro installation.
- Keep telemetry default-off until installer behavior is proven.
- Flip to OTel-only only after canary validation.

Acceptance:

- Fresh installs can start or reuse a user-scoped collector without root when telemetry is enabled.
- Upgrades replace the collector safely.
- Rollback can be done through config/default changes.

### Phase 4: TUI and V3 client-side observation

Move V3 user-experience telemetry to the local collector boundary.

Work:

- Replace TUI `chat _ emit-telemetry` subprocess calls with direct OTLP/HTTP to `127.0.0.1:14318`.
- Add collector-down fallback: drop, increment an in-process counter, and trace-log.
- Add or finish ACP-host observers for `session/prompt`, tool approvals/denials, cancellations, KAS lifecycle, surfaced errors, token data, and cost data available on the wire.
- Preserve the boundary that KAS internals remain outside our pipeline.

Acceptance:

- The collector receives V3 user-perceived and ACP-host observations.
- The collector does not receive KAS internal model/tool/sandbox metrics.
- Dashboard correlation with KAS-owned telemetry happens by shared IDs, not by merging pipelines.

### Phase 5: Multi-process correctness

Prevent overcounting across Kiro-owned emitters.

Work:

- Add `HostRole` values such as `UserCli`, `Tui`, and `BackgroundDaemon`.
- Gate per-user/day metrics to the intended role.
- Add file-lock dedup for daily heartbeat, client-version-seen, and feature-first-use metrics.
- Keep session, turn, and tool metrics counted per actual event.

Acceptance:

- Multiple foreground CLIs, TUI, and background daemons produce one per-user/day heartbeat where required.
- Session and turn counts remain accurate.
- TUI UX metrics still emit without owning per-user/day invariants.

### Phase 6: Legacy cleanup

Remove old paths after the collector path is proven.

Work:

- Delete or retire client-side WAL and custom queueing code if still present.
- Delete direct CloudWatch/meta-meter code if still present.
- Remove runtime cardinality budget code that moved to collector config.
- Retire legacy direct sinks only after dashboard parity and downstream consumers are signed off.

Acceptance:

- Product paths no longer use old direct CloudWatch or CodeWhisperer telemetry sinks.
- Critical alarms have parity or an approved migration path.
- Downstream consumers have migrated or explicitly accepted deprecation.

## Open decisions for later phases

- Exact purge vs quarantine mechanics for telemetry already persisted in collector `file_storage`; replaying pre-opt-out data after opt-out is not allowed.
- Exact `HostRole` semantics for TUI-originated user-action metrics.
- GovCloud and partition-specific collector endpoint behavior.
- Collector self-health alerting without reintroducing direct client CloudWatch writes.
- Whether an optional persistent login service is ever worth adding after on-demand spawn/reuse ships.
- Whether the local dev stack should model the final two-hop topology: Kiro process to local collector on `14318`, then collector to dev upstream/Grafana.

## Definition of done for the giant PR

- All existing non-duplicate PR stack commits are represented.
- The branch is rebased onto current `origin/main`.
- Conflict choices are documented.
- Rust and TUI validation results are recorded.
- V2 and V3/KAS-observed telemetry are validated against local Grafana.
- The plan and architecture docs are included or linked from the PR.
- Follow-up phases are clearly out of scope for the giant PR and ready to become separate work items.
