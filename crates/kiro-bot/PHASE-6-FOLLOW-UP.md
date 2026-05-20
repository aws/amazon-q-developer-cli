# Phase 6 follow-up: remaining tools + feedback wiring

The Phase 6 plan (`docs/superpowers/plans/2026-05-19-phase-6-production-hardening.md`)
covers nine tasks. Tasks 1, 3, 4, 5, 7, and 8 are merged. The two remaining
work items — the live MCP-backed tool servers (Task 2) and 👍/👎 feedback
persistence (Task 6) — both need fresh AWS-SDK or HTTP-client wiring inside
the kiro-bot binary. They are deferred here so this PR doesn't drag in the
full surface in one go. This document is the implementation guide for the
follow-up.

## Task 2 — remaining MCP-backed tools

Spec target: eight tools total. Phase 2 shipped `search_kiro_knowledge`. The
remaining seven are:

| Tool | Origin | Severity | Approval |
|---|---|---|---|
| `search_github_issues` | gh-mcp (new crate) | read | auto-approve |
| `create_github_issue`  | gh-mcp | write | reaction gate |
| `comment_on_existing`  | gh-mcp | write | reaction gate |
| `search_taskei` | builder-proxy-mcp | read | auto-approve |
| `create_taskei_task` | builder-proxy-mcp | write | reaction gate |
| `search_tcorp` | builder-proxy-mcp | read | auto-approve |
| `kiro_cli_help` | built-in | read | auto-approve |

### `gh-mcp`: new Rust crate

**Path:** `crates/kiro-github-mcp/` (mirror `kiro-knowledge-mcp`).

```toml
[dependencies]
anyhow.workspace = true
async-trait.workspace = true
clap = { workspace = true, features = ["env"] }
governor = "0.7"     # rate limiter — 30 calls/min per spec
octocrab = "0.45"
rmcp = { workspace = true, features = ["server", "transport-io"] }
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tokio = { workspace = true, features = ["full"] }
tracing.workspace = true
tracing-subscriber.workspace = true
```

**Tools, in order of implementation cost:**

1. `search_github_issues({owner, repo, query, state}) -> Vec<IssueSummary>`
   - Octocrab `issues().list().send().await` filtered by query string.
   - Drop pull requests (the issues endpoint returns both — `pull_request`
     field is non-null on PRs).
   - Skip rate limiter check on read tools (fits inside the 30 calls/min
     budget for reasonable queries).
2. `create_github_issue({owner, repo, title, body, labels})`
   - `octocrab.issues().create()` then return the new issue's URL.
   - Surface a `Result<IssueRef>` so the bot can post a Slack reply with the
     URL.
   - Acquires one rate-limit token; bails with a polite error if the bucket
     is empty.
3. `comment_on_existing({owner, repo, number, body})`
   - `octocrab.issues().create_comment()`.
   - Same rate-limit treatment as `create_github_issue`.

**PAT scopes:**

- Read: `repo:read` on `kiro-team/kiro-cli`. Stored as `kiro-bot/github-pat`
  in Secrets Manager (already provisioned for the ingest Lambda).
- Write: separate fine-grained PAT `kiro-bot/github-pat-write` with
  `issues:write` and `pull-requests:write`. Provisioned manually by oncall —
  do not commit a creation script. **Rotate every 90 days.**

The agent JSON splits the read-only and write-capable launches into two MCP
servers so the IAM separation is mirrored at the agent-config layer:

```json
"mcpServers": {
  "kiro-github-read":  {"command": "kiro-github-mcp", "args": ["--scope", "read"]},
  "kiro-github-write": {"command": "kiro-github-mcp", "args": ["--scope", "write"]}
}
```

The binary reads `KIRO_BOT_GITHUB_PAT_READ` and `KIRO_BOT_GITHUB_PAT_WRITE`
from env (set by the runtime stack from Secrets Manager).

### `builder-proxy-mcp`: thin proxy or direct invocation

The kiro-cli MCP layer already lists builder-mcp under the broader Amazon
internal MCP catalog (`mcp__builder-mcp__TaskeiCreateTask`,
`mcp__builder-mcp__TicketingReadActions`, etc.). Two options:

- **Option A:** launch builder-mcp directly from the kiro-help agent's
  `mcpServers` block. Cheapest if it's already on PATH inside the runtime
  image. Pros: no new crate. Cons: surfaces the entire builder-mcp tool
  catalog to the bot, which is broader than Phase 6 needs and risks
  agent-loop noise.
- **Option B:** thin proxy crate `crates/kiro-builder-proxy-mcp/` that
  re-exports only `search_taskei`, `create_taskei_task`, `search_tcorp`. A
  ~150 LOC `rmcp` server that forwards to a child `builder-mcp` process.
  Pros: tight surface. Cons: another binary to ship in the image.

Recommend Option B to keep the agent's tool inventory small (helps the LLM
pick the right tool, helps users reason about what the bot can do).

### `kiro_cli_help`: built-in tool

Not an MCP tool — an in-process built-in like `fs_read`. Add to
`crates/chat-cli/src/cli/chat/tools/`. Whitelist:

- `kiro-cli --help`
- `kiro-cli <subcommand> --help` where `<subcommand>` is in a hard-coded
  list (`chat`, `agent`, `mcp`, `settings`, ...).
- `kiro-cli --version`

Anything else returns `Err("only --help and --version are allowed")`. No
shell expansion, no piping, no env interpolation. Implementation: `Command::new("kiro-cli").args(allowlist).output()`.

### Image + build wiring

