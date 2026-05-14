# Kiro Help Bot — Design

**Status:** Draft
**Date:** 2026-05-13
**Target crate:** `crates/kiro-bot` (extended) + new `crates/kiro-knowledge-mcp`
**Target deploy account:** `551670267384` (AWS)

## Goal

Run the existing `kiro-bot` runtime as an internal Slack bot that helps
Amazonians use `kiro-cli`. The bot answers questions about kiro-cli
behavior, helps debug user-pasted errors, and triages bug reports —
either pointing the user at an existing GitHub issue / Taskei task /
t.corp ticket, or drafting a new one with the user's approval.

### What this solves

Today, kiro-cli questions land in Slack and stay there — sometimes
answered, sometimes not, often re-asked. Real bugs get re-filed across
GitHub, Taskei, and t.corp because the reporter doesn't know the others
exist. The bot gives users a single place to ask, grounds answers in the
canonical docs/issues, and turns "is this a known bug?" into a one-line
question.

### v1 scope

- **Q&A about kiro-cli**, grounded in `docs/`, `autodocs/`, GitHub
  issues, and release notes via a Bedrock Knowledge Base.
- **Debug help** by interpretation — the user pastes an error or command
  output; the bot reasons about it. **No** access to the user's machine
  or telemetry.
- **Live triage and dedupe** across GitHub issues, Taskei, and t.corp,
  with the agent calling each source via MCP at request time.
- **Issue creation** in any of the three systems, gated by an in-Slack
  reaction-approval flow — the user always sees the draft before
  anything is filed.
- **Strong per-thread isolation** with cross-task session pinning and
  durable transcript storage so a thread survives task restarts and
  deploys.

### Out of scope for v1

- Per-user kiro-cli compute (the grand vision — running kiro-cli sessions
  on behalf of users in cloud sandboxes).
- External Slack-app distribution (multi-tenant, per-workspace install).
- Custom kiro-cli plugin or extension execution from Slack.
- Rich Slack interactivity beyond reactions (no Block Kit pickers, no
  slash commands beyond `!cmd`).
- Multi-language support (English only).
- Voice / mobile-first integration.
- Integration with internal observability for the user's local
  kiro-cli telemetry.

## Architecture

```
                 ┌─────────────┐
                 │   Slack     │
                 │ (workspace) │
                 └──────┬──────┘
                        │ Socket Mode WebSocket (outbound only)
                        │
┌───────────────────────┼───────────────────────────────────────────┐
│ AWS account 551670267384                                          │
│                       ▼                                           │
│  ┌────────────────────────────────────────────────────────────┐   │
│  │ ECS Fargate service  "kiro-help-bot" (2 tasks)             │   │
│  │                                                            │   │
│  │  ┌───────────────────────────┐   spawn                     │   │
│  │  │ kiro-bot (Slack frontend) │───────▶ kiro-cli acp pool   │   │
│  │  └────────────┬──────────────┘   (per-conversation worker) │   │
│  │               │ MCP                                        │   │
│  │               ▼                                            │   │
│  │  ┌───────────────────────────┐                             │   │
│  │  │ kiro-knowledge-mcp        │                             │   │
│  │  │ (in same container)       │                             │   │
│  │  └─────────────┬─────────────┘                             │   │
│  └────────────────┼───────────────────────────────────────────┘   │
│                   │                                               │
│   ┌───────────────┴────────────────┐                              │
│   ▼                                ▼                              │
│  ┌─────────────────┐    ┌────────────────────────────┐            │
│  │ Bedrock KB      │    │ Secrets Manager            │            │
│  │ (docs + issues) │    │  slack/bot, slack/app,     │            │
│  └────────▲────────┘    │  github/pat                │            │
│           │             └────────────────────────────┘            │
│           │ hourly sync                                           │
│   ┌───────┴────────────┐    ┌──────────────────────────┐          │
│   │ S3: kiro-help-     │◀───│ Ingest Lambda (hourly)   │          │
│   │   corpus/          │    │  pulls git + gh issues   │          │
│   └────────────────────┘    └──────────────────────────┘          │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐   │
│  │ DynamoDB                                                   │   │
│  │   kiro-bot-leases       (conversation_id → owner_task)     │   │
│  │   kiro-bot-transcripts  (conversation_id, turn_seq → text) │   │
│  │   kiro-bot-event-dedup  (slack_event_id, TTL 5min)         │   │
│  │   kiro-bot-feedback     (msg_id → 👍/👎 + chunk_ids)        │   │
│  └────────────────────────────────────────────────────────────┘   │
│                                                                   │
│  CloudWatch Logs  •  CloudWatch Metrics  •  Alarms → oncall       │
└───────────────────────────────────────────────────────────────────┘
```

