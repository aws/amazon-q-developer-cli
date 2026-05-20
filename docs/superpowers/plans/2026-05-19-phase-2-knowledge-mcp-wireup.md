# Phase 2 — kiro-knowledge-mcp Wireup (Immediate Q&A)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status:** Draft
**Date:** 2026-05-19
**Spec:** [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](../specs/2026-05-13-kiro-help-bot-design.md)
**Predecessor:** Phase 1 — [2026-05-13-kiro-knowledge-mcp.md](2026-05-13-kiro-knowledge-mcp.md) (✅ done)

## Goal

Get the kiro-help Slack bot answering kiro-cli questions with chunks pulled from a Bedrock Knowledge Base. The smallest possible end-to-end RAG loop: user types in Slack → bot calls `search_kiro_knowledge` → response cites a real doc.

## What lands at the end of Phase 2

- A Bedrock Knowledge Base in account `551670267384`, manually seeded with `docs/` + top-level `*.md` from kiro-cli.
- The deployed bot container ships with the `kiro-knowledge-mcp` binary on `PATH`.
- The `kiro_help` agent is wired to launch `kiro-knowledge-mcp` as an MCP server and has `search_kiro_knowledge` in its tool allowlist.
- The bot's help prompt instructs the agent to call `search_kiro_knowledge` before answering.
- Smoke test: in a beta Slack channel, asking "how do I log in?" returns a grounded answer citing a real doc path.

## Non-goals (explicitly deferred)

- **Automated ingest.** The KB is hand-seeded with a one-shot upload + ingestion job. Hourly Lambda comes in [Phase 3](2026-05-19-phase-3-ingest-pipeline.md).
- **Multi-task HA.** Single Fargate task. Coordinator + 2-task pinning is [Phase 4](2026-05-19-phase-4-cross-task-coordinator.md).
- **Eval suite + release automation.** Manual binary build + manual `finch` push, same as today. [Phase 5](2026-05-19-phase-5-eval-and-release-automation.md).
- **Write tools** (`create_github_issue`, `create_taskei_task`, `comment_on_existing`). Read-only Q&A only. [Phase 6](2026-05-19-phase-6-production-hardening.md).
- **Channel-scoped Cedar.** Still `allow-all`. [Phase 6](2026-05-19-phase-6-production-hardening.md).
- **Beta/prod stage split.** Phase 2 ships to Alpha only. [Phase 6](2026-05-19-phase-6-production-hardening.md).

## Source split

| Lives in `kiro-team/kiro-cli` worktree | Lives in `Kiro-botCDK` |
|---|---|
| `crates/chat-cli/src/kiro_help.json` (modify) | `lib/kiro-bot/data-stack.ts` (new) |
| `crates/chat-cli/src/help_prompt.md` (modify) | `lib/kiro-bot/stack.ts` (modify) |
| `crates/kiro-bot/kiro-help/config.toml` (verify only) | `lib/kiro-bot/docker/Dockerfile` (modify) |
| | `lib/kiro-bot/docker/kiro-knowledge-mcp` (new — binary copied in pre-build) |
| | `lib/app.ts` (modify — wire data stack) |
| | `lib/kiro-bot/docker/seed-corpus.sh` (new — manual seed helper) |

The bot's runtime config (`crates/kiro-bot/kiro-help/config.toml`) does not change in this phase. MCP servers are configured per-agent in the kiro-cli agent JSON, not in the bot config.

---

## Task 1: CDK — provision the Bedrock Knowledge Base (data stack)

**Repo:** `Kiro-botCDK`
**Spec section:** "Storage" + "Knowledge pipeline"

Provisions an S3 bucket and a Bedrock KB. No ingest Lambda yet — corpus is uploaded manually in Task 5.

**Files:**
- Create: `lib/kiro-bot/data-stack.ts`
- Modify: `lib/app.ts`

- [x] **Step 1: Create the data stack class**

Write `lib/kiro-bot/data-stack.ts` with a new `KiroBotDataStack` extending `DeploymentStack`. It owns:

- `s3.Bucket` named `kiro-help-corpus-<account>-<region>`. `BlockPublicAccess.BLOCK_ALL`, `RemovalPolicy.RETAIN`, versioning enabled.
- `bedrock.CfnKnowledgeBase` (use `aws-cdk-lib/aws-bedrock` L1 — L2 KB constructs are still alpha at the time of writing).
  - Embeddings model: `amazon.titan-embed-text-v2:0`.
  - Vector store: opensearch-serverless (managed by CDK alpha module `@cdklabs/generative-ai-cdk-constructs`) **or** the L1 form with manual OSS collection — pick whichever is available in the version of `aws-cdk-lib` already pinned in this repo.
  - IAM service role (`AWS::IAM::Role`) trusted by `bedrock.amazonaws.com`, with `s3:GetObject`/`ListBucket` on the corpus bucket.
