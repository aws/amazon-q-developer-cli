# Cloud Sessions Runbook

Oncall guide for cloud (remote-sandbox) sessions: `kiro chat --cloud`, `/repo`, resume of cloud
ids, `/autonomous`. Design and architecture: [docs/design/cloud-sessions.md](../design/cloud-sessions.md).

Feature state: dark-shipped; live for **internal nightly only** (rollout 100%). External and
stable builds cannot reach any cloud path, so customer-facing tickets about cloud sessions from
non-internal users indicate something is very wrong (check `KIRO_TEST_MODE` misuse first).

## Session lifecycle semantics (what the events mean)

- `created` — the cloud `session/new` was accepted; a sandbox is being provisioned.
- `ready` — the sandbox reached a live status; the session is usable. `created` without `ready`
  within ~a minute = provisioning stall (check ready-latency panel; fleet issue if widespread).
- `reattached` — a client re-joined a still-running session (resume). Reattach volume exceeding
  create volume is normal and healthy — sessions are long-lived by design.
- `create_failed` / `provision_failed` — the create was rejected / the sandbox reported a failed
  activity status. Count-only events; the *reason* lives in `kiro_cli_cloud_error_total`.
- `detached` — the user disconnected (Ctrl+D, `/disconnect`, network drop) and the session KEPT
  RUNNING server-side. Not a failure. Turns in flight continue on the sandbox.
- `turned_off` — the user chose shutdown at `/quit`. The terminal state.
- `fell_back_local` — cloud was requested, KAS didn't advertise the capability, the session ran
  locally with no error shown. See the kill-switch section: expected under gating, a signature
  under a kill, a bug only if the user is provably gated in.

Disconnect policy for callbacks while detached: an outstanding permission prompt is re-delivered
to the next client that attaches (no timeout — it waits); a client vanishing mid-answer cancels
the request so the sandbox is never blocked. Frontend tool calls and open-URL requests cannot
park — they resolve as cancelled when no client can serve them. `userInput`/elicitation is never
relayed — a remote turn will not surface it.

## Dashboards

- **Feature dashboard:** `KiroFeatureDashboard-CloudSandbox` — telemetry account `615299732016`,
  us-east-1. Session funnel (created/ready/reattached + failures), errors by kind and RPC surface,
  ready latency, /repo funnel, attachments, autonomous toggles, daily unique users.
- Metrics read-only access: ADA profile against account `615299732016`, `ReadOnly` role
  (see [metrics_and_telemetry.md](metrics_and_telemetry.md) for the workflow).
- Two metric generations exist (pre/post CLI 2.15.3): dashboard lines suffixed `(legacy)` are the
  old names — expected to flatline as the fleet upgrades, kept so history stays visible.

## Copy-paste queries

Read-only setup (once): ADA profile per [metrics_and_telemetry.md](metrics_and_telemetry.md), then
`export P=kuts_telemetry_prod_read-only`.

Session lifecycle over a window (both metric generations; Sum for delta counters):

```bash
aws cloudwatch get-metric-statistics --profile $P --region us-east-1 \
  --namespace KiroCLI --metric-name kiro_cli_cloud_session_lifecycle_total \
  --start-time 2026-07-24T00:00:00Z --end-time 2026-07-31T00:00:00Z \
  --period 86400 --statistics Sum \
  --dimensions Name=cloud_event,Value=created Name=OTelLib,Value=kiro.tui
# repeat per cloud_event; pre-2.15.3 history lives under kiro_cli_cloud_session_total
# with events started/start_failed and an extra engine=v3 dimension
```

Which series are live right now (dimension sets change across client generations — always list
before graphing):

```bash
aws cloudwatch list-metrics --profile $P --region us-east-1 \
  --namespace KiroCLI --metric-name kiro_cli_cloud_session_lifecycle_total \
  --recently-active PT3H
```

Errors by kind + failing RPC (Logs Insights on `/kiro/metrics`; keep windows narrow — the group
is high volume):

```
filter ispresent(kiro_cli_cloud_error_total)
| stats sum(kiro_cli_cloud_error_total) as errors by cloud_error_kind, cloud_op
| sort errors desc
```