### Key properties

- **Two Fargate tasks, one Slack app.** Slack Socket Mode picks one task's
  WebSocket; the other is a hot spare. Strong session pinning (below)
  ensures messages for a given conversation always land on the task that
  owns it, regardless of which task Slack delivered the event to.
- **Conversation = ACP session.** Threads, channels, and DMs each get
  their own `kiro-cli acp` subprocess with its own MCP servers. Already
  implemented in `kiro-bot` today; we layer cross-task pinning on top.
- **In-process knowledge MCP.** `kiro-knowledge-mcp` runs as a child
  process of each ACP worker via the existing MCP plumbing. Pluggable —
  the contract is just an MCP tool, so it can be split into its own
  Fargate service later without changes to kiro-bot.
- **Read-only by default; writes always gated.** Search tools auto-approve;
  `create_*` and `comment_on_*` tools route through kiro-bot's existing
  Slack-reaction approval flow.
- **Source split.** Rust source + GH Actions live in `kiro-team/kiro-cli`
  (this repo). The Dockerfile, `entrypoint.sh`, and CDK stacks live in a
  separate internal package. The interface is binaries in S3 plus a few
  SSM parameters.

## Components

### 1. New Rust crate: `crates/kiro-knowledge-mcp/`

Small (~200 LOC) MCP stdio server, statically linked. Exposes one tool:

```json
{
  "name": "search_kiro_knowledge",
  "description": "Search the kiro-cli documentation, GitHub issues, and release notes for relevant context. Use this whenever the user asks about kiro-cli behavior, errors, or how-tos before answering.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query":         { "type": "string" },
      "source_filter": { "type": "string", "enum": ["docs","issues","releases","all"], "default": "all" },
      "max_results":   { "type": "integer", "default": 5 }
    },
    "required": ["query"]
  }
}
```

Implementation:

- Reads `KIRO_KNOWLEDGE_KB_ID` and `AWS_REGION` from env vars.
- Calls `bedrock-agent-runtime:Retrieve` (not `RetrieveAndGenerate` — the
  agent is the LLM; we want raw chunks).
- Formats the response as numbered chunks with source path + relevance
  score so the agent can cite them.

### 2. New Rust crate: `crates/kiro-bot/evals/`

A `cargo` binary `kiro-bot-eval` plus a JSONL data file
`kiro-help.jsonl`. Each entry:

```json
{
  "id": "auth-001",
  "query": "kiro-cli is asking me to log in but the browser flow hangs",
  "must_retrieve_any_of": [
    "docs/authentication.md",
    "github_issue:kiro-team/kiro-cli#1247"
  ],
  "category": "auth"
}
```

Runner calls Bedrock KB Retrieve for each query, computes recall@5, exits
non-zero if recall < 0.7. Used both as a pre-merge informational signal
on PRs and as a build-blocking gate on main.

### 3. Extension to `crates/kiro-bot/`: cross-task coordinator

New module `crates/kiro-bot/src/engine/coordinator.rs`. Trait-based so
local CLI dev and the cron frontend keep working without AWS:

```rust
#[async_trait]
pub trait Coordinator: Send + Sync {
    async fn try_acquire(&self, conversation_id: &str) -> LeaseOutcome;
    async fn renew(&self, conversation_id: &str);
    async fn release(&self, conversation_id: &str);
    async fn forward(&self, peer: &str, payload: ForwardEvent) -> Result<()>;
    async fn append_turn(&self, conversation_id: &str, turn: Turn);
    async fn load_history(&self, conversation_id: &str, limit: usize) -> Vec<Turn>;
    async fn dedupe_event(&self, slack_event_id: &str) -> bool; // true = first time
}

pub enum LeaseOutcome {
    Acquired,
    Held { peer: String },     // forward to this peer
    Unavailable,                // coordinator is down — fall back to in-mem
}
```