- `bedrock.CfnDataSource` of type `S3` pointing at the corpus bucket. Chunking: hierarchical default.
- `CfnOutput` for `KbId` and `CorpusBucketName`.
- Optional `ssm.StringParameter` writing `/kiro-help-bot/alpha/kb-id` so the runtime stack can read it without cross-stack refs.

> **Note:** if `@cdklabs/generative-ai-cdk-constructs` is not already a dependency, prefer the L1 `CfnKnowledgeBase` route to avoid pulling in an alpha module just for one resource. Check `package.json` first.

- [x] **Step 2: Wire the data stack into the pipeline**

Edit `lib/app.ts`. After the existing `KiroBotStack` (Alpha runtime stack), add:

```ts
const alphaDataStack = new KiroBotDataStack(app, 'KiroBotDataStack-Alpha', {
  env: pipeline.deploymentEnvironmentFor(applicationAccount, region),
  softwareType: SoftwareType.INFRASTRUCTURE,
});
alphaTarget.addStacks(alphaDataStack);
```

Order matters: data stack provisioned in the same deployment group as runtime, but the runtime stack reads the KB ID from SSM at synth time (or via a stack output exported and imported). Use SSM for loose coupling.

- [ ] **Step 3: Synth and deploy** (requires AWS creds; run on your dev box)

```bash
AWS_PROFILE=kiro-bot npx cdk synth KiroBotDataStack-Alpha
AWS_PROFILE=kiro-bot npx cdk deploy KiroBotDataStack-Alpha
```

Expected: KB created, ingestion job state = `READY` (no documents yet).

- [x] **Step 4: Commit**

```bash
git add lib/kiro-bot/data-stack.ts lib/app.ts package.json package-lock.json
git commit -m "feat(kiro-bot): add Bedrock KB data stack"
```

---

## Task 2: CDK — build kiro-knowledge-mcp into the runtime image

**Repo:** `Kiro-botCDK`

The current image bakes in pre-built `kiro-bot` and `kiro-cli` binaries dropped into `lib/kiro-bot/docker/` before `finch build`. Add `kiro-knowledge-mcp` to that set.

**Files:**
- Modify: `lib/kiro-bot/docker/Dockerfile`
- Modify: `lib/kiro-bot/docker/build.sh` (only the human-readable instructions if any)

- [x] **Step 1: Pre-build the kiro-knowledge-mcp binary in the kiro-cli worktree**

In `/Volumes/workplace/worktrees/kiro-cli/kiro-bot`:

```bash
cargo build --release --target aarch64-unknown-linux-gnu \
    -p kiro-knowledge-mcp
```

(Match the existing `kiro-bot` / `kiro-cli` build target — currently ARM64 per `runtimePlatform: ARM64` in `stack.ts`. The repo already produces ARM64 binaries; reuse that toolchain.)

Copy the resulting binary into the CDK repo:

```bash
cp target/aarch64-unknown-linux-gnu/release/kiro-knowledge-mcp \
   /Volumes/workplace/kiro-bot/src/Kiro-botCDK/lib/kiro-bot/docker/kiro-knowledge-mcp
```

- [x] **Step 2: Update the Dockerfile**

Edit `lib/kiro-bot/docker/Dockerfile`. Add the `COPY` line:

```dockerfile
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY kiro-bot              /usr/local/bin/
COPY kiro-cli              /usr/local/bin/
COPY kiro-knowledge-mcp    /usr/local/bin/
COPY kiro-help/            /etc/kiro-help/
COPY entrypoint.sh         /entrypoint.sh
RUN chmod +x /entrypoint.sh /usr/local/bin/kiro-bot /usr/local/bin/kiro-cli /usr/local/bin/kiro-knowledge-mcp
ENTRYPOINT ["/entrypoint.sh"]
```

- [ ] **Step 3: Build and push image locally** (requires `finch` + AWS creds)

```bash
cd lib/kiro-bot/docker && ./build.sh
# follow the printed `finch tag` + `finch push` commands
```

- [x] **Step 4: Commit**

```bash
git add lib/kiro-bot/docker/Dockerfile
# kiro-knowledge-mcp binary itself is gitignored — it's a build artifact
git commit -m "feat(kiro-bot): include kiro-knowledge-mcp binary in container image"
```