Daily unique cloud users (identity field is snake_case `user_id` on CLI records; camelCase
`userId` belongs to other emitters on the same group — coalesce covers both):

```
filter ispresent(kiro_cli_cloud_session_lifecycle_total) or ispresent(kiro_cli_cloud_session_total)
| fields coalesce(user_id, userId) as uid
| stats count_distinct(uid) as unique_users by bin(1d)
```

Aggregate only — never group by or output raw `user_id` values.

## Alarms

All four are in the telemetry account, built from Metrics Insights SELECT (they sum across
versions and both metric generations). Paging is currently **disabled** (`enableTicketing=false`
stack-wide) until thresholds are tuned on ramp data — treat dashboard-visible alarm state as the
signal meanwhile.

| Alarm | Threshold | Meaning / first move |
|---|---|---|
| `KiroCLI-CloudSandbox-StartFailureRate` | >5% over 15m (min 20 attempts) | Session creates failing. Check errors-by-kind panel to classify, then the matching row below |
| `KiroCLI-CloudSandbox-VersionSkew` | >10 in 5m | Deployed KAS and BFF disagree on the API (`UnknownOperationException`). Engage the KAS/BFF oncall — this is a backend deploy issue, not a client bug |
| `KiroCLI-CloudSandbox-StreamTruncation` | >10 in 15m | Relayed turns ending without a done frame — relay/websocket health. Engage the KAS oncall |
| `KiroCLI-CloudSandbox-ThrottleSurge` | >100 in 15m | BFF rate-limiting cloud RPCs. Quota/capacity conversation with the backend team |

Missing data = OK by design (dark-shipped feature; zero traffic is normal).

## Triage: "cloud session won't start / behaves wrong"

1. **Which build?** Only internal nightly has the feature. `kiro diagnostic` on the reporter's
   machine shows the rollout state, whether a remote-sessions endpoint is configured, and the
   extracted KAS versions — ask for its output first.
2. **Classify the failure.** The CLI already labels cloud errors
   (`kiro_cli_cloud_error_total{cloud_error_kind}`) and shows guidance:
   - `version_skew` → backend deploy mismatch; KAS/BFF oncall.
   - `throttling` → BFF rate limiting; backend quota.
   - `auth` → reporter re-logs in (`kiro-cli login`); if widespread, auth/CPS issue.
   - `stream_truncated` → relay health; KAS oncall. The session usually survives — resume it.
   - `network`/`timeout` → reporter-side connectivity before anything else.
3. **`fell_back_local` instead of an error?** The session silently ran locally because KAS didn't
   advertise the cloud capability. Expected when the rollout gate is off for that user, the
   endpoint env is unset, or the kill switch is active. A fleet-wide spike of `fell_back_local`
   ≈ the kill-switch signature — check whether a kill was deliberate before treating it as an
   incident.
4. **Reproduce with debug logs.** Client side:

   ```bash
   KIRO_TUI_LOG_LEVEL=debug kiro chat --cloud
   # log file: $TMPDIR/kiro-log/kiro-tui.log
   ```

   The default level is `error`, so an after-the-fact log usually has nothing — ask the reporter
   to reproduce with the env var set. Useful lines to look for: the KAS spawn, the capabilities
   dump from the initialize handshake (shows exactly what KAS advertised — if `executionTargets`
   lacks `cloud-sandbox`, the endpoint/rollout chain is where to look), the
   "did not advertise executionTarget" warning on silent fallback, and "KAS session created"
   with the session id.
5. **Capture the session id before escalating.** It is the correlation key across all three
   teams' logs (CLI ↔ KAS relay ↔ BFF). Sources: the `/sessions` listing, the
   "KAS session created" log line, or `kiro chat --list-sessions` (cloud rows carry it).
6. **Common user-fixable states**, before opening anything:
   - Never logged in / expired login → `kiro-cli login` in a new terminal.
   - No source provider connected → the CLI points at the web portal to connect GitHub; the
     `/repo` picker stays empty until then (this is a setup gate, not an outage).
   - `--repo` name dropped with a warning at create → the repo isn't visible to the connected
     provider grant; verify it in the web portal.

