# Cloud Sessions — Design

Status: Live on internal nightly (rollout 100%) · Owner: kiro-cli
Companion runbook: [docs/oncall/cloud_sessions_runbook.md](../oncall/cloud_sessions_runbook.md)

Cloud sessions run an agent session on a remote cloud sandbox instead of the local machine:
`kiro chat --cloud [--repo owner/name]`, in-session `/repo`, `/sessions`, `/disconnect`,
`/autonomous on|off`, cloud rows in `--list-sessions` / `--delete-session`, and resume of cloud
session ids. The session survives client disconnects — detach and reattach later from any machine.

## Architecture

```
CLI (Rust launcher + TUI)
  └─ spawns KAS (@kiro/agent, node) with env from launch.rs
       └─ ACP over stdio: the CLI is a pure ACP consumer
       KAS ⇄ BFF (Kiro Web Portal Service, rpcv2Cbor, bearer creds)
            ├─ remote session store  (list / load / new / delete)
            ├─ relay link            (drives the sandbox-side agent, streams turns back)
            └─ source-provider catalog (_kiro/sourceProviders/* — repos for /repo)
       BFF ⇄ sandbox fleet (server-side clone via GitHub-App grant; no local upload)
```

Key boundaries:

- **The CLI never talks to the BFF.** All remote traffic rides ACP to the local KAS child; KAS owns
  the transport, retries, and relay websocket. The CLI's own network surface is unchanged by this
  feature.
- **Capability negotiation, not version sniffing.** The TUI sends
  `executionTarget: cloud-sandbox` only if KAS advertised it on the `initialize` handshake
  (`executionTargets` / `sessionSources` / `sourceProviders` in `agentCapabilities._meta.kiro`).
  Absent capability → the session silently runs local (`fell_back_local` telemetry, no user error).
- **Repos bind at create.** `--repo` / the `/repo` picker resolve against the provider catalog;
  KAS validates server-side at `session/new` and reports dropped repos via `_meta.kiro.warnings`.
  Local files are never uploaded automatically; prompt attachments are explicit and size-bucketed
  in telemetry.
- **Auth:** KAS pulls access tokens from the CLI over the ACP callback `_kiro/auth/getAccessToken`.
  The refresh token never leaves the CLI's local store.

### Relay contract (what crosses the wire while detached)

Exactly three agent→client callbacks cross the relay: `session/request_permission`,
`_kiro/frontendToolCall`, and `openExternalUrl`. File reads/writes, command execution, and
workspace search run ON the sandbox and are never serviced by the client for a remote session.
`userInput`/MCP elicitation is not relayed at all.

Callback survival is owned at the ends of the relay, not by a timer: the sandbox re-delivers an
outstanding permission prompt to the next client that attaches, and a reconnecting client is
re-prompted — so a permission raised while nobody is attached is simply waiting when someone
attaches. If a client vanishes mid-request, the router unblocks the sandbox by forwarding a
cancellation. Frontend tool calls and open-URL requests run in the client and cannot park: a
client that does not host the requested tool replies `cancelled` (an error reply is treated as
`cancelled`), so a relayed turn never hangs. Callbacks are correlated by id — two requests raised
before either is answered route to the right one, and a cancelled turn's re-delivered permission
is dropped rather than re-prompted. The answerer is whichever client is currently attached, not
necessarily the one that started the turn.

## Gating: how the feature turns on (and off)

Two independent gates, both required:

1. **Client rollout gate** — `crates/chat-cli/rollout.json` → `remote_sandbox`
   (compile-time; currently `internal` / `nightly` / 100%). Gated-out users see `--cloud`
   rejected as an unknown argument, indistinguishable from a typo; cloud rows are hidden from
   listings. Pinned by release-profile tests (`cloud_sessions_gating.rs`).
2. **KAS endpoint gate** — KAS constructs its remote adapters (session source, relay link,
   provider catalog) only when `KIRO_REMOTE_SESSIONS_ENDPOINT` is supplied. Without it, KAS makes
   zero BFF calls and advertises `executionTargets: ['local']`.

The launcher connects the two (`launch.rs::resolve_remote_sessions_endpoint`): it sets the
endpoint env on the KAS child **only when the rollout gate is enabled** for this user. An explicit
`KIRO_REMOTE_SESSIONS_ENDPOINT` in the parent environment always wins (preprod testing). The
endpoint default derives from the auth portal stage (`app.kiro.dev` / gamma / beta).

### Dark-ship invariants (tested)

For any gated-out user, the feature is unobservable — these are pinned by release-profile
integration tests (`crates/chat-cli/tests/cloud_sessions_gating.rs`, run by the CI release step):

- `--cloud` / `--repo` are rejected as unknown arguments *before any side effects*, byte-identical
  to a typo, on every entry path including `--list-models` and session-flag early returns.
- Cloud rows never appear in `--list-sessions`; `--delete-session` cannot address them.
- Cloud-only slash commands (`/repo`, `/sessions`, `/disconnect`, `/autonomous`) stay hidden.
- The TUI never sends a cloud `executionTarget` unless KAS advertised it — so even a
  wrongly gated client cannot conjure a cloud session against a KAS that has no endpoint.

### Kill switch

Killing the feature is a client-side flip — no backend change needed for rollout-gated cohorts:

- **Ramp control / kill (release-cycle latency):** one reviewed CR setting
  `remote_sandbox.treatment_percent: 0` → launcher stops setting the endpoint → KAS advertises
  local-only → CLI degrades silently, `fell_back_local` fires. The `rollout.json` git history is
  the audit trail; rollout tests pin the expected state.