(If the repo currently *commits* the `kiro-bot` and `kiro-cli` binaries directly under `docker/` — check `git ls-files lib/kiro-bot/docker/` — then `kiro-knowledge-mcp` follows the same rule. Match whatever pattern is in place; do not introduce a divergent .gitignore policy for this one binary.)

---

## Task 3: CDK — pass `KIRO_KNOWLEDGE_KB_ID` env var + IAM into the runtime task

**Repo:** `Kiro-botCDK`

**Files:**
- Modify: `lib/kiro-bot/stack.ts`

- [x] **Step 1: Read the KB ID from SSM at synth time**

Edit `lib/kiro-bot/stack.ts`. At the top of the constructor, after `new ec2.Vpc(...)`:

```ts
const kbId = ssm.StringParameter.valueFromLookup(this, '/kiro-help-bot/alpha/kb-id');
```

(Use `valueFromLookup` rather than `valueForStringParameter` so the value is resolved at synth time — the env var must be a literal string in the task definition.)

- [x] **Step 2: Add the env var to the container**

In the `taskDef.addContainer('bot', { ... })` call, extend `environment`:

```ts
environment: {
  BOT_MEMBER_ID: props.botMemberId,
  KIRO_KNOWLEDGE_KB_ID: kbId,
  AWS_REGION: this.region,
},
```

- [x] **Step 3: Grant `bedrock:Retrieve` to the task role**

After the existing `taskDef.executionRole!.addToPrincipalPolicy(...)` block (Secrets Manager), add a statement on the **task role** (not execution role — task role is for the running container, execution role is for ECS pulling secrets/images):

```ts
taskDef.taskRole.addToPrincipalPolicy(
  new iam.PolicyStatement({
    actions: ['bedrock:Retrieve'],
    resources: [
      `arn:aws:bedrock:${this.region}:${this.account}:knowledge-base/${kbId}`,
    ],
  }),
);
```

- [ ] **Step 4: Synth + deploy** (requires AWS creds)

```bash
AWS_PROFILE=kiro-bot npx cdk diff   KiroBotStack-Alpha
AWS_PROFILE=kiro-bot npx cdk deploy KiroBotStack-Alpha
```

Expected: task definition revision bumped, env var visible in console, IAM statement attached.

- [x] **Step 5: Commit**

```bash
git add lib/kiro-bot/stack.ts
git commit -m "feat(kiro-bot): pass KB ID + grant bedrock:Retrieve to task role"
```

---

## Task 4: kiro-cli — wire `search_kiro_knowledge` into the `kiro_help` agent

**Repo:** `kiro-team/kiro-cli` (the `feature/kiro-bot` worktree at `/Volumes/workplace/worktrees/kiro-cli/kiro-bot`)

**Files:**
- Modify: `crates/chat-cli/src/kiro_help.json`
- Modify: `crates/chat-cli/src/help_prompt.md`

> **Design choice (deliberate, revisited in Phase 6):** Phase 2 extends the existing built-in `kiro_help` agent rather than creating a new `kiro-help` agent specifically for the bot. This is the smallest-touch path. Phase 6 splits them so the in-CLI help mode and bot help mode can diverge (different prompts, different tools).

- [x] **Step 1: Add the MCP server entry**

Edit `crates/chat-cli/src/kiro_help.json` (currently 13 lines). Add a `mcpServers` block and an `allowedTools` entry:

```json
{
  "name": "kiro_help",
  "description": "Help agent that answers questions about Kiro CLI features using documentation",
  "mcpServers": {
    "kiro-knowledge": {
      "command": "kiro-knowledge-mcp",
      "args": [],
      "env": {}
    }
  },
  "tools": [
    "introspect",
    "fs_read",
    "session",
    "fs_write",
    "@kiro-knowledge/search_kiro_knowledge"
  ],
  "allowedTools": [
    "@kiro-knowledge/search_kiro_knowledge"
  ],
  "includeMcpJson": false,
  "welcomeMessage": "..."
}
```

Notes:
- `command = "kiro-knowledge-mcp"` relies on the binary being on `PATH` (Task 2 places it at `/usr/local/bin/kiro-knowledge-mcp`).
- `KIRO_KNOWLEDGE_KB_ID` and `AWS_REGION` are inherited from the container env (Task 3) — no need to forward explicitly via `env`.
- `allowedTools` auto-approves the tool, matching spec sec 6 ("auto" approval for read tools).
- The `@kiro-knowledge/` prefix is the MCP-namespaced tool form. Verify against `crates/chat-cli/src/cli/mcp.rs` and the agent loader if a different sigil is in use; the spec just says the tool is named `search_kiro_knowledge`.