Add `kiro-github-mcp` and (if Option B) `kiro-builder-proxy-mcp` to:

- `runtime.Dockerfile` `COPY --from=source` line.
- `.github/workflows/kiro-bot-release.yml` `build-binaries` cargo zigbuild
  list and the `Stage artifacts` step.
- `lib/kiro-bot/docker/build.sh` (the local-fallback path).

### Approval gate (write tools)

The bot already supports an `approval_policy` config (Phase 4 left it as
`ask`). Phase 6 wires the reaction-flow in `crates/kiro-bot/src/frontend/slack/`:

1. Bot intercepts an outbound write tool call and instead posts a Slack
   message with the would-be action (e.g. *"I'd file an issue titled X with
   body Y. React 👍 within 5 min to file it, 👎 to cancel."*).
2. A reaction listener (`reaction_added`) on bot-authored messages with the
   right `chunk_id` metadata reads the user's reaction.
3. On 👍, replays the original tool call. On 👎 or 5-min timeout, drops it.

State for in-flight approvals lives in DynamoDB
`kiro-bot-pending-approvals-<stage>` (new table — add to the storage stack).
Use TTL on `expires_at` to sweep timed-out approvals.

## Task 6 — 👍/👎 feedback persistence

The `kiro-bot-feedback-<stage>` table is already provisioned. Wiring it up is
two pieces:

### 1. Per-turn chunk metadata in transcripts

Right now `engine::coordinator::Turn` is just `{role, text, ts}`. Add an
optional `chunk_ids: Vec<String>` field. The bot populates it whenever a
turn used `search_kiro_knowledge`:

```rust
pub struct Turn {
    pub role: TurnRole,
    pub text: String,
    pub ts: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunk_ids: Vec<String>,
}
```

`DynamoCoordinator::append_turn` writes the field to DDB; `NoopCoordinator`
keeps it in memory. `InMemoryClusterCoordinator` already passes the field
through unchanged because it just stores `Turn` directly.

### 2. Reaction listener

In the Slack frontend, on `reaction_added`:

```rust
async fn handle_reaction(rxn: SlackReactionAdded, coord: Arc<dyn Coordinator>) {
    if rxn.user == bot_user_id {
        return;
    }
    // Only listen to 👍 and 👎 on bot-authored messages.
    let signal = match rxn.reaction.as_str() {
        "+1" | "thumbsup" => "+1",
        "-1" | "thumbsdown" => "-1",
        _ => return,
    };
    let msg_id = format!("{}:{}", rxn.channel, rxn.message_ts);

    let conv = conversation_for_channel(&rxn.channel);
    let chunk_ids = look_up_chunk_ids_for(coord.as_ref(), &conv, &rxn.message_ts).await;

    feedback_writer.write(FeedbackRow {
        slack_msg_id: msg_id,
        reaction: signal.to_string(),
        chunk_ids,
        ts: chrono::Utc::now(),
        expires_at: 90 days from now,
    }).await;
}
```

`look_up_chunk_ids_for` walks the latest assistant turn whose `ts` matches
the reacted message's ts ± 5s and reads its `chunk_ids`. If no match, write
the row with empty `chunk_ids` — feedback without grounding info is still a
weak signal.

### 3. Smoke test

DM the bot, react 👍 to its reply, verify a row exists:

```bash
AWS_PROFILE=kiro-bot aws dynamodb scan \
  --table-name kiro-bot-feedback-<stage> \
  --max-items 5
```

### 4. NegativeFeedbackRate metric

Phase 6's nightly metrics Lambda (`alarms-stack.ts` already declares the
alarm) reads the feedback table:

```rust
async fn compute_negative_feedback_rate(client: &DynamoDbClient, table: &str) -> f64 {
    let now = chrono::Utc::now();
    let week_ago = now - chrono::Duration::days(7);
    // Scan rows with ts >= week_ago. Return |-1 rows| / |rows|.
}
```

Publish to `KiroHelpBot::NegativeFeedbackRate` with `Stage` dimension. The
`kiro-bot-<stage>-negative-feedback-high` alarm picks it up.

## Order of operations for the follow-up PR

1. Add `octocrab` to workspace deps.
2. Land `kiro-github-mcp` (read tools only first).
3. Wire those into the kiro-help agent JSON; smoke-test.
4. Add the write tools + the approval-gate flow.
5. Land `kiro-builder-proxy-mcp` (or wire builder-mcp directly).
6. Add `kiro_cli_help` built-in.
7. Wire 👍/👎 reaction listener.
8. Add the nightly metrics Lambda.
9. Beta soak — 7 days of acceptance criteria from Phase 6 plan.
10. Promote to prod via the Phase 5 RUNBOOK-promote.md path.

## See also

- Spec: `docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md` — sections
  4 (mode/tools), 6 (tools), Security, Observability.
- Plan: `docs/superpowers/plans/2026-05-19-phase-6-production-hardening.md`.
- Existing patterns:
  - `crates/kiro-knowledge-mcp/` — template for `kiro-github-mcp`.
  - `crates/kiro-help-corpus-ingest/src/github_source.rs` — already proves out
    HTTP-against-GitHub patterns; can lift `octocrab` setup from there once
    its `main.rs` lands the live wiring.
- Storage tables: `Kiro-botCDK/lib/kiro-bot/storage-stack.ts`. Add the
  pending-approvals table when that PR lands.
- Alarms: `Kiro-botCDK/lib/kiro-bot/alarms-stack.ts` — `negative-feedback-high`
  and `eval-recall-low` are already wired but in INSUFFICIENT_DATA until the
  metrics Lambda starts publishing.