Two implementations:

- `NoopCoordinator` — current in-memory behavior. `try_acquire` always
  returns `Acquired`; transcripts not persisted. Used by CLI/cron
  frontends and for `[coordinator] type = "none"`.
- `DynamoCoordinator` — backed by the DynamoDB tables above plus ECS
  Cloud Map for peer discovery (`kiro-bot.local`).

New `[coordinator]` block in `config.toml`:

```toml
[coordinator]
type = "dynamodb"                          # or "none"
leases_table = "kiro-bot-leases"
transcripts_table = "kiro-bot-transcripts"
dedup_table = "kiro-bot-event-dedup"
peer_discovery = "cloud_map:kiro-bot.local"
peer_port = 8080
lease_ttl_secs = 300
transcript_history_limit = 20
```

The bot core ([crates/kiro-bot/src/engine/core.rs:207](crates/kiro-bot/src/engine/core.rs:207))
gains a coordinator step before dispatching to the worker pool: dedup
the event, attempt to acquire the lease, either dispatch locally or
forward to the peer.

### 4. New `kiro-help` agent mode (kiro-cli side)

A new mode definition committed to kiro-cli alongside the other modes
(exact path confirmed during implementation). The mode bundles:

1. **System prompt** — instructs the agent to call `search_kiro_knowledge`
   before answering kiro-cli questions, to call live `gh`/Taskei/t.corp
   tools when triaging bugs, and to draft (not file) issues until the
   user reacts ✅.
2. **Tool allowlist** — see "Tools" below.
3. **Approval policy hint** — read tools auto-approve, write tools ask.
4. **Tool-call budget** — hard cap of 12 tool calls per prompt to prevent
   runaway loops; bot kills the prompt with an error if exceeded.

The mode file is the spec. Behavior changes = PR-reviewed changes to it.

### 5. Bot config directory: `kiro-help/`

A new bot config directory committed to the kiro-cli repo at
`crates/kiro-bot/kiro-help/` (sibling of `issue-dedupe/`):

```
crates/kiro-bot/kiro-help/
    config.toml
    policies/agents.cedar
    README.md
```

`config.toml`:

```toml
name = "kiro-help"
working_directory = "/var/lib/kiro-bot/kiro-help"  # tmpfs in container

[frontend]
type = "slack"
bot_name = "KiroHelp"
bot_member_id = "<filled at install>"
conversation_history = 0           # transcript comes from coordinator, not Slack

[agent]
command = "kiro-cli acp"
model = ""                         # use kiro-cli default
default_mode = "kiro-help"
approval_policy = "ask"
max_workers = 10
idle_timeout_secs = 300
mcp_wait_ms = 3000

[coordinator]
type = "dynamodb"
leases_table = "kiro-bot-leases"
transcripts_table = "kiro-bot-transcripts"
dedup_table = "kiro-bot-event-dedup"
peer_discovery = "cloud_map:kiro-bot.local"
peer_port = 8080
lease_ttl_secs = 300
transcript_history_limit = 20

[authorization]
cedar_policy_file = "policies/agents.cedar"

[[response_policies]]
conversation = "dm:*"
trigger = "always"
reply = "inline"

[[response_policies]]
conversation = "*"
trigger = "directed_only"
reply = "thread"
```

`policies/agents.cedar` for v1: allow any user in any allowlisted
channel. One `permit` rule per allowlisted channel; new channels are
added by appending another rule and redeploying. Example:

```cedar
@id("allow-kiro-help-channel")
permit(
  principal,
  action == Action::"use_bot",
  resource == Conversation::"channel:C0KIROHELP1"
);

@id("allow-kiro-help-dms")
permit(
  principal,
  action == Action::"use_bot",
  resource
) when {
  resource like Conversation::"dm:*"
};
```

### 6. Tools available to the `kiro-help` agent

| Tool | Source | Read/Write | Approval |
|---|---|---|---|
| `search_kiro_knowledge` | Bedrock KB (hourly index) | R | auto |
| `search_github_issues` | live `gh api` | R | auto |
| `search_taskei` | builder-mcp | R | auto |
| `search_tcorp` | builder-mcp | R | auto |
| `kiro_cli_help` | narrow allowlist of `kiro-cli --help` / `--version` invocations only — **not** generic shell access | R | auto |
| `create_github_issue` | `gh api` | W | ask |
| `create_taskei_task` | builder-mcp | W | ask |
| `comment_on_existing` | live `gh`/builder-mcp | W | ask |