- **Immediate kill for already-shipped binaries:** binaries in the field already hold the gate
  state, so the fast path is server-side — the BFF refusing cloud RPCs. Cloud calls then fail with
  classified errors and user guidance (see telemetry below). This lever is backend-owned.
- `KIRO_TEST_MODE=1` force-enables the client rollout gate on any build (documented internal
  backdoor); the endpoint/capability gate still applies.

## Failure handling

- Backend errors arrive as flattened JSON-RPC text; `packages/tui/src/utils/cloud-error-classify.ts`
  maps them to a bounded `cloud_error_kind` (throttling / auth / version_skew / not_found /
  network / timeout / stream_truncated / server_error / other) and to user guidance (e.g.
  version skew → "update kiro or retry later"; auth → re-login). `version_skew`
  (`UnknownOperationException`) is the KAS↔BFF deploy-mismatch signature.
- A cloud bring-up failure renders a terminal failed row (no spinner hang); an unadvertised
  capability degrades to local rather than erroring; `sourceProviders/list` retries once with a
  longer pause when the failure classifies as throttling.
- **Session routing on load:** `session/load` acts on one store. An explicit source wins; a
  cloud placement starts remote; otherwise the local store is tried first with a remote retry
  only on not-found — so a purely local failure is never misattributed to the cloud store, and a
  cloud id typed on a fresh machine still resolves.
- **Two session stores exist:** the local on-disk store and the remote store behind the BFF.
  Listings merge both when KAS advertises the remote source (`sessionSource: all` +
  `listScope: both` for the user slice); deletes route to exactly one store
  (`session_source: remote` for cloud rows) — `all` is rejected at the boundary by design.
- Disconnect (Ctrl+D, `/disconnect`, network loss) leaves the session running server-side;
  `/quit` offers keep-running vs turn-off.

## Telemetry

Namespace `KiroCLI`, OTel scope `kiro.tui`, emitted per the schema catalog
(`crates/kiro-telemetry-schema/schema/metrics.yaml`). All cloud emission points sit behind the
capability gates — external/stable builds read zero.

| Metric | Labels | Answers |
|---|---|---|
| `kiro_cli_cloud_session_lifecycle_total` | `cloud_event`: created, create_failed, ready, provision_failed, reattached, detached, turned_off, fell_back_local | volume, start/provision success, resume rate, kill-switch signature |
| `kiro_cli_cloud_session_ready_seconds` | histogram | sandbox provisioning latency |
| `kiro_cli_cloud_error_total` | `cloud_op` × `cloud_error_kind` | why cloud RPCs fail |
| `kiro_cli_cloud_repo_attach_total` | `repo_attach_event` × `repo_count_bucket` | /repo funnel |
| `kiro_cli_cloud_attach_total` | `attach_kind` × `attach_size_bucket` | prompt-attachment adoption; payload-cap early warning |
| `kiro_cli_autonomous_mode_total` | `autonomous_event` | /autonomous adoption + silent-failure modes |

Every datapoint is stamped with `user_id` (the CPS auth identity, cached from `GetUsageLimits`
after login and exported as `KIRO_USER_ID`) — identity rides as an EMF-queryable field, not a
CloudWatch dimension. Unique users are counted via Logs Insights `count_distinct` over the
`/kiro/metrics` log group; never group by or expose individual ids.

Pre-2.15.3 nightlies emitted a legacy shape (`kiro_cli_cloud_session_total`,
`started`/`start_failed`, `engine` dimension); dashboards read both generations during the
transition.

## Observability surfaces

- **Dashboard:** `KiroFeatureDashboard-CloudSandbox` (KiroTelemetryCDK, telemetry account) —
  session funnel, error breakdown, latency, feature usage, daily-unique-users.
- **Alarms:** StartFailureRate (>5%/15m, volume-guarded), VersionSkew, StreamTruncation,
  ThrottleSurge — Metrics Insights SELECT-based; paging is off until thresholds are tuned on ramp
  data.
- **Field diagnostics:** `kiro diagnostic` reports the rollout state, whether a remote-sessions
  endpoint is configured, and extracted KAS versions.

## Ownership

The ACP boundary is the ownership boundary:

- **CLI team** (CTI `Kiro / CLI / Intake`, resolver group `Amazon Q for CLI`): the launcher and
  gating chain, TUI rendering and UX, error classification and guidance, telemetry emission,
  the token-callback wire contract.
- **KAS / agent-server team** (CTI `AWS / Kiro / AgentServer`): the KAS process and capability
  advertisement, the relay link and turn streaming, the sandbox-side agent, and — via their
  internal routing — the BFF (Kiro Web Portal Service), remote session store, source-provider
  catalog, and sandbox fleet.
- **KUTS/telemetry team**: the metrics ingestion pipeline and dashboards infrastructure
  (KiroTelemetryCDK), distinct from the emission the CLI owns.

Operational procedures, triage tables, and per-error-kind escalation live in the
[cloud sessions runbook](../oncall/cloud_sessions_runbook.md).

## Rollout plan

| Stage | rollout.json | Exit criteria |
|---|---|---|
| 0 — internal nightly (today) | `internal / nightly / 100%` | 1 week clean: done (98.9% start success, ~2.4s ready latency) |
| 1 — internal stable | `internal / all / 100%` | 1–2 weeks: start success ≥99%, no skew/truncation spikes, alarm thresholds tuned, paging enabled |
| 2 — external ramp | `all / all / 25→50→100%` | ≥3 days/step; backend capacity sign-off before entry; alarms paging throughout |

Rollback at any stage: revert the rollout CR (next release) or backend endpoint refusal
(immediate). The dark-ship invariant holds at every stage — gated-out users cannot observe the
feature exists.
