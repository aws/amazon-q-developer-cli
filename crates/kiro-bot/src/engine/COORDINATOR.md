# Coordinator runbook

`engine::coordinator` is the seam Phase 4 introduces for cross-task arbitration.
This file is the implementation guide for the DynamoDB-backed variant. The
trait surface (and two in-memory implementations) is already merged.

## What's here today

- `Coordinator` trait — the surface `engine::core` consumes.
- `Turn`, `LeaseOutcome`, `ForwardEvent` data types.
- `NoopCoordinator` — single-task in-memory; default for CLI/cron.
- `InMemoryClusterCoordinator` — multi-task in-memory simulator. Two
  `sibling()`s share lease/transcript state. Used by HA tests so the engine
  refactor can assert lease arbitration without standing up DynamoDB.

## What still needs to land for v1 multi-task production

### 1. `aws-sdk-dynamodb` workspace dep

```toml
# crates/kiro-bot/Cargo.toml
[dependencies]
aws-config.workspace = true
aws-sdk-dynamodb.workspace = true
```

…and add `aws-sdk-dynamodb = "1.x"` to the root workspace `[workspace.dependencies]`
table.

### 2. `DynamoCoordinator` struct

```rust
pub struct DynamoCoordinator {
    client: aws_sdk_dynamodb::Client,
    leases_table: String,
    transcripts_table: String,
    dedup_table: String,
    own_task_arn: String,
    peer_discovery: PeerDiscovery,
    lease_ttl: chrono::Duration,
    transcript_limit: usize,
}
```

Five tables live in the storage stack — leases, transcripts, dedup, feedback,
all PAY_PER_REQUEST with `expires_at` TTL.

### 3. Method semantics

- **`dedupe_event`** — `PutItem` on `event-dedup` with
  `ConditionExpression = "attribute_not_exists(slack_event_id)"`.
  Returns `true` on success, `false` on `ConditionalCheckFailedException`.
  TTL value: `now + 5 minutes`.
- **`try_acquire`** — `PutItem` on `leases` with
  `ConditionExpression = "attribute_not_exists(owner_task_arn) OR
                          lease_expires_at < :now"`.
  Item: `{conversation_id, owner_task_arn = self, lease_expires_at = now + ttl}`.
  On success → `Acquired`. On conflict → `GetItem` to read `owner_task_arn`,
  return `Held { peer }`. On any other error → `Unavailable` (and log).
- **`renew`** — `UpdateItem` with
  `ConditionExpression = "owner_task_arn = :me"`; bump `lease_expires_at`.
- **`release`** — `DeleteItem` with the same condition.
- **`append_turn`** — atomic `UpdateItem ADD` on a counter item to mint
  `turn_seq`, then `PutItem` with PK `conversation_id`, SK `turn_seq`. Sets
  `expires_at = now + 30 days`.
- **`load_history`** — `Query` with `ScanIndexForward = false, Limit = N`,
  reverse on the client.

### 4. Peer discovery

`PeerDiscovery::CloudMap { namespace }` resolves `<service>.<namespace>` SRV
records via `hickory-resolver` (already in workspace? — verify; if not, add).
For each A record, drop the one that matches `own_task_arn`'s task private
IP. Cache resolutions for ≤ 60 s (Cloud Map's SRV TTL).

### 5. Dispatch server

`engine::dispatch_server`:

```rust
// Bind and serve are separate so the caller can open the socket before
// starting Slack ingress: a peer forward that races startup is then queued
// by the kernel rather than refused.
pub async fn bind_dispatch_server(port: u16, dispatcher: Arc<dyn Dispatcher>)
    -> anyhow::Result<BoundDispatchServer> {
    let app = axum::Router::new().route("/dispatch", post(handle_dispatch));
    Ok(BoundDispatchServer { listener: TcpListener::bind(addr).await?, app })
}

async fn handle_dispatch(
    State(dispatcher): State<Arc<dyn Dispatcher>>,
    Json(event): Json<serde_json::Value>,
) -> StatusCode {
    dispatcher.process_as_if_from_slack(event).await;
    StatusCode::OK
}
```

A peer that's just received a `forward` POST treats the event as if it had
arrived natively from Slack — the dedup table prevents double-processing.

### 6. Engine integration

In `engine::core::dispatch`, before the worker dispatch:

```rust
if !core.coordinator.dedupe_event(&msg.event_id).await {
    debug!("duplicate event, dropping");
    return;
}
match core.coordinator.try_acquire(&conv_id).await {
    LeaseOutcome::Acquired => { /* fall through */ }
    LeaseOutcome::Held { peer } => {
        let _ = core.coordinator.forward(&peer, ForwardEvent { /* ... */ }).await;
        return;
    }
    LeaseOutcome::Unavailable => {
        warn!("coordinator unavailable, falling back to local dispatch");
        // fall through
    }
}
```

After the worker completes, `append_turn` for both the user and assistant
turns and `renew` the lease. On graceful shutdown, walk all owned leases and
`release` them.

### 7. Config

```toml
# kiro-help/config.toml
[coordinator]
type = "dynamodb"
leases_table = "kiro-bot-leases"
transcripts_table = "kiro-bot-transcripts"
dedup_table = "kiro-bot-event-dedup"
peer_discovery = "cloud_map:kiro-bot.local"
peer_port = 8080
lease_ttl_secs = 300
transcript_history_limit = 20
```

Container env (set by `KiroBotStack`) carries each of these:

- `KIRO_BOT_LEASES_TABLE`
- `KIRO_BOT_TRANSCRIPTS_TABLE`
- `KIRO_BOT_DEDUP_TABLE`
- `KIRO_BOT_FEEDBACK_TABLE`
- `KIRO_BOT_PEER_NAMESPACE`
- `KIRO_BOT_DISPATCH_PORT`

…so the bot can build the coordinator at startup without compile-time wiring.

## HA verification (Phase 4 Task 5)

Once `DynamoCoordinator` is live in beta:

1. Send a Slack message; tail logs for both tasks. Exactly one logs
   "lease acquired" and replies; the other logs "duplicate" or
   "lease held by peer, forwarded".
2. Find the owner with `aws dynamodb get-item --table-name kiro-bot-leases
   --key '{"conversation_id":{"S":"<convo>"}}'` and stop that task. Within
   the lease TTL (~5 min), the surviving task should pick up the next message
   in the same thread; the cold path replays history from `kiro-bot-transcripts`.
3. `cdk deploy KiroBotStack-Alpha` mid-conversation. Confirm the next
   message in that thread continues to work after both tasks rotate.

## See also

- Spec: [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](../../../../docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md) — sec 3, "Data flow"
- Plan: [docs/superpowers/plans/2026-05-19-phase-4-cross-task-coordinator.md](../../../../docs/superpowers/plans/2026-05-19-phase-4-cross-task-coordinator.md)
- Storage stack: `Kiro-botCDK/lib/kiro-bot/storage-stack.ts`
- Runtime stack DDB grants + Cloud Map: `Kiro-botCDK/lib/kiro-bot/stack.ts`