GitHub tools shell out to `gh` using a single fine-grained PAT
(`issues:write` on `kiro-team/kiro-cli` only) stored in Secrets Manager.
Per-task token-bucket limiter at 30 GitHub calls/min guards rate limits.

## Data flow

### Per-message flow (happy path)

```
1. Slack delivers event to task T's WebSocket.
2. T calls coordinator.dedupe_event(slack_event_id).
   - If duplicate: ack, drop. Done.
3. T calls coordinator.try_acquire(conversation_id).
   - Acquired → step 4.
   - Held{peer} → POST /dispatch to peer; ack to Slack. Done.
4. T's worker pool: get_or_spawn(conversation_id).
   - Existing worker → reuse.
   - Cold → spawn kiro-cli acp subprocess (~6s), load history from
     coordinator.load_history(conversation_id, 20), inject as initial
     context.
5. Run prompt; stream tool-status updates to placeholder Slack message.
6. coordinator.append_turn(conversation_id, user_turn);
   coordinator.append_turn(conversation_id, assistant_turn).
7. coordinator.renew(conversation_id) (extend lease TTL).
8. Replace placeholder with final reply.
```

### Cross-task pinning details

- **Conditional acquire:** DynamoDB conditional `PutItem` with
  `attribute_not_exists(owner_task_arn) OR lease_expires_at < now`.
- **Forwarding:** Tasks register in ECS Cloud Map under `kiro-bot.local`.
  Peer discovery resolves the SRV record; forward is HTTP POST to
  `http://<peer-ip>:8080/dispatch` carrying the original Slack event.
  Peer processes as if Slack delivered it directly.
- **Lease expiry:** TTL 5 minutes, renewed on every prompt completion. If
  the owning task crashes, the next message arrives, finds the lease
  expired, and either-task takes over via cold path.
- **Rehydration:** Cold path replays the last 20 turns from
  `kiro-bot-transcripts` as injected context (reusing `format_context()`
  in [crates/kiro-bot/src/engine/core.rs:110](crates/kiro-bot/src/engine/core.rs:110)).
  ACP session itself is fresh — kiro-cli acp can't move sessions between
  processes.

### Triage / dedupe flow

```
User: "kiro-cli hangs on auth, anyone else seen this?"
        │
        ▼
Agent runs (in parallel):
  search_kiro_knowledge("kiro-cli hangs on auth")
  search_github_issues("hangs auth", state=open)
  search_taskei("kiro-cli auth hang")
  search_tcorp("kiro-cli auth")
        │
        ▼
   matches found?
   ├── yes ──▶ "I found 2 likely matches: [#1247], [TKE-3389].
   │           Is this your issue?"
   │              ├── "yes, #1247"   ──▶ comment_on_existing (ask)
   │              └── "no, different" ──▶ draft path
   │
   └── no ───▶ Agent drafts title + body, posts in thread:
               "Here's a draft issue. React ✅ to file it,
                ❌ to skip, or reply with edits."
               On ✅: create_github_issue (ask) → post link.
```

Issues filed by the bot start with `Reported by @<slack-handle> via
kiro-help-bot.` System prompt forbids copying DM content into a public
issue without showing the draft first; the reaction-approval gate
enforces this in practice.

## Storage

| Table | Schema | TTL | Purpose |
|---|---|---|---|
| `kiro-bot-leases` | PK `conversation_id`, attrs `owner_task_arn`, `expires_at` | DynamoDB TTL on `expires_at` | Single-writer lease per conversation |
| `kiro-bot-transcripts` | PK `conversation_id`, SK `turn_seq`, attrs `role`, `text`, `ts` | 30d | Durable conversation memory |
| `kiro-bot-event-dedup` | PK `slack_event_id`, attr `processed_at` | 5min | Slack retry de-duplication |
| `kiro-bot-feedback` | PK `slack_msg_id`, attrs `reaction`, `chunk_ids`, `ts` | 90d | 👍/👎 quality signal |

S3 corpus bucket (`kiro-help-corpus-<account>`):

