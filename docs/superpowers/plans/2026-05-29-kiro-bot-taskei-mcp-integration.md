# Kiro-Bot × Taskei Remote MCP Integration — Plan

**Status:** Draft (revised 2026-06-01 — bundled-shim pivot per PR #2727 review)
**Date:** 2026-05-29
**Branch:** `kiro-bot/taskei-mcp-integration`
**Target crates:** new `crates/kiro-mcp/` (bundled shim — see §2), edits to `crates/kiro-bot/`
**Target deploy account:** `551670267384` (already approved for Taskei MCP)
**Taskei MCP gateway account:** `962531445649` (prod), endpoint `https://iad.prod.service.mcp.taskei.amazon.dev/mcp`

**Revision note (2026-06-01):** Following kensave's review on PR #2727
([discussion_r3336117006](https://github.com/kiro-team/kiro-cli/pull/2727#discussion_r3336117006)),
the integration pivots from a third sibling shim crate
(`crates/kiro-taskei-mcp/` alongside `kiro-knowledge-mcp` /
`kiro-github-mcp`) to a single bundled binary `crates/kiro-mcp/` that
hosts taskei (and over time knowledge / github) as tool-family modules,
mirroring `builder-mcp`'s shape. Read/write isolation moves from
process-level (separate binaries per scope) to tool-level: MCP
`readOnlyHint` annotations + per-call STS write-role assumption.
Sections 2, Phase 1a–1d, Phase 2, Phase 3c, §4, and §5 updated accordingly.

## 1. Goal

Wire kiro-bot's `kiro-help` agent to the Taskei Remote MCP gateway so a Slack
user reporting a bug or feature request gets:

- a **dedup search** against existing Taskei tasks in the configured room(s),
- on duplicate → a **+1 / cross-link comment** posted on the existing task
  after human :thumbsup: in Slack,
- on novel report → a **drafted task**, posted to Slack for review, then
  **created** in the configured Taskei room after human :thumbsup:.

All write paths route through kiro-bot's existing reaction-approval gate
(`crates/kiro-bot/src/engine/acp.rs:48-83`). The integration must isolate
AWS SigV4 + STS code from the shared `crates/agent/` loader, preserve
kiro-bot's cluster-safe dispatch semantics, and keep IAM authority minimal at
every phase.

## 2. Integration strategy

- **Transport — one bundled stdio shim.** Land `crates/kiro-mcp/` as a
  single binary that exposes the union of tool families (taskei in
  Phase 1; knowledge + github migrated in via a follow-up plan).
  Spawned by the existing `LocalMcpServerConfig` path in
  `crates/agent/src/agent/mcp/service.rs:104-147`. Mirrors `builder-mcp`'s
  shape (one server, multiple tool families) — flagged by kensave on
  PR #2727
  ([discussion_r3336117006](https://github.com/kiro-team/kiro-cli/pull/2727#discussion_r3336117006)).
  The shim signs outbound HTTP with SigV4 (service `execute-api`) and
  proxies to the Taskei prod gateway. We do **not** add SigV4 to the
  shared `HttpServiceBuilder` in `crates/agent/` — that stays consumer-safe
  for kiro-cli end users.

- **Why bundled, not three siblings:**
  - One SigV4 client, one STS cache, one `--allow-rooms` enforcer reused
    across taskei / knowledge / github tool families.
  - Single `kiro-help.json` MCP entry — collapses kiro-bot's startup-time
    MCP wait from N concurrent inits to 1 (one process spawn, one
    handshake, one set of init notifications).
  - Operationally: one binary on `$PATH`, one process to debug, one
    audit-log pipeline.

- **Read/write split, redesigned at the tool level.** Three layers carry
  the scope information instead of relying on separate processes:

  1. **MCP tool annotations** at the server. Every tool the bundled
     shim exposes declares `readOnlyHint` and `destructiveHint` per the
     MCP spec. `Taskei___list_tasks` → `readOnlyHint: true`;
     `Taskei___create_task` → `readOnlyHint: false, destructiveHint:
     false`; `Taskei___update_task` is `readOnlyHint: false` (because
     of the `addComment` / mutation paths).
  2. **kiro-bot approval policy** (`crates/kiro-bot/src/engine/acp.rs:232-271`)
     keys off `readOnlyHint` instead of MCP server name. `Approve` if
     the hint is true; `Ask` (reaction gate) otherwise. A per-tool
     allowlist in `agent.json` (`auto_approve: ["Taskei___list_*", ...]`)
     stays as a humans-can-override escape hatch and as a second layer
     against annotation drift.
  3. **IAM** — see next bullet.

- **IAM — per-call STS, single shim process.** Shim base creds (ECS task
  role) hold `sts:AssumeRole` on **both** roles when split. Read tools
  assume the Read role once at boot, refresh on a 50-min recycle. Write
  tools call `AssumeRole(KiroBotTaskeiMcpWriteRole)` *fresh per
  invocation*, bound to one signed HTTP call, ≤60s session, dropped
  after. **A read code path cannot escalate to writes without
  explicitly invoking the write-role STS path** — the protection moves
  from OS-enforced (separate PIDs) to code-enforced.
  - **Defense-in-depth note:** we lose the OS-level guarantee "this
    PID cannot assume the write role." Compensating controls: per-call
    STS scoping; the bot's reaction-approval gate; the
    `taskId`-must-have-been-listed check (Phase 3b); read-only Rust
    modules under `families/<x>/read/` that do not import the
    `sts_bridge::assume_write` helper. Documented here so future
    readers don't have to rediscover it.
  - **Phase-0 update:** as the Phase-0 retrospective notes, Taskei
    accepted the kiro-bot ECS task role directly as a room member, so
    today there is one role, not two. The shim's `--read-role-arn` /
    `--write-role-arn` args remain optional; `sts_bridge` is a no-op
    when both are unset. Re-introducing the split is non-breaking.

- **Base credentials — task role in prod, profile in dev:** the shim
  uses `aws-config`'s default provider chain. In production this
  resolves to the **ECS task role** via
  `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` (injected by Fargate; no
  `AWS_PROFILE` set on the task). Locally the same chain picks up
  `AWS_PROFILE=kiro-bot`. The shim takes **no** `AWS_PROFILE`
  argument; the only credential-shaping CLI args are
  `--read-role-arn` and `--write-role-arn` (both optional after
  Phase 0). When the split is reintroduced, CDK must put
  `sts:AssumeRole` on both Read and Write roles into the kiro-bot
  ECS task role's permissions, and the trust policy on each role must
  list the task role's ARN.

- **Per-call `roomId` model:** shim accepts `roomId` per call and
  validates it against an `--allow-rooms` allowlist passed at launch.
  Multi-room support comes from the allowlist + a kiro-bot-side
  resolver (Slack channel → roomId), not from one shim per room.

- **Gateway URL — fixed string with env override.** Kensave asked how
  Taskei URLs are vended
  ([discussion_r3336119400](https://github.com/kiro-team/kiro-cli/pull/2727#discussion_r3336119400)).
  The endpoint `https://iad.prod.service.mcp.taskei.amazon.dev/mcp` is
  fixed (single-region, IAD). The shim takes `--endpoint` as a CLI
  arg with that value as the documented default; bot config provides
  the override via `TASKEI_ENDPOINT` env var so dev/beta can point
  elsewhere without rebuilding the image. Multi-region failover is
  **not in scope** for Phases 1–5.

- **API schema — pinned at startup.** Kensave asked whether to pin the
  gateway's tool schema
  ([discussion_r3336123684](https://github.com/kiro-team/kiro-cli/pull/2727#discussion_r3336123684)).
  Strategy: at startup, the shim calls `tools/list` against the
  upstream gateway and asserts it matches a checked-in fixture
  (`crates/kiro-mcp/tests/fixtures/taskei-tools.json`). Mismatch
  produces a paging audit log line and a non-zero exit. Phase 0's
  smoke captured the canonical 7-tool catalog; that becomes the
  fixture. Schema drift is therefore caught at deploy time, not in
  user traffic. Detailed in **Phase 1d**.

- **Attribution gap (acknowledged):** writes are attributed to the
  service role, not the Slack human. Slack user + permalink live in
  the task description and a DynamoDB audit row. The
  `addComment.onBehalfOf = "kerberos:<slack-user>"` path is
  preferred when the gateway honors it (open question §2);
  Maxis-JWT / Federate human-identity federation is tracked as an
  open question, not a phase.

- **Observability from Phase 1:** structured `taskei_audit` tracing
  target ships with the shim, not in a final phase.

`crates/agent/` already supports remote MCP via `RemoteMcpServerConfig`
(`crates/agent/src/agent/mcp/service.rs:149-179`) but the Remote path only
handles static headers + OAuth — no SigV4 request signing. The bundled
shim is the right place for that, and it lets us (a) enforce per-tool
argument validation the agent loader can't, and (b) keep the AWS SDK
out of `crates/agent/`.

## 3. Phased PR plan

Each phase is independently mergeable and end-to-end testable. Read paths
ship before any write path. The `1a-1d` split keeps the shim PRs reviewable.

### Phase 0 — ECS task-role grant + Taskei room membership ✅ DONE 2026-06-01

**Outcome:** completed end-to-end. The original split-Read/Write design
was simplified — Taskei's gateway accepts the **kiro-bot ECS task role
ARN directly** as a room member, so we did not introduce dedicated
`KiroBotTaskeiMcp{Read,Write}Role` roles. The task role is added to
each room with whatever tier (Customer vs Resolver) the room's intent
requires. Re-evaluate the split-role design only if cross-room blast
radius becomes a concern in production.

- **What landed:**
  - Inline statement on the alpha task role's policy:
    `execute-api:Invoke` on `arn:aws:execute-api:*:962531445649:*/mcp/*`
    (added to `Kiro-botCDK` `lib/kiro-bot/stack.ts`, deployed to
    `KiroBotStack-Alpha`).
  - Matching test in `Kiro-botCDK` `tst/kiro-bot/stack.test.ts`.
  - Two Taskei rooms with the alpha task role added as a member:
    - **Kiro CLI** (read-only target, role added as **Customer**) —
      `7c221a81-7ca7-436c-8f05-a7278949341b` — the live team room
      (1231 tasks at smoke time).
    - **Kiro-Sandbox** (read+write test target, role added as
      **Resolver**) —
      `4ac982ae-1285-44e0-af12-8d3662da5935` — empty room reserved for
      Phase 3c/4a write tests.
- **Smoke evidence:** one-shot ECS task running awscurl from the alpha
  task role successfully called `Taskei___list_tasks` against both
  rooms. Server responded `TaskeiMCPService-Prod-us-east-1`, MCP
  protocol `2025-06-18`. Tool catalog returned 7 tools (see
  retrospective below).
- **Exit gate:** ✅ task role can sign and call the gateway from inside
  ECS with no `AWS_PROFILE` set; both rooms return data.

**Phase 0 retrospective — corrections to phases 1d–4c:**

1. **Tool names use `Taskei___` prefix (triple underscore), not
   `taskei_`.** All later phases that reference tool names have been
   updated.
2. **There is no `taskei_post_comment` tool.** Comments are posted via
   `Taskei___update_task` with an `addComment: { message,
   onBehalfOf? }` field. This **simplifies Phase 3c** — the write
   surface is `Taskei___update_task` for both updates and
   commenting, plus `Taskei___create_task` in Phase 4a.
3. **`onBehalfOf` accepts kerberos / ARN.** This is a path through the
   service-principal attribution gap noted in §4: when a Slack human
   triggers a write, the bot can pass their kerberos in `onBehalfOf`
   and the comment / task is attributed to them. Whether the gateway
   honors `onBehalfOf` from a service-principal caller is open
   question §2 — verify before relying on it for prod.
4. **Single task role + per-room tier is sufficient** at this scale.
   Read-only access is enforced by the room granting Customer (not
   Resolver) tier; write paths require a Resolver-tier room. The
   plan's `--read-role-arn` / `--write-role-arn` shim args are kept
   as **optional** so a future split is non-breaking, but Phase
   1c's STS bridge is now a no-op when both are unset.

### Phase 1a — Bundled shim crate skeleton + release pipeline

- **Goal:** empty bundled `kiro-mcp` crate that builds and ships in the
  kiro-bot OCI image, with module scaffolding for tool families.
- **Changes:**
  - **Rename** `crates/kiro-taskei-mcp/` → `crates/kiro-mcp/`. Binary
    name: `kiro-mcp`. The in-progress work on
    `kiro-bot/taskei-mcp-sigv4-client` (Phase 1b SigV4 client) ports as-is
    to the new crate path before that branch lands.
  - `src/main.rs` keeps the existing clap shape (`--region`,
    `--endpoint`, `--scope`, `--read-role-arn`, `--write-role-arn`,
    `--allow-rooms`). Add `--enabled-families` (default `taskei`;
    later `taskei,knowledge,github`).
  - `src/families/mod.rs` + `src/families/taskei/mod.rs` — module
    scaffold, no logic yet. `families/taskei/read/` and
    `families/taskei/write/` submodules established empty so the
    "read code does not import the write-role STS helper" lint
    boundary lands from day 1.
  - Workspace member updated in root `Cargo.toml` (rename, not new).
  - Update binary name in `.github/workflows/kiro-bot-release.yml` and
    the runtime image referenced in
    `crates/kiro-bot/PHASE-6-FOLLOW-UP.md`.
- **Test plan:** `cargo build -p kiro-mcp` clean; release CI green;
  `kiro-mcp --help` runs in the built image; `--enabled-families taskei`
  parses cleanly.
- **Exit:** binary lands in ECR / runtime image.
- **Depends on:** Phase 0.

### Phase 1b — SigV4 client + creds refresh + audit log

- **Goal:** sign requests, fail closed on cred failure, log every call.
- **Changes:**
  - `crates/kiro-mcp/src/sigv4_client.rs` wrapping `reqwest::Client`,
    signing with `aws-sigv4` (`service=execute-api`, region from arg).
  - Refresh-on-expiry-5min + hard 50-min recycle.
  - **Fail-closed:** refresh failure exits process with paging-severity log
    on the `taskei_audit` tracing target.
  - Structured JSON audit line per request (request_id, tool name, latency,
    outcome) emitted from this phase already.
- **Test plan:** unit tests for header set, refresh trigger,
  refresh-failure-exits; integration test against a localhost mock
  endpoint.
- **Exit:** signed request to localhost mock returns `200`; refresh failure
  produces a single audit line + non-zero exit.
- **Depends on:** Phase 1a.

### Phase 1c — STS AssumeRole bridge + per-call write sessions

- **Goal:** read-tool calls present Read-role creds; write-tool calls
  re-assume the Write role *per invocation*. Replaces the prior
  "process-level scope" design — scope is now a per-tool property,
  enforced inside one process.
- **Changes:**
  - `crates/kiro-mcp/src/sts_bridge.rs` with role-keyed session caches:
    Read role cached with refresh-on-expiry-5min + 50-min hard recycle;
    Write role cache TTL = single-call (insert → use → drop).
  - clap args `--read-role-arn`, `--write-role-arn` (both **optional**
    after Phase 0 — when both are unset, the bridge is a no-op and the
    base task-role creds drive both read and write paths).
  - The `assume_write` helper lives in `families/<x>/write/` only;
    `families/<x>/read/` modules do not import it. A clippy-restriction
    lint or a `mod.rs`-level visibility boundary enforces this.
- **Test plan:** unit tests verifying (a) read tools never present
  write-role creds and vice versa; (b) cache invalidation on simulated
  expiry; (c) when both role arns are unset, base task-role creds are
  used for both scopes; (d) the read-module-cannot-import-write-helper
  boundary trips a build error if violated.
- **Exit:** scope-bound creds verified in audit log; no-op-when-unset
  path verified.
- **Depends on:** Phase 1b.

**Phase 1c implementation notes (technical drifts from the original draft):**

The bridge as shipped lives at `crates/kiro-taskei-mcp/src/sts_bridge.rs`
(the pre-bundle path); Phase 1c-bundle (below) moves it to
`crates/kiro-mcp/src/sts_bridge.rs` along with the rest of the binary.
Two STS-API-level adjustments:

1. **Write `DurationSeconds`.** The original draft said "≤60s session"
   for write-role assumption. STS rejects values below 900s, so the
   shim requests STS's 900s minimum and drops the
   `AssumeRoleProvider` after a single resolve. The "single-call"
   intent is preserved at the *cache* level — no other call ever
   reuses the snapshot. Bounding wall-clock blast radius below 900s
   is left to IAM and the per-call audit log, not the duration knob.
2. **Read provider caching.** `AssumeRoleProvider` does not cache
   credentials internally (per the SDK's
   `provider_does_not_cache_credentials_by_default` test). Phase 1c
   adds a small in-crate `CachingProvider` wrapper so the read-tool
   fan-out doesn't burn one STS call per invocation, honoring the
   plan's stated 5-min refresh margin and 50-min recycle.

The trybuild compile-fail fixture for the read/write boundary lands in
Phase 1d alongside the `families/<x>/read/` modules that will hold a
`ReadOnlyView`. Until then the boundary is API-shape (the view doesn't
expose `assume_write_once`) plus the visibility on the helper.

### Phase 1c-bundle — rename to `crates/kiro-mcp/` + bundled-shim shape

- **Goal:** finish the rename §2 / Phase 1a's revision note committed
  to: a single `crates/kiro-mcp/` binary with `families/` module
  scaffolding, so Phase 1d's rmcp proxy + read-tool annotations land
  inside the bundled shape (one MCP server entry in `kiro-help.json`,
  not a sibling-crate fan-out). Without this phase the bundled-vs-
  sibling decision quietly slips back to siblings, which review
  ([discussion_r3336117006](https://github.com/kiro-team/kiro-cli/pull/2727#discussion_r3336117006))
  rejected.
- **Why this is its own phase, not a phase-1d prerequisite:** the
  rename touches the workspace member table, the OCI image's binary
  name, the kiro-bot release pipeline, and the Phase-2 `kiro-help.json`
  entry. Folding it into Phase 1d would mix "rename + module split"
  with "rmcp transport + schema pin," and reviewers would have to
  unpick the diff to tell which change broke what. Splitting it lands
  the rename on its own, leaves Phase 1d to do exactly what its name
  says, and keeps each PR independently revertable.
- **Changes:**
  - Rename `crates/kiro-taskei-mcp/` → `crates/kiro-mcp/`. Binary name
    `kiro-mcp`, library name `kiro_mcp`. The Phase-1c `sts_bridge.rs`
    and Phase-1b `sigv4_client.rs` move under the new path unchanged.
  - Workspace `Cargo.toml` member rename (not a new entry — the old
    name leaves the workspace).
  - `src/families/mod.rs` + `src/families/taskei/mod.rs` — module
    scaffold, no logic. `families/taskei/read/` and
    `families/taskei/write/` submodules established empty so the
    "read code does not import the write-role STS helper" lint
    boundary lands from day 1 (trybuild compile-fail fixture lands
    here, not in 1d).
  - Add `--enabled-families` clap arg (default `taskei`; later
    `taskei,knowledge,github`). No behavioral effect yet — knowledge
    and github migrations are out of scope for this integration plan;
    they get a follow-up consolidation plan (§5 open question 6).
  - Update binary name in `.github/workflows/kiro-bot-release.yml` and
    the runtime image referenced in
    `crates/kiro-bot/PHASE-6-FOLLOW-UP.md`.
  - Phase-1c's `kiro-taskei-mcp` binary becomes a deprecated alias for
    one release: keep the old `[[bin]]` entry pointing at the same
    `main.rs` so any in-flight kiro-bot agent.json or local script
    referencing `kiro-taskei-mcp` keeps working until Phase 2's
    `kiro-help.json` switch is merged. The alias is removed in Phase 2.
- **Test plan:**
  - `cargo build -p kiro-mcp` clean; release CI green.
  - `kiro-mcp --help` works; `kiro-mcp --enabled-families taskei`
    parses cleanly.
  - `kiro-taskei-mcp --help` still works (alias).
  - All Phase 1b/1c unit tests pass under the new crate name.
  - The trybuild compile-fail fixture asserting `families/taskei/read/`
    cannot import `sts_bridge::assume_write_once` is added in this
    phase and fails closed when the boundary is violated.
- **Exit:** new binary lands in ECR / runtime image; alias still
  works; trybuild boundary asserted.
- **Depends on:** Phase 1c.

### Phase 1d — rmcp HTTP proxy + tool annotations + schema pin

- **Goal:** end-to-end stdio → signed-HTTP → Taskei, with per-tool
  `readOnlyHint` annotations driving downstream approval policy and a
  startup schema-pin assertion guarding against gateway drift.
- **Changes:**
  - `crates/kiro-mcp/src/families/taskei/mod.rs` implements
    `rmcp::ServerHandler`, exposing the 7 Phase-0-confirmed tools.
    Each tool declares `readOnlyHint` / `destructiveHint` annotations:
    - `Taskei___list_tasks`, `_get_task`, `_get_room`,
      `_list_room_resource` → `readOnlyHint: true`.
    - `Taskei___create_task` → `readOnlyHint: false, destructiveHint: false`.
    - `Taskei___update_task` → `readOnlyHint: false` (mutates state;
      `addComment` is also expressed via this tool).
    - `x_amz_bedrock_agentcore_search` → annotation TBD per Phase-0
      schema review.
  - `crates/kiro-mcp/src/mcp_proxy.rs` opens an rmcp HTTP client using
    the SigV4 `reqwest::Client`, mirroring `list_tools()` /
    `call_tool()`. Read tools ride the Read-role signed client; write
    tools trigger the per-call Write-role STS path before signing.
  - **Schema pin (kensave PR #2727 follow-up):** at startup, the shim
    calls `tools/list` against the upstream gateway and asserts the
    returned set matches a checked-in fixture
    (`crates/kiro-mcp/tests/fixtures/taskei-tools.json`). Mismatch
    emits one paging audit line on the `taskei_audit` target and
    exits non-zero. The fixture seeds from Phase 0's smoke output.
  - Read-tool allowlist (defense-in-depth alongside annotations):
    `Taskei___list_tasks`, `Taskei___get_task`, `Taskei___get_room`,
    `Taskei___list_room_resource`.
  - `--allow-rooms` validation on every call.
  - Retry policy: exponential backoff, max 3 attempts on 5xx/429,
    structured error on exhaustion.
- **Test plan:**
  - `#[ignore]` integration test against real IAD endpoint with ada
    creds; manual mcp-inspector smoke confirms `readOnlyHint`
    annotations are visible in `tools/list`; clippy/fmt clean.
  - **Schema-pin negative test:** mutate the fixture (rename a tool,
    add a phantom argument) and assert the shim exits non-zero with
    one paging audit line.
  - **Container-shaped credential test:** run the shim inside a local
    container with `AWS_PROFILE` unset and
    `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` pointed at a
    credential-server stub returning the kiro-bot task role's creds.
    The shim must successfully call `Taskei___list_tasks`. This is the
    test that catches "works on my laptop, broken in ECS."
- **Exit:** developer can run the shim locally with
  `AWS_PROFILE=kiro-bot` and call `Taskei___list_tasks` over stdio;
  same binary running with no `AWS_PROFILE` and only
  `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` set also succeeds (proves
  the prod credential path); schema-pin negative test fails closed.
- **Depends on:** Phase 1c-bundle.

### Phase 2 — Read-only wiring into kiro-help agent

- **Goal:** bot answers "is there a task for X?" in dev Slack via a
  single bundled `kiro-mcp` server entry.
- **Changes:**
  - `crates/kiro-bot/agents/kiro-help.json`: add a single `kiro-mcp`
    MCP server entry — no separate `-read` / `-write` entries. Read
    tools listed in `allowedTools`; auto-approval is driven by the
    tool's `readOnlyHint` annotation, with the `allowedTools` list as a
    second-layer defense against annotation drift. Example entry:
    ```json
    "mcpServers": {
      "kiro-mcp": {
        "command": "kiro-mcp",
        "args": ["--enabled-families", "taskei"],
        "env": {
          "AWS_REGION": "us-east-1",
          "TASKEI_ENDPOINT": "https://iad.prod.service.mcp.taskei.amazon.dev/mcp",
          "TASKEI_ALLOW_ROOMS": "..."
        }
      }
    },
    "allowedTools": [
      "Taskei___list_tasks",
      "Taskei___get_task",
      "Taskei___get_room",
      "Taskei___list_room_resource"
    ]
    ```
    No `--scope read` flag needed in Phase 2 — the bundled shim
    decides per-tool. (`--scope` is retained as an optional break-glass
    flag at the binary level; see §6.)
  - `crates/kiro-bot/src/engine/acp.rs:232-271`: approval policy reads
    the tool's `readOnlyHint` annotation. `Approve` if true; `Ask` if
    false. The existing `allowedTools` allowlist remains as the second
    layer.
  - `crates/kiro-bot/agents/kiro_help_prompt.md`: prompt fragment
    telling the agent to search Taskei first.
  - Env plumbing in `crates/kiro-bot/src/engine/core.rs` for
    `TASKEI_ENDPOINT`, `TASKEI_ALLOW_ROOMS`, `AWS_REGION` (and
    `TASKEI_READ_ROLE_ARN` / `TASKEI_WRITE_ROLE_ARN` reserved for
    future re-introduction of the role split — currently unset).
- **Test plan:**
  - Local: `kiro-cli bot slack --config dev.toml`, DM bot a bug-shaped
    question, confirm `Taskei___list_tasks` is called and quoted.
  - Coordinator dedup unaffected (verify with two replicas + same
    event).
  - `403` path on revoked room perm surfaces clean error, no ACP crash.
  - Reaction gate untouched (no approval prompt fires for tools whose
    `readOnlyHint` is true).
  - Annotation-drift drill: temporarily flip `Taskei___list_tasks` to
    `readOnlyHint: false` in the shim and confirm the `allowedTools`
    second layer still auto-approves it.
- **Exit:** dev bot quotes live Taskei results in Slack thread; one
  bundled MCP server handshake replaces the prior three-server design.
- **Depends on:** Phase 1d.

### Phase 3a — Dedup ranker (read-only, no writes)

- **Goal:** bot proposes a draft +1 comment **without** posting it; pure
  observability of ranker quality before any write opens.
- **Changes:**
  - `crates/kiro-bot/src/engine/taskei_dedup.rs`: Bedrock embeddings on
    title + description (reuse the kiro-knowledge embedding client),
    recency boost.
  - Prompt update: "post the proposed +1 comment text as a Slack message
    only; do NOT call write tools."
  - Metrics: `TaskeiDuplicateMatchedCount`, score histogram,
    `BedrockEmbedTokensConsumed`.
- **Test plan:** unit tests on canned task lists; one week of dev-Slack
  traffic to tune threshold; metrics visible in CloudWatch.
- **Exit:** ranker precision/recall measured; no writes have occurred yet.
- **Depends on:** Phase 2.

### Phase 3b — Prompt-injection mitigations + reaction-gate hardening

- **Goal:** make the bot safe before any write tool is exposed.
- **Changes:**
  - User-content escaping helper that wraps Slack message text before
    insertion into the prompt.
  - Hard rule in shim: write tools refuse a `taskId` not seen in the same
    conversation's prior `Taskei___list_tasks` / `Taskei___get_task` results
    (per-conversation memory in shim).
  - Reaction-gate cluster fix in
    `crates/kiro-bot/src/frontend/slack/slack.rs:505-610`: persist
    `pending_approvals` in DDB so a `:thumbsup:` to replica B finds replica
    A's draft; lease-then-forward semantics tested with two-replica
    harness.
  - Slack-sender allowlist: writes only fire for real human users (no
    bot/app subtypes).
- **Test plan:** prompt-injection corpus (50 hostile messages — all must
  refuse or no-op); two-replica reaction routing test; bot-subtype
  rejection.
- **Exit:** corpus passes; reaction tests pass on a 2-pod cluster.
- **Depends on:** Phase 3a.

### Phase 3c — Comment-on-duplicate write path

- **Goal:** post +1 cross-link comment on existing task after
  `:thumbsup:`. **No new MCP server entry** — same bundled `kiro-mcp`
  process serves the write tool.
- **Changes:**
  - In `kiro-help.json`, add `Taskei___update_task` to `allowedTools`
    so the bot will *consider* the call. Because the tool's
    `readOnlyHint` is `false`, the reaction gate fires automatically;
    we deliberately do NOT add it to any `auto_approve` list.
  - Allowed write tool: `Taskei___update_task` with the `addComment`
    field. (Confirmed in Phase 0 — there is no separate
    `taskei_post_comment`; commenting is a sub-field of
    `update_task`.) Pass `addComment.onBehalfOf =
    "kerberos:<slack-user>"` to attribute the comment to the human,
    pending verification of open question §2 (does the gateway honor
    `onBehalfOf` from a service-principal caller?).
  - **STS path activates here.** The shim's
    `families/taskei/write/` module invokes `sts_bridge::assume_write`
    per call; if `--write-role-arn` is unset (current Phase-0 state),
    the bridge no-ops and the base task-role creds drive the write,
    relying on the room's Resolver-tier membership.
  - **Deploy ordering** (Taskei UI + bot image): bot image with gate
    logic rolls out first; then ensure the task role has Resolver tier
    on the target room (already done for Kiro-Sandbox in Phase 0);
    then add `Taskei___update_task` to `allowedTools` in agent config.
    Disabling writes is a one-line config revert — remove the tool
    name from `allowedTools` — no code deploy required (this also
    serves as the kill switch flagged by the bot-rcm review).
- **Test plan:** dev bug → bot proposes → human :thumbsup: → comment
  posted exactly once; cancel path clean; double-react idempotency on
  `(taskId, slack_message_ts)`; cluster-dedup with two replicas;
  `onBehalfOf` attribution verified end-to-end against Kiro-Sandbox
  before any prod-room exposure.
- **Exit:** end-to-end +1 flow works in dev with audit row + Slack
  permalink.
- **Depends on:** Phase 3b.

### Phase 4a — `Taskei___create_task` happy path

- **Goal:** novel reports become tasks; tolerable double-create on retry
  (mitigated by user-visible draft-then-approve).
- **Changes:**
  - Add `Taskei___create_task` to `allowedTools` in `kiro-help.json` —
    no separate MCP server entry, no `auto_approve` (annotation
    `readOnlyHint: false` ensures reaction gate fires). `roomId`
    validated against `--allow-rooms`.
  - Draft template in prompt: title ≤80 chars; description includes
    "Reported by `<slack-user>` in `<permalink>`" header + verbatim
    message.
  - "Created `<taskId>` — `<task-url>`" reply in Slack thread.
- **Test plan:** dev novel report → task created with correct metadata;
  cancel path produces no task; permission downgrade surfaces clean 403.
- **Exit:** end-to-end create works in dev.
- **Depends on:** Phase 3c.

### Phase 4b — Idempotency with composite key

- **Goal:** no double-creates on retry; future-proof for multi-room.
- **Changes:**
  - DDB table `kiro-bot-taskei-idempotency` keyed
    `(roomId, slack_thread_ts) → taskId` with conditional-write semantics.
  - Remove any "list-then-create" fallback (racy across replicas).
- **Open question gating this phase:** does Taskei MCP accept an
  idempotency-token header? If yes, simpler shim-side; if no,
  conditional DDB write is the only mechanism.
- **Test plan:** kill ACP worker mid-create with `kill -9`; assert single
  task on restart.
- **Exit:** idempotency proven under fault injection.
- **Depends on:** Phase 4a.

### Phase 4c — Follow-up updates in same Slack thread

- **Goal:** a second message in the same thread updates the existing task
  rather than creating a new one.
- **Changes:** lookup `(roomId, slack_thread_ts) → taskId`; call
  `Taskei___update_task` instead of create.
- **Test plan:** thread-reply path produces an update, not a create;
  out-of-thread reply produces a fresh task.
- **Exit:** update flow validated.
- **Depends on:** Phase 4b.

### Phase 5 — Multi-room + dashboards + budget alarms

- **Goal:** onboard a second team via config alone.
- **Changes:**
  - `[taskei.rooms]` TOML map in bot config (Slack channel → roomId).
  - Per-room prompt fragments under
    `crates/kiro-bot/agents/taskei-rooms/<roomId>.md`.
  - CloudWatch dashboard: call count, latency, dedup rate, create rate,
    rejection rate.
  - Bedrock per-room daily-budget alarm.
  - Slack rate-limit back-pressure: drop draft + audit row when posting
    fails under rate limit.
  - Onboarding runbook in `docs/kiro-bot/`.
- **Test plan:** onboard second test room with config-only change;
  synthetic-load test fires the budget alarm.
- **Exit:** second room live without code change.
- **Depends on:** Phase 4c.

## 4. Cross-cutting risks & mitigations

| Risk | Mitigation |
|---|---|
| Service-principal write attribution loses human identity | Slack user + permalink in description + DDB audit row; Maxis-JWT federation as open question. |
| Prompt injection via Slack content into tool calls | Phase 3b: content escaping + `taskId`-must-have-been-listed check + Slack-sender allowlist. |
| IAM widens silently on first write phase | Split Read/Write roles from Phase 0; Write role assumed per write call only. |
| Reaction gate races across replicas | Phase 3b: persist `pending_approvals` in DDB; lease before forward; tested on 2-pod harness. |
| Idempotency under retry / multi-room | Composite key `(roomId, slack_thread_ts)` from Phase 4b; DDB conditional write; no list-then-create. |
| Cred refresh failure | Fail-closed exit + paging-severity audit row from Phase 1b. |
| Bedrock cost blow-up per tenant | `BedrockEmbedTokensConsumed` metric + per-room daily budget alarm in Phase 5. |
| Slack rate-limit storm during outage flood | Back-pressure: drop draft cleanly, audit row recorded; Phase 5. |
| Account/role allowlist drift | Pinned `roleName` in CDK + synth-time Aspect; documented per-stage in onboarding runbook. |
| Multi-stage (dev/beta/prod) coordination | Phase 0 onboarding form lists all three account/role pairs upfront. |
| Loss of OS-level read/write process isolation in the bundled shim | Per-call Write-role STS assumption (Phase 1c); reaction-approval gate keyed off `readOnlyHint`; `taskId`-must-have-been-listed check (Phase 3b); `families/<x>/read/` modules forbidden from importing `sts_bridge::assume_write` (lint-enforced); `allowedTools` second-layer name allowlist guards against annotation drift. |
| Tool annotation drift (a tool ships with wrong `readOnlyHint`) | `allowedTools` in `agent.json` is the second layer — explicit name-based allowlist of tools the bot will dispatch. Annotation-only would be one-layer; this gives two. |
| Gateway tool schema changes silently | Phase 1d schema-pin fixture + startup assertion + paging exit on mismatch. Drift caught at deploy, not in user traffic. |
| Single-process blast radius (one panic kills all bundled tool families) | Bundled binary is small and well-fenced (each family in its own module); panics produce a paging audit row + non-zero exit, the agent loader treats it as init failure, the bot continues with no Taskei tools rather than crashing. |

## 5. Open questions

1. **Idempotency token at the Taskei API layer?** Gates Phase 4b's
   design. The `tools/list` schemas for `Taskei___create_task` and
   `Taskei___update_task` (captured in Phase 0) do not surface an
   idempotency-token field — strong signal we'll need DDB
   conditional-write as the canonical mechanism.
2. **Does the Taskei gateway honor `onBehalfOf` from a service-principal
   caller (the bot's task role)?** The schema accepts kerberos and
   ARN values for `onBehalfOf` on both `Taskei___create_task` and
   `addComment` — if the gateway accepts it, it closes the
   service-principal attribution gap without a Maxis-JWT migration.
   Verify in Phase 3c with a sandbox-room write before exposing to
   prod traffic.
3. **Does the gateway distinguish 5xx vs 4xx semantically?** Confirms
   Phase 1d retry policy is correct.
4. **Per-room write rate limits at the gateway?** Affects Phase 5
   back-pressure design.
5. **Stage-account topology** — does kiro-bot have separate
   beta/prod accounts, or only `551670267384`? Today only alpha is
   provisioned; beta/prod replication is a Phase 5 concern.

**Resolved by Phase 0 smoke (2026-06-01):**
- Taskei gateway endpoint: `https://iad.prod.service.mcp.taskei.amazon.dev/mcp`,
  protocol `2025-06-18`, server `TaskeiMCPService-Prod-us-east-1`.
- Account `551670267384` and the alpha task role are allowlisted on
  the gateway — no additional Taskei-team coordination per role; only
  per-room membership is needed.
- Tool catalog: `Taskei___{create_task, update_task, get_task,
  list_tasks, get_room, list_room_resource}` plus the
  `x_amz_bedrock_agentcore_search` helper. Comments are posted via
  `Taskei___update_task` with an `addComment` field (no separate
  comment tool).

**Resolved by PR #2727 review (2026-06-01):**
- **Bundled vs sibling shim:** bundled. Single `crates/kiro-mcp/`
  binary hosts taskei (Phase 1) with knowledge / github migration as
  a follow-up.
- **Gateway URL vending:** fixed string with `TASKEI_ENDPOINT` env
  override. Single-region (IAD); multi-region failover out of scope
  for Phases 1–5.
- **API schema pinning:** startup assertion against checked-in
  `tools/list` fixture (Phase 1d); paging exit on mismatch.

**New / deferred:**
6. **Should `kiro-knowledge-mcp` and `kiro-github-mcp` be folded into
   `crates/kiro-mcp/` as additional families?** Decision deferred to a
   follow-up plan (`docs/superpowers/plans/<date>-kiro-mcp-consolidation.md`).
   Out of scope for this integration — taskei lands as the first
   family; the consolidation plan can land independently once the
   bundled shim is proven.

## 6. Operational notes

- **`--scope` retained as a break-glass flag.** The bundled `kiro-mcp`
  binary still accepts `--scope read` / `--scope write` at the binary
  level. Default operation does not pass it (scope is decided per-tool
  via `readOnlyHint`). When set, the binary refuses to dispatch any
  tool whose annotation contradicts the flag, providing a process-level
  "read-only sidecar" mode for ops/debugging or for a future deployment
  shape that wants OS-enforced isolation back.
- **Kill switch.** Disabling Taskei tools entirely is a one-line revert
  in `kiro-help.json` (remove the entries from `allowedTools`); no code
  deploy required. This satisfies the kill-switch concern raised by the
  bot-rcm review of PR #2727.