## Kill switch

Mechanism (code-verified; see design doc §Gating): KAS only advertises cloud capability when the
launcher passes `KIRO_REMOTE_SESSIONS_ENDPOINT`, and the launcher only passes it when the
`remote_sandbox` rollout gate is on.

- **Kill for gated cohorts (release-cycle latency):** CR setting
  `remote_sandbox.treatment_percent: 0` in `crates/chat-cli/rollout.json`. Effect: CLIs silently
  degrade to local sessions (`fell_back_local` rises; `create_failed` does NOT rise). Local chat
  is unaffected.
- **Immediate kill for shipped binaries:** backend-side — BFF refuses cloud RPCs. Users then see
  classified errors with guidance rather than silent fallback. Backend-owned; engage the KAS/BFF
  oncall. Use this (not the silent path) if the kill is security-motivated and users must know.
- **Verification either way:** internal build `kiro chat --cloud` → local fallback, no error;
  `fell_back_local` metric increments; existing relayed sessions keep running server-side
  (decide per incident whether backend drains them).
- Log every activation (date / lever / reason / duration) in the ops log.

## Ownership boundary (who owns what)

The ACP boundary is the ownership boundary. If the problem is in how the CLI renders, gates,
classifies, or launches — it's ours. If the problem is past KAS's outbound edge — relay
websocket, BFF APIs, remote session store, sandbox fleet, source-provider catalog — it belongs to
the KAS/backend team.

| Component | Owner | Where to look first |
|---|---|---|
| TUI rendering, gating, error classification, `--cloud`/`/repo` UX | CLI team (us) | `kiro-tui.log`, `kiro diagnostic` |
| KAS process, ACP handshake, capability advertisement | KAS/agent team | KAS logs (the CLI captures spawn + handshake lines) |
| Relay websocket, turn streaming | KAS/agent team | `stream_truncated` error kind; session id |
| BFF (Kiro Web Portal Service) APIs, remote store, provider catalog | Backend/services team | `version_skew`/`throttling`/`server_error` kinds |
| Sandbox fleet (provisioning, clones) | Backend/services team | `provision_failed` events; ready-latency panel |
| Auth identity (CPS), token validity | CLI first (`crates/chat-cli-v2/src/auth/kas_token.rs` wire contract), then CPS | `auth` error kind |
| Telemetry pipeline (metrics missing wholesale) | KUTS/telemetry team | `metrics_and_telemetry.md` |

## Escalation paths

- **CLI (us):** CTI `Kiro / CLI / Intake`, resolver group `Amazon Q for CLI` — same queue and
  rotation as the rest of this runbook set (see [runbook.md](runbook.md) for the queue link and
  oncall-summary doc).
- **KAS / agent server (relay, capability advertisement, sandbox-side agent):** CTI
  `AWS / Kiro / AgentServer`. Cut a ticket with the **session id**, the reporter's
  `kiro diagnostic` output (KAS version list), and the classified error kind — that triple lets
  them find the relay/BFF trace without a round-trip.
- **Backend / web portal service (BFF, remote store, sandbox fleet, source providers):** reach
  through the AgentServer CTI above or the kiro-services team channel; they route internally.
  For a suspected KAS↔BFF deploy mismatch (`version_skew` firing), say so explicitly — the fix is
  a deploy-ordering correction on their side, and time matters while the fleets disagree.
- **Telemetry pipeline:** if cloud metrics disappear but the feature demonstrably works (sessions
  create fine, dashboard blank), that's ingestion, not the feature — escalate per
  [metrics_and_telemetry.md](metrics_and_telemetry.md) rather than paging the backend team.
- **Severity guide:** backend-wide cloud outage on an internal-only cohort is a Sev-3 with a
  clear owner handoff (dark-shipped, no customer impact). The same signature after external ramp
  is Sev-2.5 — the alarms encode these severities once paging is enabled.

When escalating cross-team, always attach: session id, UTC timestamp, classified error kind
(from the dashboard errors panel or the user-visible guidance line), CLI version, and KAS version
(from `kiro diagnostic`). Those five fields are the shared correlation vocabulary across all
three teams' logs.