```
docs/<date>/<batch>.jsonl
issues/<date>/<batch>.jsonl
releases/<date>/<batch>.jsonl
manifest.json
```

Each chunk: `{path, content, last_modified, source}`.

## Knowledge pipeline

| Source | How it's pulled | Refresh |
|---|---|---|
| `docs/`, `autodocs/`, `autodocs-v2/`, top-level `*.md` | shallow git clone of `kiro-team/kiro-cli` | hourly |
| `crates/*/README.md`, `packages/*/README.md` | same git clone | hourly |
| `kiro-team/kiro-cli` GitHub issues (open + closed) | `gh api`, incremental since last run | hourly |
| `.changes/` + GitHub releases | git pull + `gh release list` | hourly |

Ingest Lambda (Rust, ~30 MB binary) walks each source, normalizes to
JSONL, writes to S3, calls Bedrock KB `StartIngestionJob`. EventBridge
schedule `0 * * * *`. Emits `IngestSucceeded` / `IngestFailed` metrics;
two consecutive failures alarm.

Chunking is delegated to Bedrock KB (hierarchical default) — the Lambda
deliberately stays dumb.

## Deployment

### Source split

| Lives in `kiro-team/kiro-cli` (this repo) | Lives in internal CDK package |
|---|---|
| `crates/kiro-bot/` (extended with coordinator) | `Dockerfile` |
| `crates/kiro-knowledge-mcp/` (new) | `entrypoint.sh` |
| `crates/kiro-bot/evals/` (new) | CDK stacks (data, ingest, runtime) |
| `crates/kiro-bot/kiro-help/` (config dir) | CodeBuild step that builds image from binaries |
| `.github/workflows/kiro-bot-release.yml` | Runbooks |

### GitHub → AWS contract

Three SSM parameters in account `551670267384`:

| Parameter | Direction | Purpose |
|---|---|---|
| `/kiro-help-bot/<stage>/binary-image-digest` | GH writes, CDK reads | Pinned digest of the binary-carrier image to consume |
| `/kiro-help-bot/<stage>/kb-id` | CDK writes, GH reads | Which Bedrock KB to eval against |
| `/kiro-help-bot/<stage>/config-schema-version` | GH writes, bot validates at startup | Fail-fast on schema drift |

Plus an ECR repo `kiro-bot-binaries` in `551670267384` holding
binary-carrier images: `kiro-bot-binaries:<git-sha>` and
`kiro-bot-binaries@sha256:...`. Each image is a minimal `FROM scratch`
layer carrying just the three statically-linked binaries — no runtime
deps, no shell. Consumed by the runtime Dockerfile via
`COPY --from=...`.

Why a binary-carrier image instead of S3 objects:

- **Content-addressed.** The digest is the artifact identity; nothing
  silently mutates under a tag.
- **One artifact store.** ECR for both the binary-carrier and the
  runtime image. Same auth, same audit trail.
- **Native `COPY --from=`.** The runtime Dockerfile pulls binaries
  with one line; no shell glue.
- **Promotion is `docker tag`.** Beta → prod is retagging a verified
  digest, no re-upload.
- **Signing is built in** via AWS Signer / cosign on ECR if we add it
  later.

### GH Actions workflow (`.github/workflows/kiro-bot-release.yml`)

Triggers:
- Push to `main` touching bot/CLI/eval paths → publishes to beta version channel.
- Tag matching `kiro-bot-v*` → publishes to prod version channel.

Steps (sketch):

1. `cargo test -p kiro-bot -p kiro-knowledge-mcp`
2. `cargo run --bin kiro-bot-eval -- --kb-id $(aws ssm get-parameter --name /kiro-help-bot/beta/kb-id ...) --threshold 0.7` — fails build on regression.
3. `cargo build --release -p kiro-bot -p chat-cli -p kiro-knowledge-mcp` (musl target).
4. `docker buildx build --push --platform linux/amd64 -t <acct>.dkr.ecr.us-west-2.amazonaws.com/kiro-bot-binaries:<git-sha>` using a minimal carrier Dockerfile:

   ```dockerfile
   FROM scratch
   COPY target/x86_64-unknown-linux-musl/release/kiro-bot         /usr/local/bin/kiro-bot
   COPY target/x86_64-unknown-linux-musl/release/kiro-cli         /usr/local/bin/kiro-cli
   COPY target/x86_64-unknown-linux-musl/release/kiro-knowledge-mcp /usr/local/bin/kiro-knowledge-mcp
   ```