- [x] **Step 2: Update the help prompt**

Edit `crates/chat-cli/src/help_prompt.md`. Add (or strengthen, if a similar instruction is already there) language like:

> Before answering questions about kiro-cli behavior, errors, or how-to questions, call `search_kiro_knowledge` with a focused query derived from the user's message. Cite the returned chunks by source path (e.g., "from `docs/auth.md`...") in your answer. If `search_kiro_knowledge` returns "No relevant results found." fall back to your general knowledge but explicitly tell the user the answer is not from the canonical docs.

Keep this addition tight — the existing prompt covers in-CLI semantics that should survive.

- [x] **Step 3: Validate the agent JSON loads**

```bash
cargo build -p chat-cli
cargo test  -p chat-cli --lib agent::
```

Expected: clean build, agent validator does not reject the new fields. The `kiro_help.json` is loaded via `include_str!` in `crates/chat-cli/src/cli/agent/mod.rs:1228` — a parse failure at startup will be caught immediately.

- [x] **Step 4: Commit**

```bash
git add crates/chat-cli/src/kiro_help.json crates/chat-cli/src/help_prompt.md
git commit -m "feat(chat-cli): wire search_kiro_knowledge MCP into kiro_help agent"
```

---

## Task 5: Manually seed the knowledge base

**Repo:** `Kiro-botCDK` (helper script lives here)

**Files:**
- Create: `lib/kiro-bot/docker/seed-corpus.sh`
- Create: `lib/kiro-bot/docker/SEED.md` (one-pager)

This is a one-shot operation, not part of any deploy. Re-run when the corpus drifts. Replaced wholesale by the Phase 3 ingest Lambda.

- [x] **Step 1: Write `seed-corpus.sh`**

```bash
#!/bin/bash
# Manually seed the corpus bucket with docs/ + top-level *.md from kiro-cli.
# Then trigger a one-off Bedrock KB ingestion job.
set -euo pipefail

KIRO_CLI_DIR="${KIRO_CLI_DIR:-/Volumes/workplace/worktrees/kiro-cli/kiro-bot}"
PROFILE="${AWS_PROFILE:-kiro-bot}"
REGION="${AWS_REGION:-us-east-1}"
BUCKET="$(aws --profile "$PROFILE" --region "$REGION" \
    ssm get-parameter --name /kiro-help-bot/alpha/corpus-bucket \
    --query Parameter.Value --output text)"
KB_ID="$(aws --profile "$PROFILE" --region "$REGION" \
    ssm get-parameter --name /kiro-help-bot/alpha/kb-id \
    --query Parameter.Value --output text)"
DS_ID="$(aws --profile "$PROFILE" --region "$REGION" \
    bedrock-agent list-data-sources --knowledge-base-id "$KB_ID" \
    --query 'dataSourceSummaries[0].dataSourceId' --output text)"

# Upload docs/ and top-level *.md
aws --profile "$PROFILE" --region "$REGION" s3 sync \
    "$KIRO_CLI_DIR/docs/"   "s3://$BUCKET/docs/"   --delete
aws --profile "$PROFILE" --region "$REGION" s3 cp \
    "$KIRO_CLI_DIR"  "s3://$BUCKET/root/"  --recursive \
    --exclude "*" --include "*.md"

# Trigger ingestion
JOB_ID="$(aws --profile "$PROFILE" --region "$REGION" \
    bedrock-agent start-ingestion-job \
    --knowledge-base-id "$KB_ID" \
    --data-source-id "$DS_ID" \
    --query 'ingestionJob.ingestionJobId' --output text)"
echo "Ingestion job: $JOB_ID"
echo "Track with:  aws bedrock-agent get-ingestion-job ..."
```

- [ ] **Step 2: Run it once** (requires AWS creds; run after data stack deploy)

```bash
cd lib/kiro-bot/docker && ./seed-corpus.sh
```

Watch the job. When it finishes, verify chunk count via `bedrock-agent get-knowledge-base` or by running `kiro-knowledge-mcp` against the KB locally:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"manual","version":"0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_kiro_knowledge","arguments":{"query":"how do I log in"}}}' | \
  KIRO_KNOWLEDGE_KB_ID=$KB_ID AWS_PROFILE=kiro-bot AWS_REGION=us-east-1 \
  /Volumes/workplace/kiro-bot/src/Kiro-botCDK/lib/kiro-bot/docker/kiro-knowledge-mcp
