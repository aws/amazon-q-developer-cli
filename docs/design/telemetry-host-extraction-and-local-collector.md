# Telemetry host extraction and local OTel Collector

**Status:** Proposed (host-extraction Phase 1 PRs already open: #3008/#3009/#3010)
**Owners:** vinayshah1998
**Reviewers:** kensave (G1 review prompted this design)
**Successor to:** [`telemetry-otel-cloudwatch-migration.md`](./telemetry-otel-cloudwatch-migration.md)

## Why a new doc

The original migration doc (`telemetry-otel-cloudwatch-migration.md`) committed to publishing OTLP from the chat-cli SDK directly to CloudWatch via an ADOT collector we'd own. After landing the foundation crates (G1 #2988, merged), the V1 token-cost work (G2 #2989, merged), and starting on the V3 (KAS) migration plan, several constraints surfaced that change the right architecture:

1. **The host code lives in V2.** `TelemetryThread`, `TelemetryObserver`, the mpsc channel and background tokio task all live under `crates/chat-cli-v2/src/telemetry/`, but V2 is being deprecated in favor of V3 (KAS — separate agent process talked to over ACP). V3, kiro-bot, lite mode, and the TUI all need the same telemetry pipeline. The host is in the wrong crate.

2. **V3 (KAS) is opaque to our telemetry.** KAS runs its own OpenTelemetry stack to measure agent-side performance; that's owned by a different team and exports to a different endpoint. **We do not emit telemetry from inside KAS, and we do not consume KAS's OTel output.** Our V3 telemetry is **client-side observation of our interactions with the agent**, captured in the surface that talks to KAS — primarily the TUI, with the chat-cli ACP host filling in what only it can see (turn metering, tool-invocation outcomes, errors surfaced through ACP). The boundary is firm: KAS's internal model-call latency, retries, and tool-execution timings are KAS's concern; our latency is wall-clock from "user pressed enter" to "TUI rendered the response."

3. **The custom SDK was over-engineered for a CLI.** Per kensave's review on G1: `QueuedTelemetrySink` (~500 LOC) duplicates `PeriodicReader`, `TelemetryWal` (~240 LOC) tries to give a CLI server-grade durability, and `CardinalityLimiter` runtime-enforced budgets are typically a server-side concern. We're carrying ~1.5k LOC the OTel SDK + a local collector handle for free.

4. **N concurrent kiro-cli processes per user is the common case.** Without coordination, every process opens its own OTLP HTTPS connection, runs its own retry timer, has an independent cardinality budget, and emits per-user-per-day metrics N times. Centralizing through a single local OTel Collector solves all of those at once.

5. **A separate Kiro-managed endpoint exists.** `prod.us-east-1.telemetry-v2.kiro.dev` is the V2 endpoint; we point at it directly so we can validate end-to-end against live infrastructure rather than landing pre-wired code that can't run until separate infra is provisioned.

This doc is the architecture we're committing to, the rollout plan, and the open questions resolved with explicit answers.

---

## Target architecture

### Diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│ User machine                                                              │
│                                                                           │
│  ┌────────────────┐   ┌─────────────────────┐    ┌──────────────────┐    │
│  │ kiro-cli (V2)  │   │ TUI (TS)            │    │ kiro-bot         │    │
│  │ background     │   │ ── observes user-   │    │ background task  │    │
│  │ task           │   │    side interaction │    │                  │    │
│  │                │   │    with KAS via ACP │    │                  │    │
│  │                │   │                     │    │                  │    │
│  │ Plus: chat-cli │   │ Plus: chat-cli      │    │                  │    │
│  │ ACP host (V3)  │◄──┤ talks ACP→KAS,      │    │                  │    │
│  │ observes       │   │ derives client-side │    │                  │    │
│  │ ACP-level      │   │ metrics from the    │    │                  │    │
│  │ KAS interaction│   │ stream it sees      │    │                  │    │
│  └──────┬─────────┘   └──────────┬──────────┘    └────────┬─────────┘    │
│         │                        │                         │              │
│         └────────────┬───────────┴─────────────────────────┘              │
│                      │                                                    │
│         OTLP/HTTP @ 127.0.0.1:14318 (kiro-namespaced loopback port)       │
│                      │                                                    │
│           ┌──────────▼──────────────────────┐                             │
│           │  otelcol-contrib (bundled)     │                             │
│           │  ──────────────────────────    │                             │
│           │  receivers: otlp/http          │                             │
│           │  processors:                   │                             │
│           │    memory_limiter (128 MiB)    │                             │
│           │    transform (cardinality)     │                             │
│           │    batch (5s / 512 records)    │                             │
│           │  exporters:                    │                             │
│           │    otlphttp/upstream           │                             │
│           │    sending_queue:              │                             │
│           │      storage: file_storage/kiro│                             │
│           │  extensions:                   │                             │
│           │    file_storage/kiro           │                             │
│           │      ~/.kiro/telemetry/queue   │                             │
│           │    health_check :13133         │                             │
│           └──────────┬──────────────────────┘                             │
│                      │                                                    │
│                      │                                                    │
│  ┌───────────────────┴─────────────────────────────────────────┐          │
│  │ KAS subprocess (V3 agent harness)                            │          │
│  │ ── runs OWN OpenTelemetry SDK, OWN endpoint, OWN team        │          │
│  │ ── invisible to our pipeline; we do not consume its output   │          │
│  │ ── talks ACP to chat-cli host above; that's our observation  │          │
│  │    surface for V3 telemetry                                  │          │
│  └──────────────────────────────────────────────────────────────┘          │
└─────────────────┬────────────────────────────────────────────────────────┘
                  │
                  │ OTLP/HTTP (batched, retried, persisted)
                  ▼
           prod.us-east-1.telemetry-v2.kiro.dev
           (Kiro-managed, no client auth required —
            internal endpoint behind VPC + service-side authn)
```

### V3 telemetry: client-side observation only

V3 is materially different from V2 in how telemetry flows. This section is load-bearing — most of the rest of the design follows from it.

**KAS owns its own pipeline.** KAS (the V3 agent harness) is a separate Rust subprocess maintained by a different team. It already runs an OpenTelemetry stack to measure agent-side performance — model invocation latency, retry behavior internal to the agent, tool-execution timing inside the sandbox, planner iteration counts. That telemetry exports to a KAS-team-owned endpoint and is none of our concern.

**We do not emit from inside KAS.** We are not adding our schema, our redactor, or our pricing tables to the KAS process. We are not consuming KAS's OTel output to re-emit it under our schema. The two pipelines are independent.

**Our V3 telemetry is what the user-facing surface sees.** The chat-cli ACP host and the TUI both talk to KAS over ACP. Anything observable from those vantage points — and only from those vantage points — is our domain:

| What we measure for V3 | Where the observation happens |
|---|---|
| User-perceived turn latency (Enter pressed → first content rendered) | TUI |
| User-perceived turn duration (Enter pressed → turn complete) | TUI |
| Tool invocations the user approved or denied | TUI |
| Cancellations issued by the user | TUI |
| Slash commands the user ran | TUI |
| ACP method-level success/failure (`session/prompt`, `tools/list`, etc.) | chat-cli ACP host |
| Token economics, model class, cost (parsed from ACP responses) | chat-cli ACP host |
| KAS process lifecycle from our side (spawn, exit, crash signature) | chat-cli ACP host |
| Errors KAS surfaces through ACP (with PII-redacted reasons) | chat-cli ACP host |

| What we do **not** measure | Why |
|---|---|
| Model API latency from the agent's side | KAS owns; visible in their pipeline |
| Internal tool-call retry behavior in KAS | KAS owns |
| Sandbox provisioning time | KAS owns |
| Planner iteration counts inside the agent | KAS owns |
| Anything that requires changing KAS code to emit | Different team, different release cadence |

**Two telemetry streams meet on the dashboard, not in the pipeline.** When we need to correlate a slow user-perceived turn with what KAS was doing, that correlation happens in Grafana / dashboards by joining on `session_id` and `request_id` attributes that both pipelines record. The pipelines themselves remain disjoint.

**Existing V2-shaped infrastructure carries forward.** The TUI's `_kiro.dev/telemetry/*` ext-notification handlers (which today forward KAS process-health, mode-changed, and turn-completion events into the V2 telemetry pipeline via the chat-cli host) are exactly the right shape: KAS surfaces *interaction* events, the TUI/chat-cli host enriches and emits them under our schema. That pattern keeps working in V3; we just stop calling it "V2-only" and start calling it the V3 client-side observation channel. The host extraction (PRs C-E) makes it work without depending on chat-cli-v2 specifically.

### Crate layout

The current monolithic V2 telemetry surface gets split across **five** crates with explicit responsibilities:

| Crate | Responsibility | Cross-harness? | Status |
|---|---|---|---|
| `kiro-telemetry-schema` | YAML metric/event catalog + registry + `validate_metric_record()` | Yes | exists, slim |
| `kiro-telemetry` | Typed metric/log constructors, `pricing.rs`, PII redaction, `consent.rs`, OTel provider init | Yes | exists, slim post-G1 |
| `kiro-telemetry-host` | `TelemetryThread`, `Event`/`EventType`, `HostConfig`, `*Params` types, `ReasonCode` | Yes | scaffold #3008, types #3010 |
| `kiro-telemetry-observer` | `TelemetryObserver` (agent-event → telemetry-event translation) | Yes | new, PR E |
| `kiro-telemetry-collector` | Local-collector lifecycle: `ensure_running`, `run`/`status`/`stop`, health probe, PID/lock/config coordination, default endpoint | Yes | scaffold #3009 |

What does **not** become a crate:

- ~~`kiro-telemetry-cloudwatch`~~ — **deleted** in G1 slim. Telemetry-on-telemetry is handled server-side via collector self-health metrics; client-side `PutMetricData` from Cognito-unauth pool was wrong on multiple axes (untrusted-client write scope, every CLI hits CW control plane, no rate-limit between client and CW).
- ~~Direct CloudWatch sink in V1/V2~~ — **removed** in G1 slim.
- ~~`TelemetryWal` (`crates/kiro-telemetry/src/wal.rs`)~~ — **deleted**. The collector's `file_storage` extension is the WAL.

### Dependency direction

```
                                     ┌──────────────────────┐
                                     │ kiro-telemetry-schema│
                                     └──────────┬───────────┘
                                                │
                       ┌────────────────────────┴───────────────────────┐
                       │                                                │
            ┌──────────▼──────────┐                  ┌──────────────────▼─────────────┐
            │  kiro-telemetry     │                  │ kiro-telemetry-host            │
            │  (constructors,     │                  │ (TelemetryThread, Event,       │
            │   pricing, OTel,    │◄─────────────────┤  HostConfig, *Params, Reason)  │
            │   redaction)        │                  └──┬─────────────────────────────┘
            └─────────────────────┘                     │
                                                        │
                                            ┌───────────▼─────────────┐
                                            │ kiro-telemetry-observer │
                                            │ (AgentEvent → Event)    │
                                            └───────────┬─────────────┘
                                                        │
                          ┌─────────────────────────────┼─────────────────────────┐
                          │                             │                         │
              ┌───────────▼──────┐         ┌────────────▼──────┐    ┌─────────────▼──────┐
              │  chat-cli-v2     │         │  chat-cli-v3      │    │  kiro-bot          │
              │  (existing host  │         │  (new host crate) │    │  (lite host)       │
              │   moves to lib   │         │                   │    │                    │
              │   re-exports)    │         │                   │    │                    │
              └──────────────────┘         └───────────────────┘    └────────────────────┘

           ┌─────────────────────────────┐
           │ kiro-telemetry-collector    │     used by harness binaries (chat-cli-v2,
           │ (process lifecycle, spawn)  │ ◄── chat-cli-v3, kiro-bot) at startup to
           └─────────────────────────────┘     ensure_running() the local collector.
```

### Deployment model

**One local OTel Collector per user, not per process.**

- All of *our* user-facing processes — chat-cli (V2 today, V3 ACP host tomorrow), the TUI, and the kiro-bot daemon — emit OTLP/HTTP to `127.0.0.1:14318` (kiro-namespaced port, avoiding default-4318 conflicts with Datadog/Honeycomb agents).
- KAS runs in a separate process and **does not** emit to this collector. It uses its own KAS-team-owned OTel pipeline (different endpoint, different team's responsibility).
- The collector is `otelcol-contrib` v0.128+ bundled with the kiro-cli installer (see "Why otelcol-contrib over otelcol-otlp" below).
- The collector is **spawned and reused on demand** by the existing Kiro installation. The first enabled Kiro process calls `kiro_telemetry_collector::ensure_running()`; later processes probe and reuse the same collector.
- The first implementation does **not** register launchd, systemd-user, login-item, Windows Service, or Scheduled Task units. Persistent login startup remains a future opt-in if cold start or collector availability becomes a proven problem.
- Per-user state under `~/.kiro/telemetry/collector/` stores a lock, PID record, health/config metadata, and collector queue. The PID record should include PID plus a process start identity/time where the platform supports it, so stale PID reuse does not attach to an unrelated process.
- A file lock ensures simultaneous kiro-cli invocations don't race-start multiple collectors.
- The collector runs as a user-scoped process (no root / system-wide install).

### Managed sidecar lifecycle

The collector is a separate process, but it is not a separately installed service. It is a managed sidecar of the Kiro installation.

Startup flow:

1. The Kiro launcher computes the effective telemetry gate from enterprise env vars, user settings, and launch-mode overrides.
2. If telemetry is disabled, the process installs a no-op telemetry provider, does not call `ensure_running()`, and does not emit to loopback.
3. If telemetry is enabled, Kiro resolves the bundled collector helper from the current install and calls `kiro_telemetry_collector::ensure_running()`.
4. `ensure_running()` takes the per-user collector lock, reads the PID/config state under `~/.kiro/telemetry/collector/`, probes the health endpoint, and compares the config fingerprint.
5. If a healthy collector with a matching fingerprint is already running, Kiro reuses it.
6. If state is stale or the config changed, Kiro cleans stale state and starts a new user-scoped collector process with generated config for `127.0.0.1:14318`.
7. The Rust launcher passes the effective telemetry state and loopback OTLP endpoint to the TUI so the TUI can emit directly once PR H lands.

The collector process can be launched either as the bundled `otelcol-contrib` binary with generated config, or through a thin hidden Kiro wrapper such as `kiro telemetry collector run` that performs policy checks and then supervises or execs the collector. The wrapper option is preferred if it keeps path resolution, logging, stop/status commands, and enterprise policy enforcement simpler across platforms. Either way, the installation model is the same: ship the helper with Kiro, start it from Kiro, and coordinate reuse through per-user state.

The sidecar is intentionally detached from the foreground chat process so it can serve multiple concurrent Kiro processes and drain its queue after one session exits. It should still have explicit lifecycle controls: `status` for diagnostics, `stop` for user/admin cleanup, stale-PID cleanup on the next `ensure_running()`, and an optional idle shutdown after the queue drains if cold-start measurements say that is safe.

### Telemetry policy propagation

The policy gate must be enforced at every boundary:

- Parent Kiro processes do not start the collector or initialize exporting providers when disabled.
- The collector wrapper or `run` command re-checks the effective gate before listening on loopback.
- The TUI receives the effective `KIRO_TELEMETRY_ENABLED` value from the Rust launcher and treats disabled telemetry as a hard no-op.
- A durable opt-out through settings or enterprise policy must stop future collector startup and purge or quarantine queued collector data before telemetry can be re-enabled. Replaying data that was queued before opt-out is not allowed.
- An ephemeral per-process env override such as `KIRO_DISABLE_TELEMETRY=1` disables that process immediately. If another already-enabled Kiro process is legitimately using the sidecar, the disabled process simply does not emit.

This keeps enterprise/user opt-out authoritative while still allowing one enabled sidecar to accept telemetry from every enabled Kiro process owned by the same user.

### Managed-sidecar precedent: Codex

We compared this with `openai/codex`'s `app-server-daemon`. Codex does not install its durable helper as a per-platform login service by default; it starts/reuses the helper explicitly, stores PID/lock/socket state under `CODEX_HOME`, probes health before spawning, cleans stale state, and shuts down gracefully before force-killing. Kiro should follow that managed-sidecar pattern: bundle the helper, coordinate process lifetime in the CLI, and avoid OS service registration until a concrete need appears.

### Refactoring impact of the sidecar decision

The new approach does not change the giant PR goal: consolidate and validate the currently-open telemetry stack. It changes what the next refactoring phases optimize for:

- Host extraction still moves reusable runtime and observer code out of `chat-cli-v2`, but the exported host boundary should assume "emit to local collector" instead of "each client owns its own sink durability."
- `kiro-telemetry-collector` becomes the boundary for multi-process behavior: one loopback endpoint, one upstream exporter, one retry queue, one file-storage WAL, and one cardinality processor for the merged per-user stream.
- TUI/V3 telemetry should stop paying the cost of spawning `chat _ emit-telemetry` for each event once the sidecar exists. The replacement is direct fire-and-forget OTLP/HTTP to loopback after the Rust launcher has ensured the collector is running.
- Packaging work is reduced to bundling/signing/notarizing the helper and exposing Kiro-managed lifecycle commands. OS login services are not part of the first implementation.
- Tests need to cover sidecar coordination: simultaneous startup, stale PID handling, config-fingerprint mismatch, disabled telemetry gates, opt-out queue purge/quarantine, collector-down TUI behavior, and multiple Kiro processes sending into the same collector.

---

## Resolved decisions

### Endpoint: `prod.us-east-1.telemetry-v2.kiro.dev`

- The default upstream endpoint for the collector exporter.
- In the current pre-collector stack, `KIRO_TELEMETRY_OTLP_ENDPOINT` is still the SDK endpoint override. In the final local-collector mode, client processes target loopback by default and the collector targets this upstream endpoint.
- The collector's `otlphttp/upstream` exporter targets this endpoint with `https://` and no client-side auth header. Auth is service-side (VPC + signed identity at the endpoint).
- No bundled credentials in the installer — eliminates the "extract token from .pkg" risk.

### Loopback port: 14318 (not 4318)

- The OTel default is 4318. Datadog Agent, Honeycomb agent, dev environments all bind 4318.
- 14318 is kiro-namespaced and unlikely to collide.
- Set as `DEFAULT_LOOPBACK_OTLP_ENDPOINT` in `crates/kiro-telemetry-collector/src/lib.rs` (already shipped in #3009).
- Endpoint overrides are part of the collector config fingerprint. If a process asks for a different upstream endpoint than the running collector was started with, `ensure_running()` must not silently reuse the old collector; it should restart safely, use a dev-specific port, or fail with an actionable message.

### Telemetry opt-out and enterprise overrides

- The client-side gate is authoritative. If telemetry is disabled, Kiro processes do not initialize exporting providers, do not emit to loopback, and do not call `ensure_running()`.
- The gate includes `KIRO_DISABLE_TELEMETRY`, `Q_DISABLE_TELEMETRY`, the persisted `TelemetryEnabled` setting, `KIRO_TELEMETRY_ENABLED=false` as passed to the TUI, and `KIRO_TELEMETRY_OTEL=0` / `off`.
- The collector command also checks the effective gate on startup so stale manual launches cannot bypass policy.
- On opt-out, queued collector data must be purged or quarantined before any future export can resume. The follow-up implementation must choose the exact purge/quarantine mechanics, but replaying pre-opt-out data after opt-out is not allowed.
- The Rust launcher should pass the effective `KIRO_TELEMETRY_ENABLED` value and collector endpoint into the TUI. Standalone TUI development can call `kiro telemetry collector ensure-running` only when `KIRO_CHAT_CLI_BIN` is available and telemetry is enabled.

### `otelcol-contrib` over `otelcol-otlp`

- `otelcol-otlp` (slim, ~10 MB) does **not** ship the `file_storage` extension.
- `file_storage` is what gives us offline-laptop persistence (the WAL replacement).
- `otelcol-contrib` (~45 MB) includes `file_storage` and the `transform`/`filter` processors we need for cardinality enforcement.
- 35 MB extra binary size is acceptable for the operational gain.

### Crash signature handling: file-drop + `filelog` receiver

- Today: panic-hook in the SDK writes a crash signature to disk; replayed on next start.
- After: SDK best-effort drops `~/.kiro/telemetry/crashes/<pid>.json` on panic (synchronous filesystem write, no network).
- The collector's `filelog` receiver ingests that directory and forwards crash sigs as log records to the upstream endpoint.
- Why not synchronous OTLP send on panic: 100ms-timeout sync send during a panic is racy and adds blocking I/O to a process that's already in a bad state.

### TUI emit on collector down: counter-only fallback

- After PR H, TUI emits OTLP/HTTP via `fetch` directly to `127.0.0.1:14318`.
- If the collector is down: drop the event, increment an in-process counter (`tui.telemetry.emit.failed_total`), log at `trace` level.
- No IndexedDB queue, no retry, no subprocess fallback.
- Acceptable because: TUI-originated metrics are user-experience concerns (render latency, startup time) — losing some on collector-down is fine.

### macOS Gatekeeper: re-notarize under Amazon Developer ID

- `otelcol-contrib` is signed by OTel maintainers, not Amazon.
- Apple Gatekeeper will quarantine an OTel-signed helper inside an Amazon-signed `.pkg`.
- We re-notarize under Amazon's Developer ID as part of the installer build (PR G).
- Adds release-engineering cost even without a LaunchAgent; flagged as PR G's blocking dependency.

### `KIRO_TELEMETRY_OTEL` default: `0` (off) through PR G

- Stays default-off until the bundled collector + on-demand spawn/reuse path are verified on real installs.
- Flipping to `2` (otel-only) happens in a follow-up PR after a canary build.
- This gives us a rollback path: if upstream telemetry volume spikes or the collector misbehaves on user machines, we revert one env-default flip.

### V1 telemetry: kept as-is

- V1's OTel additions in G1 are kept (decision made before kensave's review; user confirmed).
- They cost no incremental review surface (already merged in G1) and provide a useful side-by-side parity sanity check while we migrate V2 → V3.
- V1 will be removed entirely as the V3 migration completes; until then, V1 (chat-cli legacy) and the V3 ACP host (in chat-cli) both emit through the same local collector. KAS itself does not.

---

## Multi-process semantics

This is the part the original migration doc didn't address. The N concurrent processes that share our pipeline are **our** processes — multiple kiro-cli foreground sessions, the TUI, and the kiro-bot daemon. KAS and the agent crates it spawns are explicitly **not** part of this set (see "V3 telemetry: client-side observation only" above). With N concurrent emitters per user as the common case, a few problems emerge:

### Deduplication: per-user-per-day events

Some catalog metrics are **session-lifecycle** (`cli_session_started_total`, `chat_session_started_total`) and N processes correctly produce N counts.

Others are **per-user-per-day** invariants:
- `kiro_cli_daily_heartbeat`
- `client_version_seen` (once per day per version)
- `feature_first_use` (once per user per feature, ever)

Without dedup, the user with 5 kiro-cli windows open emits 5× the heartbeats, breaking DAU/MAU calculations.

**Solution (PR L, ~30 LOC):** SDK-side file lock at `~/.kiro/telemetry/heartbeat-{YYYY-MM-DD}.lock`. The first process on a given day acquires the lock and emits; subsequent processes try-lock and skip on contention. Renewed daily.

Why file lock and not collector-side dedup:
- File lock is simpler (no collector state, no `groupbyattrs` processor cost)
- Atomic across processes via `flock(2)` / `LockFile()` semantics
- The collector still has the option to add backstop dedup later if we find drift

### Cardinality budget × N processes

Today's `CardinalityLimiter` (deleted in G1 slim) was per-process. Without coordination, total cardinality delivered to upstream was N× the per-process budget.

**Solution (PR F-impl, in collector config):**

```yaml
processors:
  transform/cardinality:
    metric_statements:
      - context: datapoint
        statements:
          - keep_keys(attributes, [<allowlist of dimensions>])
  filter/known_high_cardinality:
    metrics:
      datapoint:
        - 'attributes["error_code"] != nil and not IsMatch(attributes["error_code"], "^[a-z_]+$")'
```

- The collector enforces the actual cardinality budget against the merged stream from N processes.
- Per-process, a thin compile-time check via `kiro-telemetry-schema::validate_metric_record()` catches obvious bugs (closed-enum violations, undeclared attributes) — kept as a sanity guard, not a runtime budget.
- Schema YAML remains the source of truth: every metric declares its allowed dimensions; the collector's `transform` processor enforces that allowlist.

### `host_role` discriminator on per-user metrics

Multiple of *our* processes can emit telemetry concurrently (kiro-cli foreground, kiro-bot daemon, the TUI process). They shouldn't all independently emit user-facing metrics like `daily_heartbeat` and `feature_first_use` — only the user-facing CLI should.

(Note: KAS and KAS-spawned sub-agents are **not** in our pipeline at all per the V3 boundary above. This discriminator is about distinguishing among *our* surfaces — chat-cli host vs kiro-bot vs TUI.)

**Solution (PR K):**

- Add a `HostRole` enum: `UserCli | Tui | BackgroundDaemon`.
- Wire through `HostConfig::host_role`.
- Per-user metric constructors gate on `host_role == UserCli` at the constructor layer; calls from `Tui` or `BackgroundDaemon` become no-ops for those specific metrics.
- The chat-cli foreground process identifies as `UserCli`; the TUI as `Tui`; kiro-bot as `BackgroundDaemon`.
- The existing `is_subagent: bool` field stays, but for a different purpose — it tags metrics emitted on behalf of an agent-spawned subagent within the V2 host (V2-only concern; V3 doesn't have this since sub-agents are KAS-side).

### Single OTLP connection, single retry timer, single WAL

These come for free with the collector pattern:
- All processes emit to localhost (sub-millisecond, no TLS handshake amortized over time)
- Single upstream HTTPS connection from collector → endpoint
- One retry/backoff curve, not N independent ones (avoids retry-storm on upstream degradation)
- Single `file_storage` directory, replayed on collector restart, not per-process WAL replay

---

## Migration plan (12 PRs, on top of #2988-#2998)

PRs B and F-scaffold are already open as #3010 and #3009. The plan replaces my earlier 11-PR plan with two additions: PR L (per-user dedup) and the host_role split.

### Critical path (sequential)

| PR | Title | Status | LOC est | Key risk |
|---|---|---|---|---|
| A | scaffold `kiro-telemetry-host` | **open #3008** | ~80 | — |
| F | scaffold `kiro-telemetry-collector` (port 14318) | **open #3009** | ~150 | — |
| B | move `Event`/`EventType`/`*Params` into host | **open #3010** | ~1,300 | orphan rule on `Event::otel_*` impls — solved via `EventLegacyExt` trait in V2 |
| C | extract `kiro-telemetry-legacy` (folds in `Event::into_metric_datum` etc.; kills `EventLegacyExt`) | next | ~1,500 | feature-gate behind `legacy` for kiro-bot |
| D | extract `TelemetryThread` + `HostConfig` | next | ~600 | `(env, fs, database, region)` → `HostConfig { ... }` is a one-call-site change in each of V1+V2 |
| E | extract `kiro-telemetry-observer` | next | ~1,100 | the `database`/`agent::rts` couplings — replace with `EventMetadataProvider` trait + closure for `model_provider` |

### Parallelizable (after E lands)

| PR | Title | LOC | Independent of |
|---|---|---|---|
| F-impl | flesh out `kiro-telemetry-collector`: `ensure_running()`, `run`/`status`/`stop`, lock/PID/config fingerprinting, `health::probe()` + cardinality processors in YAML template | ~650 | C/D/E |
| G | bundle `otelcol-contrib` into kiro-cli installers without registering login services; Apple notarization under Amazon Developer ID | ~200 (release tooling) | C/D/E |
| L | per-user-per-day dedup file lock + `HostRole` enum | ~80 | C/D/E |
| H | TUI: replace `chat _ emit-telemetry` subprocess with direct OTLP/HTTP fetch to `127.0.0.1:14318` | ~300 (TS) | C/D/E |
| I | V1 (`chat-cli`) adopts the new host crates | ~800 | C/D/E |
| J | `kiro-bot` lite mode: thin `HostConfig`, no observer, only `session-started` + `daily_heartbeat` | ~150 | C/D/E |
| K | V3 client-side observer: `chat-cli/src/acp/v3_observer.rs` translates ACP events (`session/prompt`, `tools/list`, error replies, `_kiro.dev/telemetry/*` ext-notifications) into `Event`s via the host crate; `host_role: HostRole` propagation | ~250 | C/D/E |

### Stage gates

1. **After A/B/F-scaffold:** types live in their final home; V2 imports unchanged via `pub use`. Already done.
2. **After C/D/E:** V2 builds *only* by going through `kiro-telemetry-host`. The `chat-cli-v2/src/telemetry/` directory becomes a 30-line shim. Validates the boundary.
3. **After F-impl/G/L:** local collector ships in the installer; enabled Kiro processes spawn/reuse it on demand; SDK emits to `127.0.0.1:14318`; per-user dedup is enforced. Still gated behind `KIRO_TELEMETRY_OTEL=2`.
4. **After H/I/J/K:** TUI + V1 + kiro-bot + V3 ACP host (in chat-cli) all emit through the same local collector. Default flips to `KIRO_TELEMETRY_OTEL=2`. KAS continues to operate its own independent pipeline.
5. **Cleanup:** after V3 dogfooding stabilizes, `chat-cli` (V1) gets removed; `chat-cli-v2` (V2) gets removed; the host crates remain.

### Unblocking work that doesn't fit the critical path

- **Drop unused `aws-sdk-cloudwatch` workspace dep** — flagged by review-bot on G1; orphaned after the meta-meter delete. Small follow-up PR (~5 LOC + Cargo.lock churn). Lands independently.
- **`KIRO_TELEMETRY_OTLP_ENDPOINT` env var override for staging** — kensave's non-blocking suggestion. ~10 LOC. Lands independently.

---

## What V3 looks like with this in place

V3 telemetry is two parallel observers — TUI-side and chat-cli-ACP-host-side — both feeding the same pipeline. **There is no `chat-cli-v3` crate that runs a `TelemetryThread`**; KAS owns the agent process, and our visibility into V3 is bounded by the ACP wire and the TUI.

### chat-cli ACP host (Rust): observes ACP-level interactions with KAS

```rust
// crates/chat-cli/src/acp/v3_observer.rs (sketch — actual location TBD during PR K)
use kiro_telemetry_host::{TelemetryThread, HostConfig, HostRole};
use kiro_telemetry_observer::{TelemetryObserver, TelemetryContext, AppType};
use kiro_telemetry_collector::ensure_running;

fn spawn_v3_acp_observer(
    acp_event_stream: AcpEventStream,
    state_dir: &Path,
    database: &Database,
) -> Result<TelemetryObserverHandle> {
    // 1. Ensure the local collector is up. Blocks ~50ms on first launch
    //    of the day; otherwise returns immediately via PID-file probe.
    ensure_running(state_dir)?;

    // 2. Build a HostConfig for V3 client-side observation.
    //    client_application=ChatCliV3 marks "this is the host that talks to KAS,"
    //    host_role=UserCli (we're the user-facing process).
    let telemetry = TelemetryThread::new(HostConfig {
        client_application: ClientApplication::ChatCliV3,
        host_role: HostRole::UserCli,
        client_id_provider: Arc::new(KasClientId::new(database)),
        metadata_provider: Arc::new(KasMetadataProvider { ... }),
        legacy_sink: None,
        ..HostConfig::default()
    })?;

    // 3. Spawn the observer over the ACP event stream — NOT over an internal
    //    AgentEvent stream the way V2 does. The observer translates ACP-level
    //    events (session/prompt responses, tool invocations seen on the wire,
    //    error replies, KAS lifecycle ext-notifications) into our schema's
    //    Event types.
    Ok(TelemetryObserver::spawn_for_acp(
        TelemetryContext { app_type: AppType::Acp, ... },
        telemetry,
        acp_event_stream,
    ))
}
```

### TUI (TS): observes user-side interaction with KAS

```typescript
// packages/tui/src/utils/v3-telemetry-observer.ts (sketch — replaces today's
// kas-telemetry-cli.ts subprocess shim with direct OTLP fetch)
import { emitMetricRecord } from './otlp-emit';

// Wired into the existing keypress / approval / cancel flow in app-store.ts
// and ACP client. Each helper below is called from the user-action site,
// emits one OTLP record to 127.0.0.1:14318, and returns immediately
// (fire-and-forget; collector handles retry/persist).

export function recordUserPerceivedTurn(args: {
  sessionId: string;
  promptToFirstContentMs: number;
  promptToCompleteMs: number;
  outcome: 'completed' | 'cancelled' | 'errored';
  toolUseCount: number;
}): void {
  emitMetricRecord({
    name: 'kiro_cli_user_turn_duration_seconds',
    kind: 'histogram',
    value: args.promptToCompleteMs / 1000,
    attributes: {
      client_application: 'chat_cli_v3',
      host_role: 'user_cli',
      outcome: args.outcome,
      // tool_use_count_bucket: derived
    },
  });
  // ...
}

export function recordUserApproval(args: {
  toolName: string;
  approved: boolean;
}): void {
  emitMetricRecord({
    name: 'kiro_cli_tool_approval_total',
    kind: 'counter',
    value: 1,
    attributes: {
      client_application: 'chat_cli_v3',
      tool_name: args.toolName,
      approved: String(args.approved),
    },
  });
}
```

### Observation boundary in code

The two streams describe the **same user interaction** but from different vantage points:

```
                 user presses Enter
                        │
                        ▼
                 ┌──────────────┐                  TUI sees:
        emit:    │ TUI         │ ── ACP request ─►  - prompt_length
   user_turn_    │              │                    - keystroke timing
   started      │              │ ◄── stream ──────  - first_content_ms
                 └──────┬───────┘                    - render_latency
                        │
                        │ ACP wire
                        ▼
              ┌──────────────────┐                chat-cli host sees:
   emit:      │ chat-cli ACP    │ ── ACP req ───►  - acp_method
   acp_call_  │ host (Rust)     │                  - acp_status
   total      │                 │ ◄── ACP resp ──   - tokens (from response)
              └────────┬────────┘                  - cost (computed)
                       │                            - kas_error_code
                       │ stdio
                       ▼
                ┌──────────────┐                  KAS sees (THEIR pipeline):
                │ KAS         │                    - model_invocation_latency
                │ subprocess  │                    - sandbox_setup_ms
                │             │                    - tool_execution_ms
                └──────────────┘                    - planner_iterations
                       │
                       └──► KAS-team OTel endpoint (not ours)
```

Same pattern works for kiro-bot (also talks to KAS over ACP — same observer shape applies) and lite mode (constructs only `session-started` / `daily_heartbeat`, no ACP observer needed). What does **not** work is "V3 host emits its own internal metrics" — that responsibility lives entirely in KAS's pipeline.

---

## Open questions / explicit non-goals

**Non-goals:**

- **No machine-scope collector.** User-scope only. Avoids root install, avoids cross-user data leakage.
- **No login-service collector by default.** The initial rollout does not install launchd, systemd-user, login-item, Windows Service, or Scheduled Task units. If we later need persistent startup, it should be an explicit follow-up with separate review.
- **No per-invocation collector.** `ensure_running()` coordinates one reusable sidecar per user. An idle shutdown after draining is acceptable; starting a fresh collector for every event or every process is not.
- **No client-side cardinality budget enforcement.** The collector's `transform` processor handles it. Per-process schema validation is a runaway-loop guard, not a budget.
- **No per-process WAL.** `file_storage` extension on the collector is the WAL.

**Open questions for follow-up:**

1. **Endpoint authentication.** Today: no client auth, server-side authn at the endpoint. Future: if we need per-user authentication (e.g., for tenant isolation), add a Bearer token sourced from the user's existing kiro-cli login (Cognito or OAuth flow), with rotation.
2. **Multi-user shared machines.** Build farms or shared dev machines. Today: each user gets their own user-scope collector. Future: an opt-in machine-scope collector for build farms via systemd system units.
3. **`ensure_running()` cost on each kiro-cli invocation.** A new kiro-cli process probing for the local collector on every invocation is fine in the steady state (PID-file probe is sub-millisecond). Worst case is a freshly-installed machine where the first telemetry-enabled invocation has to wait ~50-200ms for the collector to start. Acceptable. (KAS-spawned sub-agents do not call `ensure_running()` since they don't emit through our pipeline.)
4. **Collector upgrade cadence.** When otelcol-contrib has a CVE, we rebuild the kiro-cli installer with the patched binary. No in-place collector self-upgrade.
5. **Telemetry-on-telemetry.** Collector self-health metrics (`otelcol_exporter_sent_metric_points`, `otelcol_processor_dropped_metric_points`) get exported to the same upstream endpoint as a side-channel. Absence of heartbeat from a client is the primary "is the user emitting?" signal.

---

## Appendix: what changes vs. the original migration doc

| Original | Updated |
|---|---|
| ADOT collector at `otel.chat-cli.us-east-1.amazonaws.dev` (not yet provisioned) | `prod.us-east-1.telemetry-v2.kiro.dev` (Kiro-managed, live) |
| Custom `QueuedTelemetrySink` with bounded mpsc + worker thread | Deleted; OTel SDK `PeriodicReader` defaults |
| `TelemetryWal` for crash-safe replay | Deleted; collector `file_storage` extension is the WAL |
| `CardinalityLimiter` runtime budget | Deleted; collector `transform`/`filter` processors enforce cardinality |
| Per-event `force_flush()` on OTLP path | Removed; flush only on `TelemetryThread::finish()` |
| Direct `PutMetricData` from clients via `kiro-telemetry-cloudwatch` | Deleted; collector self-health metrics replace |
| Host (`TelemetryThread`, observer) lives in V2 | Extracted to `kiro-telemetry-host` + `kiro-telemetry-observer` |
| V3 path unspecified | V3 telemetry is *client-side observation* of KAS via TUI + chat-cli ACP host; KAS owns its own independent OTel stack and endpoint, opaque to us |
| Default loopback port 4318 | 14318 (avoids agent collisions) |
| Per-platform login service | Replaced with an on-demand managed sidecar spawned/reused by Kiro via `ensure_running()` |
| Telemetry default `KIRO_TELEMETRY_OTEL=1` (dual-write) | Stays at `0` (off) until installer ships otelcol-contrib + on-demand spawn/reuse verifies on real installs; then flips to `2` (otel-only) |

The schema YAML, PII redaction wiring, observer enrichments, pricing, and `consent.rs` carry forward unchanged.