5. Capture the resulting image digest from `docker buildx imagetools inspect` and write it: `aws ssm put-parameter --name /kiro-help-bot/<stage>/binary-image-digest --value sha256:<...> --overwrite`.

AWS auth via OIDC. IAM role `arn:aws:iam::551670267384:role/GitHubActionsKiroBotPublisher`,
trust policy scoped to `repo:kiro-team/kiro-cli:ref:refs/heads/main` and
`repo:kiro-team/kiro-cli:ref:refs/tags/kiro-bot-v*`. Permissions:
`ecr:GetAuthorizationToken`, `ecr:BatchCheckLayerAvailability`,
`ecr:PutImage`, `ecr:InitiateLayerUpload`, `ecr:UploadLayerPart`,
`ecr:CompleteLayerUpload` on the `kiro-bot-binaries` repo, plus
`ssm:PutParameter` on the three SSM parameters above. GH secret
`AWS_ACCOUNT_ID_KIRO_BOT = 551670267384`.

`paths:` filter on the workflow so CLI-only commits don't churn the bot
pipeline.

### CDK package (separate repo, internal)

Three stacks, deployed per stage (beta / prod):

| Stack | Contents |
|---|---|
| `kiro-help-bot-data` | DynamoDB tables, S3 corpus bucket, Bedrock KB |
| `kiro-help-bot-ingest` | Ingest Lambda + EventBridge + GitHub PAT secret |
| `kiro-help-bot-runtime` | ECS cluster, Fargate service (2 tasks), task IAM role, Cloud Map service, Slack secrets, CloudWatch alarms, CodeBuild step that builds container image from binaries |

Build flow:

1. CDK runtime stack reads `/kiro-help-bot/<stage>/binary-image-digest`
   from SSM at deploy time.
2. CodeBuild step builds the runtime image. The Dockerfile in the CDK
   package starts with the binary-carrier image consumed by digest:

   ```dockerfile
   ARG BINARY_DIGEST
   FROM <acct>.dkr.ecr.us-west-2.amazonaws.com/kiro-bot-binaries@${BINARY_DIGEST} AS source

   FROM debian:bookworm-slim
   RUN apt-get update && apt-get install -y --no-install-recommends \
       ca-certificates git gh && rm -rf /var/lib/apt/lists/*
   COPY --from=source /usr/local/bin/kiro-bot              /usr/local/bin/
   COPY --from=source /usr/local/bin/kiro-cli              /usr/local/bin/
   COPY --from=source /usr/local/bin/kiro-knowledge-mcp    /usr/local/bin/
   COPY entrypoint.sh /entrypoint.sh
   ENTRYPOINT ["/entrypoint.sh"]
   ```

   CodeBuild passes `--build-arg BINARY_DIGEST=$(aws ssm get-parameter ...)`,
   pushes the resulting image to ECR repo `kiro-bot` in `551670267384`,
   and captures **its** digest as a CodeBuild output.
3. ECS task definition references the runtime image **by digest** (not
   tag) — what the task pulls at run time is exactly what CodeBuild
   just pushed. Combined with the binary-carrier digest also being
   pinned, the full provenance chain is content-addressed end to end.

EventBridge rule on SSM `Parameter Store Change` events fires the CDK
pipeline when GH writes a new `binary-image-digest`.

### Initial bootstrap order

1. Deploy CDK `data` stack → creates KB, writes `kb-id` to SSM.
2. Deploy CDK `ingest` stack → starts hourly indexing.
3. Push to kiro-cli `main` → GH Actions builds binaries, pushes
   binary-carrier image to ECR `kiro-bot-binaries`, writes its digest
   to `binary-image-digest` SSM.
4. Deploy CDK `runtime` stack → CodeBuild builds runtime image with
   `FROM kiro-bot-binaries@<digest>`, pushes to ECR `kiro-bot`, ECS
   starts tasks.

Documented in the CDK package's README.

## Security boundaries