```

Expected: second response contains chunks from real kiro-cli docs.

- [x] **Step 3: Commit the helper**

```bash
git add lib/kiro-bot/docker/seed-corpus.sh lib/kiro-bot/docker/SEED.md
git commit -m "chore(kiro-bot): add manual corpus seed script"
```

---

## Task 6: End-to-end smoke test

**Where:** Slack workspace where the Alpha bot is installed.

- [ ] **Step 1: Ensure both stacks deployed and image is current**

```bash
# Confirm latest image is in ECR and ECS picked it up
aws --profile kiro-bot ecs update-service \
    --cluster <cluster> --service <service> --force-new-deployment
```

Wait for the new task to reach `RUNNING`.

- [ ] **Step 2: Tail container logs**

```bash
aws --profile kiro-bot logs tail /ecs/kiro-bot --follow
```

Look for `kiro-knowledge-mcp` startup line (it logs to stderr; the bot picks it up when the agent first runs).

- [ ] **Step 3: DM the bot**

Ask: `how do I log in to kiro-cli?`

Expected:
- Bot responds in-thread / inline (per `[[response_policies]]`).
- Response references a real doc path (e.g., "`docs/auth.md` says...").
- Logs show a `tools/call` for `search_kiro_knowledge`.

If the bot answers but cites no doc path, the agent didn't call the tool — debug:
- Did the agent JSON load? Search logs for `kiro_help.json` parse errors.
- Is the binary on PATH inside the container? `aws ecs execute-command` into the task: `which kiro-knowledge-mcp` should print `/usr/local/bin/kiro-knowledge-mcp`.
- Is the env var set? `env | grep KIRO_KNOWLEDGE_KB_ID` should print the KB ID.
- Does the IAM call work? `aws --region us-east-1 bedrock-agent-runtime retrieve --knowledge-base-id $KIRO_KNOWLEDGE_KB_ID --retrieval-query text="login"`.

- [ ] **Step 4: Document outcome**

If smoke test passes, Phase 2 is done. Open a tracking ticket for any rough edges discovered (slow first response, wrong citations, etc.) and feed them into Phase 3 / 6.

---

## Phase 2 acceptance

- KB exists, has chunks from kiro-cli docs.
- Bot container has `kiro-knowledge-mcp` on PATH.
- Bot's task role can call `bedrock:Retrieve` on the KB.
- `kiro_help` agent has `search_kiro_knowledge` in its tool list and the prompt encourages its use.
- A real Slack message gets a real grounded answer.

## Risks / known issues to watch

- **First-response latency.** Cold-start the ACP worker (~6s) plus a Bedrock retrieve (~1-3s) plus LLM time. If users complain, send a `:hourglass:` placeholder reaction within 1s while work is in flight (already supported by kiro-bot).
- **Agent MCP wireup may differ from spec.** The spec assumes namespaced tool names (`search_kiro_knowledge`); kiro-cli may surface them as `@kiro-knowledge/search_kiro_knowledge` or similar. Verify against the agent loader; tweak `tools` and `allowedTools` accordingly.
- **`include_str!` means agent changes require a new binary.** Editing `kiro_help.json` requires a kiro-cli rebuild and a new container image push. Acceptable for v1 but accumulates friction — Phase 5 will automate this via GH Actions.
- **No source filter.** `kiro-knowledge-mcp` ignores `source_filter` (Phase 1.5 follow-up). All chunks come from the same blended corpus. Acceptable while the KB is small.

## Spec sections this phase covers

- Sec 1 — kiro-knowledge-mcp (binary integration into runtime; the crate itself shipped in Phase 1).
- Sec 4 — partial (kiro-help agent mode, but riding on existing built-in rather than a separate mode).
- Sec 5 — partial (config dir already exists; MCP wireup is at the agent layer in this phase).
- Sec 6 — `search_kiro_knowledge` only.
- "Knowledge pipeline" — manual seed of `docs/` + top-level `*.md`. Issues + releases + automation in Phase 3.

## Hand-off to Phase 3

Phase 3 replaces the manual seed with an hourly Lambda. Concrete artefacts Phase 2 leaves in place that Phase 3 reuses:

- The corpus S3 bucket (Phase 3 Lambda writes here).
- The Bedrock KB (Phase 3 Lambda calls `StartIngestionJob` against it).
- The data stack (Phase 3 extends it with the ingest Lambda + EventBridge rule).