| Boundary | Enforcement |
|---|---|
| Slack → bot | Socket Mode token auth, no inbound network, channel allowlist enforced bot-side via Cedar |
| Bot ↔ AWS | IAM task role, least-privilege: Bedrock KB read; DynamoDB CRUD on its own tables; Secrets Manager read on its own secrets only |
| Bot → GitHub | Single fine-grained PAT, `issues:write` on `kiro-team/kiro-cli` only |
| Bot → builder-mcp | Bot service principal (not user identity); whatever auth builder-mcp requires |
| User → write tools | Reaction-approval gate in kiro-bot (ask policy) — user always sees draft |

No customer data flows through the system in v1. All inputs are public
(GitHub) or internal-Amazon (Slack messages, Taskei, t.corp).

## Observability

Bot-internal metrics via `tracing` + CloudWatch metrics exporter:

- `MessagesReceived`, `MessagesAnswered`, `MessagesFailed`
- `WorkerSpawnDuration`, `PromptDuration`
- `LeaseAcquired`, `LeaseConflict`, `PeerForwardCount`, `PeerForwardFailed`
- `ToolCallCount` by tool name
- `ApprovalRequested`, `ApprovalGranted`, `ApprovalDenied`

Quality metrics (small nightly Lambda):

- `EvalRecall` — nightly run of the eval suite.
- `NegativeFeedbackRate` — 7-day moving average of 👎 reactions.

Alarms:

| Alarm | Threshold | Severity |
|---|---|---|
| Both Fargate tasks unhealthy | 3 consecutive minutes | page |
| `MessagesFailed` rate > 20% over 10 min | rolling | page |
| Slack WebSocket disconnects > 5/hr | rolling | ticket |
| Bedrock retrieve p95 > 5s | 15 min | ticket |
| Lease conflict rate > 1/min | 15 min | ticket |
| Ingest Lambda failed | 2 consecutive runs | ticket |
| `EvalRecall` < 0.7 | nightly | ticket |
| `NegativeFeedbackRate` > 30% | 7-day | ticket |

Pages route to existing kiro-cli oncall — no new rotation.

## Capacity & cost

| Resource | Sizing |
|---|---|
| Fargate tasks | 2 × (1 vCPU, 2 GB) |
| Concurrent ACP workers per task | 10 (default) |
| DynamoDB | On-demand, all tables |
| Bedrock KB | Titan v2 embeddings, hierarchical chunking, no reranker |

Monthly cost (rough): ~$200 compute + ~$30 RAG + ~$100 LLM = ~$330.
LLM cost dominates and scales with usage.

Per-worker resources (already documented in
[crates/kiro-bot/README.md](crates/kiro-bot/README.md)): ~150 MB RAM,
~6s startup. Two tasks × 10 workers gives the same headroom as the
single-task "10-15 workers" recommendation in the README.

## Testing

| Layer | What it covers | Where it runs | Gates |
|---|---|---|---|
| Rust unit tests | Logic — command parsing, response policy, lease state machine, config validation | GH Actions on PR | PR merge |
| Eval suite (retrieval) | RAG quality | GH Actions on main against beta KB | image binary publish |
| Beta integration smoke | End-to-end (answer + dedupe + draft) | CDK pipeline post-deploy | promotion to prod |

Deliberately NOT in CI:

- E2E against Slack (rate limits + Socket Mode model — beta bot is the
  integration test environment).
- LLM-output unit tests (assertions on free text are a treadmill).

## Rollout

**Phase 0 — local dev** (week 1).
`kiro-bot start kiro-help-local` against personal Slack + personal AWS
account. Catches handshake races, IAM mistakes, Cedar bugs.

**Phase 1 — beta with kiro-cli team** (1-2 weeks).
Deploy beta stack to `551670267384`. Single private channel
`#kiro-help-beta` plus DMs from kiro-cli team. Tune alarm thresholds
against real traffic. One controlled test issue gets filed during this
phase.

Exit criteria:
- 7 consecutive days with no paging alarm.
- `EvalRecall` ≥ 0.7 stable.
- `NegativeFeedbackRate` < 25%.

**Phase 2 — internal Amazon GA** (week 4-5).
Promote to prod stack. Channel allowlist starts with 2-3 channels (main
kiro-cli channel + support channel) plus DMs. Wider rollout by adding to
the allowlist as teams ask.

Promotion = retag the beta-validated binary-carrier digest as
`kiro-bot-binaries:prod-<git-sha>` and write that digest to
`/kiro-help-bot/prod/binary-image-digest`. Same artifact, separate
KB, separate Slack app.

**Phase 3 — external launch** (post-v1, deferred).
Per-tenant Slack-app distribution; bot tokens become per-workspace; KB
stays shared (kiro-cli docs are public). The strong-pinning model
already accommodates "one tenant = one task."

## Backout

| Problem | Backout |
|---|---|
| New image is worse | Revert SSM `binary-image-digest` to previous digest, trigger CDK redeploy. ~5 min. |
| Bot misbehaving but image fine | Update config to `approval_policy = "deny"` — Q&A still works, writes blocked. |
| Bot on fire | `aws ecs update-service --desired-count 0` — bot offline in ~30s. |

Each documented as a runbook in the CDK package.

## Things we expect to get wrong

- **Approval-gate fatigue.** If users reflexively ✅ every prompt, the
  gate stops mattering. Signal: high `ApprovalGranted` with low
  `ApprovalDenied`. Mitigation: lift search tools to permanent
  auto-approve; keep gates only on writes (already done in v1).
- **Noisy retrieval on long-tail questions.** First 30 eval questions
  miss real edge cases. Signal: 👎 cluster around questions whose
  answers aren't in the KB. Mitigation: grow corpus + eval set together.
- **Flaky lease coordination.** Real-world races we didn't think of.
  Signal: `LeaseConflict > PeerForwardCount` over time. Mitigation:
  lengthen lease TTL or pre-acquire on first message.
- **Cost from chatty agent loops.** One bad prompt change → tools called
  dozens of times per message. Signal: `ToolCallCount` p95 > 8.
  Mitigation: per-prompt budget enforced in kiro-bot (12-call cap; kills
  prompt with error if exceeded). Already in v1 design.

## Open questions

These don't block the plan but want resolution during implementation:

1. **Issue-dedupe bot integration.** Once the existing issue-dedupe bot
   ([docs/superpowers/specs/2026-05-11-issue-dedupe-bot-design.md](docs/superpowers/specs/2026-05-11-issue-dedupe-bot-design.md))
   is producing reliable cluster snapshots in S3, expose a fifth
   `search_dedupe_clusters` tool. Targeting v1.5; v1 keeps the two bots
   independent.
2. **Channel allowlist mechanism.** Cedar policies in v1, but channel
   adds require a config redeploy. Future: SSM-backed allowlist that
   the bot watches for live updates.
3. **Eval set ownership.** Default: kiro-bot maintainers. Re-evaluate
   if a clearer owner emerges (kiro-cli docs team).
4. **Bedrock model selection.** Spec doesn't pin the chat model — use
   whatever kiro-cli's default is at deploy time. Revisit if cost or
   quality tells us to.

## Implementation phasing guidance

This spec is wide; the implementation plan should split it into phases
that can be reviewed and merged independently. A reasonable split:

1. **`kiro-knowledge-mcp` + agent mode + bot config dir.** Standalone
   pieces that don't touch existing kiro-bot Rust code. Verifiable
   locally against a personal AWS account.
2. **Coordinator trait + `NoopCoordinator`.** Refactor in kiro-bot core
   to introduce the seam without changing behavior. Tests pass; CLI/cron
   frontends untouched.
3. **`DynamoCoordinator` implementation.** New code, behind config flag.
   Unit tests + integration test against DynamoDB Local.
4. **Eval suite + GH Actions workflow.** Independent of the AWS bits.
5. **Ingest Lambda + CDK data + ingest stacks.** AWS infrastructure.
6. **CDK runtime stack + CodeBuild image step.** Wires everything
   together for the first beta deploy.

## References

- Existing kiro-bot crate: [crates/kiro-bot/](crates/kiro-bot/)
- kiro-bot ACP coordination: [crates/kiro-bot/src/engine/acp.rs](crates/kiro-bot/src/engine/acp.rs)
- kiro-bot conversation keying: [crates/kiro-bot/src/engine/core.rs:41](crates/kiro-bot/src/engine/core.rs:41)
- Issue-dedupe bot spec: [docs/superpowers/specs/2026-05-11-issue-dedupe-bot-design.md](docs/superpowers/specs/2026-05-11-issue-dedupe-bot-design.md)
- AGENTS.md (workspace conventions): [AGENTS.md](AGENTS.md)
