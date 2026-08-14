//! Production [`Coordinator`] backed by Amazon DynamoDB.
//!
//! Phase 4 spec. The four DDB tables (leases, transcripts, dedup, feedback)
//! are owned by `KiroBotStorageStack` in the CDK repo; this module just
//! talks to them.
//!
//! Semantics:
//!
//! - `dedupe_event` — conditional `PutItem` on the dedup table with
//!   `attribute_not_exists(slack_event_id)`. Returns distinct accepted, duplicate, and unavailable
//!   outcomes; accepted rows carry a generation token for conditional cleanup.
//! - `acquire_lease` — conditional `PutItem` on the leases table allowing the write iff
//!   `attribute_not_exists(owner_task_arn)` OR the existing lease is past `lease_expires_at`. On
//!   conflict we `GetItem` to return `LeaseAcquisition::Held { peer }`. Any other error →
//!   `Unavailable` (and log).
//! - `renew` — conditional `UpdateItem` requiring `owner_task_arn = :me`.
//! - `release` — conditional `DeleteItem` with the same condition.
//! - `append_turn` — atomic `UpdateItem ADD` on a `<conv_id>:counter` row to mint the next
//!   `turn_seq`, then `PutItem` of the transcript row at that key. Sets `expires_at` 30 days out.
//! - `load_history` — `Query` with `ScanIndexForward = false, Limit = N`, reverse on the client.
//! - `forward` — POST `/dispatch` to the peer over plain HTTP.
//!
//! Tests use `aws-smithy-mocks` to assert request/response shape without a
//! live DDB.

use std::collections::HashMap;

use anyhow::Context;
use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::error::SdkError;
use aws_sdk_dynamodb::operation::delete_item::DeleteItemError;
use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_dynamodb::operation::update_item::UpdateItemError;
use aws_sdk_dynamodb::types::AttributeValue;
use chrono::{
    DateTime,
    Duration,
    Utc,
};
use tracing::warn;

use crate::engine::coordinator::{
    Coordinator,
    DedupeOutcome,
    DedupeToken,
    ForwardEvent,
    LeaseAcquisition,
    LeaseToken,
    RateLimitOutcome,
    Turn,
    TurnRole,
};

const DEFAULT_LEASE_TTL_SECS: i64 = 300;
const DEFAULT_TRANSCRIPT_RETENTION_DAYS: i64 = 30;
const DEFAULT_DEDUP_RETENTION_SECS: i64 = 24 * 60 * 60;
const AMBIGUOUS_DEDUP_READ_ATTEMPTS: usize = 3;
const AMBIGUOUS_DEDUP_CLEANUP_ATTEMPTS: usize = 3;
const AMBIGUOUS_DEDUP_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(25);

fn rate_window_bounds(now: i64, window: std::time::Duration) -> anyhow::Result<(i64, i64, i64)> {
    let window_secs = i64::try_from(window.as_secs()).context("rate-limit window is too large")?;
    anyhow::ensure!(window_secs > 0, "rate-limit window must be greater than zero");
    let window_start = now
        .checked_sub(now.rem_euclid(window_secs))
        .context("rate-limit window start overflowed")?;
    let window_end = window_start
        .checked_add(window_secs)
        .context("rate-limit window end overflowed")?;
    let expires_at = window_end
        .checked_add(crate::engine::rate_limit::RECORD_TTL_GRACE_SECS)
        .context("rate-limit record expiry overflowed")?;
    Ok((window_start, window_end, expires_at))
}

/// Column names — keep in sync with `KiroBotStorageStack` in Kiro-botCDK.
mod col {
    pub const CONVERSATION_ID: &str = "conversation_id";
    pub const OWNER_TASK_ARN: &str = "owner_task_arn";
    pub const LEASE_TOKEN: &str = "lease_token";
    pub const LEASE_EXPIRES_AT: &str = "lease_expires_at";
    pub const SLACK_EVENT_ID: &str = "slack_event_id";
    pub const DEDUPE_TOKEN: &str = "dedupe_token";
    pub const PROCESSED_AT: &str = "processed_at";
    pub const TURN_SEQ: &str = "turn_seq";
    pub const ROLE: &str = "role";
    pub const TEXT: &str = "text";
    pub const TS: &str = "ts";
    pub const EXPIRES_AT: &str = "expires_at";
    pub const CHUNK_IDS: &str = "chunk_ids";
    pub const SLACK_MSG_TS: &str = "slack_msg_ts";
    pub const RATE_COUNT: &str = "rate_count";
}

/// One DDB-backed coordinator. Cheap to clone (`Client` is `Arc`-internal).
#[derive(Clone)]
pub struct DynamoCoordinator {
    client: Client,
    pub leases_table: String,
    pub transcripts_table: String,
    pub dedup_table: String,
    /// Optional — when `None`, `register_approval` / `lookup_approval_owner`
    /// are no-ops. Set on Fargate when `KIRO_BOT_APPROVALS_TABLE` is in the
    /// env; absent in local dev so the bot still works without provisioning
    /// the table.
    pub approvals_table: Option<String>,
    pub own_task_arn: String,
    /// How long a lease is valid. Renewed on each successful prompt turn.
    pub lease_ttl: Duration,
    /// HTTP client used by `forward` — held here so tests can swap the base
    /// URL in via `with_peer_base_url`.
    http: reqwest::Client,
    peer_base_url: Option<String>,
}

impl DynamoCoordinator {
    pub fn new(
        client: Client,
        leases_table: impl Into<String>,
        transcripts_table: impl Into<String>,
        dedup_table: impl Into<String>,
        own_task_arn: impl Into<String>,
    ) -> Self {
        Self {
            client,
            leases_table: leases_table.into(),
            transcripts_table: transcripts_table.into(),
            dedup_table: dedup_table.into(),
            approvals_table: None,
            own_task_arn: own_task_arn.into(),
            lease_ttl: Duration::seconds(DEFAULT_LEASE_TTL_SECS),
            http: reqwest::Client::new(),
            peer_base_url: None,
        }
    }

    pub fn with_lease_ttl(mut self, ttl: Duration) -> Self {
        self.lease_ttl = ttl;
        self
    }

    /// Enable approval-row routing. Without this the two approval methods
    /// log+ignore. The CDK injects the table name via `KIRO_BOT_APPROVALS_TABLE`.
    pub fn with_approvals_table(mut self, table: impl Into<String>) -> Self {
        self.approvals_table = Some(table.into());
        self
    }

    /// For tests: override the dispatch base URL so `forward` hits a
    /// wiremock-style server instead of the real peer namespace.
    pub fn with_peer_base_url(mut self, url: impl Into<String>) -> Self {
        self.peer_base_url = Some(url.into());
        self
    }

    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn n(value: i64) -> AttributeValue {
        AttributeValue::N(value.to_string())
    }

    fn s(value: impl Into<String>) -> AttributeValue {
        AttributeValue::S(value.into())
    }

    fn peer_url(&self, peer: &str) -> String {
        match &self.peer_base_url {
            Some(base) => format!("{base}/dispatch"),
            None => format!("http://{peer}/dispatch"),
        }
    }
}

#[async_trait::async_trait]
impl Coordinator for DynamoCoordinator {
    async fn dedupe_event_outcome(&self, slack_event_id: &str) -> DedupeOutcome {
        let now = self.now().timestamp();
        let expires = now + DEFAULT_DEDUP_RETENTION_SECS;
        let token = DedupeToken::new(&self.own_task_arn);
        let result = self
            .client
            .put_item()
            .table_name(&self.dedup_table)
            .item(col::SLACK_EVENT_ID, Self::s(slack_event_id))
            .item(col::DEDUPE_TOKEN, Self::s(token.as_str()))
            .item(col::PROCESSED_AT, Self::n(now))
            .item(col::EXPIRES_AT, Self::n(expires))
            .condition_expression(format!("attribute_not_exists({})", col::SLACK_EVENT_ID))
            .send()
            .await;
        match result {
            Ok(_) => DedupeOutcome::Accepted { token },
            Err(e) if is_conditional_check_failed_put(&e) => DedupeOutcome::Duplicate,
            Err(e) => {
                warn!(?e, "dedupe_event PutItem response was ambiguous");
                self.resolve_ambiguous_dedup(slack_event_id, token).await
            },
        }
    }

    async fn release_dedup(&self, slack_event_id: &str, token: &DedupeToken) -> anyhow::Result<()> {
        let result = self
            .client
            .delete_item()
            .table_name(&self.dedup_table)
            .key(col::SLACK_EVENT_ID, Self::s(slack_event_id))
            .condition_expression(format!("{} = :token", col::DEDUPE_TOKEN))
            .expression_attribute_values(":token", Self::s(token.as_str()))
            .send()
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(error) if is_conditional_check_failed_delete(&error) => Ok(()),
            Err(error) => Err(anyhow::Error::new(error).context("release_dedup DeleteItem")),
        }
    }

    async fn acquire_lease(&self, conversation_id: &str) -> LeaseAcquisition {
        let now = self.now().timestamp_millis();
        let expires = now + self.lease_ttl.num_milliseconds();
        let token = LeaseToken::new(&self.own_task_arn);
        let result = self
            .client
            .put_item()
            .table_name(&self.leases_table)
            .item(col::CONVERSATION_ID, Self::s(conversation_id))
            .item(col::OWNER_TASK_ARN, Self::s(&self.own_task_arn))
            .item(col::LEASE_TOKEN, Self::s(token.as_str()))
            .item(col::LEASE_EXPIRES_AT, Self::n(expires))
            // expires_at TTL — DDB sweeps the row a while after the lease
            // window. Set it well past lease_ttl so a paused task can still
            // see the lease history.
            .item(col::EXPIRES_AT, Self::n((expires / 1000) + 24 * 3600))
            .condition_expression(format!(
                "attribute_not_exists({owner}) OR {expires_col} < :now",
                owner = col::OWNER_TASK_ARN,
                expires_col = col::LEASE_EXPIRES_AT,
            ))
            .expression_attribute_values(":now", Self::n(now))
            .send()
            .await;

        match result {
            Ok(_) => LeaseAcquisition::Acquired { token },
            Err(e) if is_conditional_check_failed_put(&e) => {
                // Someone else has it (or held it within the TTL) — read who.
                match self.read_lease(conversation_id).await {
                    Ok(Some(existing)) if existing.owner != self.own_task_arn => {
                        LeaseAcquisition::Held { peer: existing.owner }
                    },
                    Ok(Some(existing)) => match self
                        .reacquire_self_owned(conversation_id, existing.token.as_ref())
                        .await
                    {
                        Ok(Some(token)) => LeaseAcquisition::Acquired { token },
                        Ok(None) => LeaseAcquisition::Unavailable,
                        Err(reacquire_err) => {
                            warn!(?reacquire_err, "self-owned lease reacquisition failed");
                            LeaseAcquisition::Unavailable
                        },
                    },
                    Ok(None) => LeaseAcquisition::Unavailable, // raced — no owner now
                    Err(read_err) => {
                        warn!(?read_err, "lease read after conflict failed");
                        LeaseAcquisition::Unavailable
                    },
                }
            },
            Err(e) => {
                warn!(?e, "acquire_lease PutItem failed");
                LeaseAcquisition::Unavailable
            },
        }
    }

    async fn renew(&self, conversation_id: &str, token: &LeaseToken) -> anyhow::Result<()> {
        let now = self.now().timestamp_millis();
        let expires = now + self.lease_ttl.num_milliseconds();
        self.client
            .update_item()
            .table_name(&self.leases_table)
            .key(col::CONVERSATION_ID, Self::s(conversation_id))
            .update_expression("SET #lea = :exp")
            .condition_expression(format!(
                "{} = :me AND {} = :token",
                col::OWNER_TASK_ARN,
                col::LEASE_TOKEN
            ))
            .expression_attribute_names("#lea", col::LEASE_EXPIRES_AT)
            .expression_attribute_values(":exp", Self::n(expires))
            .expression_attribute_values(":me", Self::s(&self.own_task_arn))
            .expression_attribute_values(":token", Self::s(token.as_str()))
            .send()
            .await
            .map_err(|e| {
                if is_conditional_check_failed_update(&e) {
                    anyhow::anyhow!("renew failed: lease no longer owned by this task")
                } else {
                    anyhow::Error::new(e).context("renew UpdateItem")
                }
            })?;
        Ok(())
    }

    fn lease_heartbeat_interval(&self) -> std::time::Duration {
        self.lease_ttl
            .to_std()
            .unwrap_or_else(|_| std::time::Duration::from_secs(3))
            .div_f64(3.0)
            .max(std::time::Duration::from_secs(1))
    }

    fn lease_ttl(&self) -> std::time::Duration {
        self.lease_ttl
            .to_std()
            .unwrap_or_else(|_| std::time::Duration::from_secs(DEFAULT_LEASE_TTL_SECS as u64))
    }

    async fn release(&self, conversation_id: &str, token: &LeaseToken) -> anyhow::Result<()> {
        let result = self
            .client
            .delete_item()
            .table_name(&self.leases_table)
            .key(col::CONVERSATION_ID, Self::s(conversation_id))
            .condition_expression(format!(
                "{} = :me AND {} = :token",
                col::OWNER_TASK_ARN,
                col::LEASE_TOKEN
            ))
            .expression_attribute_values(":me", Self::s(&self.own_task_arn))
            .expression_attribute_values(":token", Self::s(token.as_str()))
            .send()
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(e) if is_conditional_check_failed_delete(&e) => Ok(()), // not ours / already gone
            Err(e) => Err(anyhow::Error::new(e).context("release DeleteItem")),
        }
    }

    async fn forward(&self, peer: &str, payload: ForwardEvent) -> anyhow::Result<()> {
        let url = self.peer_url(peer);
        let mut req = self
            .http
            .post(&url)
            .json(&payload.slack_event_json)
            .timeout(std::time::Duration::from_secs(5));
        match std::env::var(crate::engine::dispatch_server::ENV_DISPATCH_TOKEN) {
            Ok(token) => {
                req = req.header(crate::engine::dispatch_server::DISPATCH_TOKEN_HEADER, token);
            },
            Err(_) => warn!(
                peer,
                env = crate::engine::dispatch_server::ENV_DISPATCH_TOKEN,
                "forwarding without dispatch token; peer will reject with 401 if it requires auth"
            ),
        }
        let resp = req.send().await.with_context(|| format!("POST {url}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("peer {peer} returned status {}", resp.status());
        }
        Ok(())
    }

    async fn append_turn(&self, conversation_id: &str, turn: Turn) -> anyhow::Result<()> {
        // 1. Atomic-ADD on a counter row to mint the next turn_seq.
        let counter_pk = format!("{conversation_id}:counter");
        let counter_resp = self
            .client
            .update_item()
            .table_name(&self.transcripts_table)
            .key(col::CONVERSATION_ID, Self::s(&counter_pk))
            .key(col::TURN_SEQ, Self::n(0))
            .update_expression("ADD #seq :one")
            .expression_attribute_names("#seq", "next_seq")
            .expression_attribute_values(":one", Self::n(1))
            .return_values(aws_sdk_dynamodb::types::ReturnValue::UpdatedNew)
            .send()
            .await
            .context("transcripts counter UpdateItem")?;

        let next_seq = counter_resp
            .attributes
            .as_ref()
            .and_then(|m| m.get("next_seq"))
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| anyhow::anyhow!("counter UpdateItem returned no next_seq"))?;

        // 2. PutItem the transcript row.
        let role_str = match turn.role {
            TurnRole::User => "user",
            TurnRole::Assistant => "assistant",
        };
        let expires = (turn.ts + Duration::days(DEFAULT_TRANSCRIPT_RETENTION_DAYS)).timestamp();
        let mut req = self
            .client
            .put_item()
            .table_name(&self.transcripts_table)
            .item(col::CONVERSATION_ID, Self::s(conversation_id))
            .item(col::TURN_SEQ, Self::n(next_seq))
            .item(col::ROLE, Self::s(role_str))
            .item(col::TEXT, Self::s(&turn.text))
            .item(col::TS, Self::s(turn.ts.to_rfc3339()))
            .item(col::EXPIRES_AT, Self::n(expires));
        if !turn.chunk_ids.is_empty() {
            req = req.item(col::CHUNK_IDS, AttributeValue::Ss(turn.chunk_ids.clone()));
        }
        req.send().await.context("transcript PutItem")?;
        Ok(())
    }

    async fn load_history(&self, conversation_id: &str, limit: usize) -> anyhow::Result<Vec<Turn>> {
        let resp = self
            .client
            .query()
            .table_name(&self.transcripts_table)
            .key_condition_expression(format!("{} = :pk", col::CONVERSATION_ID))
            .expression_attribute_values(":pk", Self::s(conversation_id))
            .scan_index_forward(false)
            .limit(limit as i32)
            .send()
            .await
            .context("transcripts Query")?;

        let mut turns: Vec<Turn> = resp
            .items
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| item_to_turn(&item))
            .collect();
        turns.reverse();
        Ok(turns)
    }

    async fn register_approval(
        &self,
        slack_msg_ts: &str,
        conversation_id: &str,
        ttl: std::time::Duration,
    ) -> anyhow::Result<()> {
        let Some(table) = self.approvals_table.as_ref() else {
            // Misconfigured: no approvals table provisioned. Caller will fall
            // back to local-only behaviour; cross-task forwarding won't work
            // for approvals until the env var is set.
            warn!("register_approval called but no approvals_table configured");
            return Ok(());
        };
        let expires = self.now().timestamp() + ttl.as_secs() as i64;
        self.client
            .put_item()
            .table_name(table)
            .item(col::SLACK_MSG_TS, Self::s(slack_msg_ts))
            .item(col::OWNER_TASK_ARN, Self::s(&self.own_task_arn))
            .item(col::CONVERSATION_ID, Self::s(conversation_id))
            .item(col::EXPIRES_AT, Self::n(expires))
            .send()
            .await
            .context("register_approval PutItem")?;
        Ok(())
    }

    async fn lookup_approval_owner(&self, slack_msg_ts: &str) -> anyhow::Result<Option<String>> {
        let Some(table) = self.approvals_table.as_ref() else {
            return Ok(None);
        };
        let resp = self
            .client
            .get_item()
            .table_name(table)
            .key(col::SLACK_MSG_TS, Self::s(slack_msg_ts))
            .send()
            .await
            .context("lookup_approval_owner GetItem")?;
        // DDB TTL sweep is best-effort; honour expires_at locally so a stale
        // row doesn't redirect a freshly registered approval.
        let Some(item) = resp.item else {
            return Ok(None);
        };
        let now = self.now().timestamp();
        let still_valid = item
            .get(col::EXPIRES_AT)
            .and_then(|v| v.as_n().ok())
            .and_then(|s| s.parse::<i64>().ok())
            .map(|exp| exp > now)
            .unwrap_or(true);
        if !still_valid {
            return Ok(None);
        }
        Ok(item.get(col::OWNER_TASK_ARN).and_then(|v| v.as_s().ok().cloned()))
    }

    async fn admit_prompt(
        &self,
        user_id: &str,
        max_prompts: u32,
        window: std::time::Duration,
    ) -> anyhow::Result<RateLimitOutcome> {
        let now = self.now().timestamp();
        let (window_start, window_end, expires_at) = rate_window_bounds(now, window)?;
        let key = format!("rate:{user_id}:{window_start}");
        let result = self
            .client
            .update_item()
            .table_name(&self.dedup_table)
            .key(col::SLACK_EVENT_ID, Self::s(key))
            .update_expression("SET #expires = :expires ADD #count :one")
            .condition_expression("attribute_not_exists(#count) OR #count < :limit")
            .expression_attribute_names("#count", col::RATE_COUNT)
            .expression_attribute_names("#expires", col::EXPIRES_AT)
            .expression_attribute_values(":one", Self::n(1))
            .expression_attribute_values(":limit", Self::n(i64::from(max_prompts)))
            .expression_attribute_values(":expires", Self::n(expires_at))
            .send()
            .await;
        match result {
            Ok(_) => Ok(RateLimitOutcome::Allowed),
            Err(error) if is_conditional_check_failed_update(&error) => {
                let retry_seconds = window_end
                    .checked_sub(now)
                    .context("rate-limit retry interval overflowed")?
                    .max(1);
                Ok(RateLimitOutcome::Limited {
                    retry_after: std::time::Duration::from_secs(
                        u64::try_from(retry_seconds).context("rate-limit retry interval was negative")?,
                    ),
                })
            },
            Err(error) => Err(error).context("fleet rate-limit UpdateItem"),
        }
    }
}

impl DynamoCoordinator {
    async fn resolve_ambiguous_dedup(&self, slack_event_id: &str, token: DedupeToken) -> DedupeOutcome {
        for attempt in 1..=AMBIGUOUS_DEDUP_READ_ATTEMPTS {
            let result = self
                .client
                .get_item()
                .table_name(&self.dedup_table)
                .key(col::SLACK_EVENT_ID, Self::s(slack_event_id))
                .consistent_read(true)
                .send()
                .await;
            match result {
                Ok(output) => {
                    return match output
                        .item
                        .as_ref()
                        .and_then(|item| item.get(col::DEDUPE_TOKEN))
                        .and_then(|value| value.as_s().ok())
                    {
                        Some(stored_token) if stored_token == token.as_str() => DedupeOutcome::Accepted { token },
                        Some(_) => DedupeOutcome::Duplicate,
                        None => DedupeOutcome::Unavailable,
                    };
                },
                Err(error) => {
                    warn!(?error, attempt, "dedupe_event ownership read failed");
                    if attempt < AMBIGUOUS_DEDUP_READ_ATTEMPTS {
                        tokio::time::sleep(AMBIGUOUS_DEDUP_RETRY_DELAY).await;
                    }
                },
            }
        }

        self.cleanup_ambiguous_dedup(slack_event_id, &token).await;
        DedupeOutcome::Unavailable
    }

    async fn cleanup_ambiguous_dedup(&self, slack_event_id: &str, token: &DedupeToken) {
        for attempt in 1..=AMBIGUOUS_DEDUP_CLEANUP_ATTEMPTS {
            match self.release_dedup(slack_event_id, token).await {
                Ok(()) => return,
                Err(error) => {
                    warn!(?error, attempt, "ambiguous dedupe reservation cleanup failed");
                    if attempt < AMBIGUOUS_DEDUP_CLEANUP_ATTEMPTS {
                        tokio::time::sleep(AMBIGUOUS_DEDUP_RETRY_DELAY).await;
                    }
                },
            }
        }
    }

    async fn reacquire_self_owned(
        &self,
        conversation_id: &str,
        previous_token: Option<&LeaseToken>,
    ) -> anyhow::Result<Option<LeaseToken>> {
        let now = self.now().timestamp_millis();
        let expires = now + self.lease_ttl.num_milliseconds();
        let token = LeaseToken::new(&self.own_task_arn);
        let mut request = self
            .client
            .put_item()
            .table_name(&self.leases_table)
            .item(col::CONVERSATION_ID, Self::s(conversation_id))
            .item(col::OWNER_TASK_ARN, Self::s(&self.own_task_arn))
            .item(col::LEASE_TOKEN, Self::s(token.as_str()))
            .item(col::LEASE_EXPIRES_AT, Self::n(expires))
            .item(col::EXPIRES_AT, Self::n((expires / 1000) + 24 * 3600))
            .expression_attribute_values(":me", Self::s(&self.own_task_arn));
        request = if let Some(previous_token) = previous_token {
            request
                .condition_expression(format!(
                    "{} = :me AND {} = :previous_token",
                    col::OWNER_TASK_ARN,
                    col::LEASE_TOKEN
                ))
                .expression_attribute_values(":previous_token", Self::s(previous_token.as_str()))
        } else {
            request.condition_expression(format!(
                "{} = :me AND attribute_not_exists({})",
                col::OWNER_TASK_ARN,
                col::LEASE_TOKEN
            ))
        };

        match request.send().await {
            Ok(_) => Ok(Some(token)),
            Err(error) if is_conditional_check_failed_put(&error) => Ok(None),
            Err(error) => Err(anyhow::Error::new(error).context("self-owned lease PutItem")),
        }
    }

    async fn read_lease(&self, conversation_id: &str) -> anyhow::Result<Option<LeaseRecord>> {
        let resp = self
            .client
            .get_item()
            .table_name(&self.leases_table)
            .key(col::CONVERSATION_ID, Self::s(conversation_id))
            .send()
            .await
            .context("read_lease GetItem")?;
        let Some(item) = resp.item else {
            return Ok(None);
        };
        let Some(owner) = item
            .get(col::OWNER_TASK_ARN)
            .and_then(|value| value.as_s().ok())
            .cloned()
        else {
            return Ok(None);
        };
        let token = item
            .get(col::LEASE_TOKEN)
            .and_then(|value| value.as_s().ok())
            .cloned()
            .map(LeaseToken::from_stored);
        Ok(Some(LeaseRecord { owner, token }))
    }
}

struct LeaseRecord {
    owner: String,
    token: Option<LeaseToken>,
}

fn item_to_turn(item: &HashMap<String, AttributeValue>) -> Option<Turn> {
    let role_str = item.get(col::ROLE)?.as_s().ok()?;
    let role = match role_str.as_str() {
        "user" => TurnRole::User,
        "assistant" => TurnRole::Assistant,
        _ => return None,
    };
    let text = item.get(col::TEXT)?.as_s().ok()?.clone();
    let ts_str = item.get(col::TS)?.as_s().ok()?;
    let ts: DateTime<Utc> = DateTime::parse_from_rfc3339(ts_str).ok()?.with_timezone(&Utc);
    let chunk_ids = item
        .get(col::CHUNK_IDS)
        .and_then(|v| v.as_ss().ok())
        .map(|ss| ss.to_vec())
        .unwrap_or_default();
    Some(Turn {
        role,
        text,
        ts,
        chunk_ids,
    })
}

fn is_conditional_check_failed_put(err: &SdkError<PutItemError>) -> bool {
    matches!(
        err,
        SdkError::ServiceError(svc) if matches!(svc.err(), PutItemError::ConditionalCheckFailedException(_))
    )
}

fn is_conditional_check_failed_update(err: &SdkError<UpdateItemError>) -> bool {
    matches!(
        err,
        SdkError::ServiceError(svc) if matches!(svc.err(), UpdateItemError::ConditionalCheckFailedException(_))
    )
}

fn is_conditional_check_failed_delete(err: &SdkError<DeleteItemError>) -> bool {
    matches!(
        err,
        SdkError::ServiceError(svc) if matches!(svc.err(), DeleteItemError::ConditionalCheckFailedException(_))
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        Mutex,
    };

    use aws_sdk_dynamodb::config::retry::RetryConfig;
    use aws_sdk_dynamodb::operation::delete_item::{
        DeleteItemError,
        DeleteItemOutput,
    };
    use aws_sdk_dynamodb::operation::get_item::{
        GetItemError,
        GetItemOutput,
    };
    use aws_sdk_dynamodb::operation::put_item::PutItemError;
    use aws_sdk_dynamodb::types::error::{
        ConditionalCheckFailedException,
        InternalServerError,
    };
    use aws_smithy_mocks::{
        MockResponse,
        Rule,
        RuleMode,
        mock,
        mock_client,
    };
    use chrono::TimeZone;

    use super::*;

    fn ambiguous_dedup_coordinator(use_attempt_token: bool) -> (DynamoCoordinator, Arc<Mutex<Option<String>>>, Rule) {
        let attempted_token = Arc::new(Mutex::new(None));
        let captured_token = attempted_token.clone();
        let put_rule = mock!(aws_sdk_dynamodb::Client::put_item).then_compute_response(move |input| {
            let token = input
                .item()
                .and_then(|item| item.get(col::DEDUPE_TOKEN))
                .and_then(|value| value.as_s().ok())
                .cloned()
                .expect("dedup PutItem must carry its ownership token");
            *captured_token.lock().expect("captured token poisoned") = Some(token);
            MockResponse::Error(PutItemError::InternalServerError(
                InternalServerError::builder().message("response lost").build(),
            ))
        });

        let returned_token = attempted_token.clone();
        let get_rule = mock!(aws_sdk_dynamodb::Client::get_item)
            .match_requests(|input| input.consistent_read() == Some(true))
            .then_compute_output(move |_| {
                let token = if use_attempt_token {
                    returned_token
                        .lock()
                        .expect("captured token poisoned")
                        .clone()
                        .expect("PutItem must run before GetItem")
                } else {
                    "competing-task:1".to_string()
                };
                GetItemOutput::builder()
                    .item(col::SLACK_EVENT_ID, AttributeValue::S("EvAmbiguous".to_string()))
                    .item(col::DEDUPE_TOKEN, AttributeValue::S(token))
                    .build()
            });

        let delete_rule =
            mock!(aws_sdk_dynamodb::Client::delete_item).then_output(|| DeleteItemOutput::builder().build());
        let client = mock_client!(
            aws_sdk_dynamodb,
            RuleMode::MatchAny,
            [&put_rule, &get_rule, &delete_rule],
            |builder| builder.retry_config(RetryConfig::disabled())
        );
        (
            DynamoCoordinator::new(client, "leases", "transcripts", "dedup", "attempting-task"),
            attempted_token,
            delete_rule,
        )
    }

    type CapturedDelete = Arc<Mutex<Option<(String, String)>>>;

    fn unreadable_ambiguous_dedup_coordinator(
        competing_owner: bool,
    ) -> (
        DynamoCoordinator,
        Arc<Mutex<Option<String>>>,
        CapturedDelete,
        Rule,
        Rule,
    ) {
        let attempted_token = Arc::new(Mutex::new(None));
        let captured_token = attempted_token.clone();
        let put_rule = mock!(aws_sdk_dynamodb::Client::put_item).then_compute_response(move |input| {
            let token = input
                .item()
                .and_then(|item| item.get(col::DEDUPE_TOKEN))
                .and_then(|value| value.as_s().ok())
                .cloned()
                .expect("dedup PutItem must carry its ownership token");
            *captured_token.lock().expect("captured token poisoned") = Some(token);
            MockResponse::Error(PutItemError::InternalServerError(
                InternalServerError::builder().message("response lost").build(),
            ))
        });
        let get_rule = mock!(aws_sdk_dynamodb::Client::get_item)
            .match_requests(|input| input.consistent_read() == Some(true))
            .then_error(|| {
                GetItemError::InternalServerError(InternalServerError::builder().message("read unavailable").build())
            });

        let captured_delete = Arc::new(Mutex::new(None));
        let delete_request = captured_delete.clone();
        let delete_rule = mock!(aws_sdk_dynamodb::Client::delete_item)
            .match_requests(move |input| {
                let condition = input.condition_expression().unwrap_or_default().to_string();
                let token = input
                    .expression_attribute_values()
                    .and_then(|values| values.get(":token"))
                    .and_then(|value| value.as_s().ok())
                    .cloned()
                    .unwrap_or_default();
                *delete_request.lock().expect("captured delete poisoned") = Some((condition, token));
                true
            })
            .then_compute_response(move |_| {
                if competing_owner {
                    MockResponse::Error(DeleteItemError::ConditionalCheckFailedException(
                        ConditionalCheckFailedException::builder()
                            .message("token changed")
                            .build(),
                    ))
                } else {
                    MockResponse::Output(DeleteItemOutput::builder().build())
                }
            });

        let client = mock_client!(
            aws_sdk_dynamodb,
            RuleMode::MatchAny,
            [&put_rule, &get_rule, &delete_rule],
            |builder| builder.retry_config(RetryConfig::disabled())
        );
        (
            DynamoCoordinator::new(client, "leases", "transcripts", "dedup", "attempting-task"),
            attempted_token,
            captured_delete,
            get_rule,
            delete_rule,
        )
    }

    /// Smoke test: the table-name plumbing accepts owned strings + slices and
    /// the resulting `DynamoCoordinator` is `Clone + Send + Sync`. Live AWS
    /// behaviour is exercised by integration tests against DynamoDB Local
    /// (out of scope for this unit suite).
    #[tokio::test]
    async fn coordinator_constructs_and_is_clone() {
        // Build the SDK client without contacting AWS.
        let cfg = aws_config::SdkConfig::builder()
            .region(aws_config::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        let client = Client::new(&cfg);
        let coord = DynamoCoordinator::new(
            client,
            "kiro-bot-leases-test",
            "kiro-bot-transcripts-test",
            "kiro-bot-event-dedup-test",
            "arn:aws:ecs:us-east-1:1:task/c/abc",
        )
        .with_lease_ttl(Duration::minutes(5))
        .with_peer_base_url("http://127.0.0.1:8080");

        assert_eq!(coord.leases_table, "kiro-bot-leases-test");
        assert_eq!(coord.lease_ttl, Duration::minutes(5));
        assert_eq!(coord.lease_heartbeat_interval(), std::time::Duration::from_secs(100));
        assert!(coord.peer_base_url.is_some());

        // Cloneable so the engine + dispatch server can each hold a handle.
        let _cloned = coord.clone();
        // Send + Sync: stash in an Arc<dyn Coordinator>.
        let _erased: std::sync::Arc<dyn Coordinator> = std::sync::Arc::new(coord);
    }

    #[test]
    fn dedup_records_are_retained_for_twenty_four_hours() {
        assert_eq!(DEFAULT_DEDUP_RETENTION_SECS, 24 * 60 * 60);
    }

    #[tokio::test]
    async fn ambiguous_dedup_write_accepts_committed_attempt_token() {
        let (coordinator, attempted_token, delete_rule) = ambiguous_dedup_coordinator(true);

        let outcome = coordinator.dedupe_event_outcome("EvAmbiguous").await;

        let DedupeOutcome::Accepted { token } = outcome else {
            panic!("committed attempt must be accepted");
        };
        assert_eq!(
            Some(token.as_str()),
            attempted_token.lock().expect("captured token poisoned").as_deref()
        );
        assert_eq!(delete_rule.num_calls(), 0);
    }

    #[tokio::test]
    async fn ambiguous_dedup_write_reports_competing_token_as_duplicate() {
        let (coordinator, _, delete_rule) = ambiguous_dedup_coordinator(false);

        assert_eq!(
            coordinator.dedupe_event_outcome("EvAmbiguous").await,
            DedupeOutcome::Duplicate
        );
        assert_eq!(delete_rule.num_calls(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn ambiguous_dedup_read_failure_conditionally_cleans_attempted_reservation() {
        let (coordinator, attempted_token, captured_delete, get_rule, delete_rule) =
            unreadable_ambiguous_dedup_coordinator(false);

        assert_eq!(
            coordinator.dedupe_event_outcome("EvAmbiguous").await,
            DedupeOutcome::Unavailable
        );
        assert_eq!(get_rule.num_calls(), AMBIGUOUS_DEDUP_READ_ATTEMPTS);
        assert_eq!(delete_rule.num_calls(), 1);
        assert_eq!(
            *captured_delete.lock().expect("captured delete poisoned"),
            Some((
                format!("{} = :token", col::DEDUPE_TOKEN),
                attempted_token
                    .lock()
                    .expect("captured token poisoned")
                    .clone()
                    .expect("PutItem token missing"),
            ))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn ambiguous_dedup_cleanup_never_deletes_competing_reservation() {
        let (coordinator, attempted_token, captured_delete, get_rule, delete_rule) =
            unreadable_ambiguous_dedup_coordinator(true);

        assert_eq!(
            coordinator.dedupe_event_outcome("EvAmbiguous").await,
            DedupeOutcome::Unavailable
        );
        assert_eq!(get_rule.num_calls(), AMBIGUOUS_DEDUP_READ_ATTEMPTS);
        assert_eq!(delete_rule.num_calls(), 1);
        assert_eq!(
            *captured_delete.lock().expect("captured delete poisoned"),
            Some((
                format!("{} = :token", col::DEDUPE_TOKEN),
                attempted_token
                    .lock()
                    .expect("captured token poisoned")
                    .clone()
                    .expect("PutItem token missing"),
            ))
        );
    }

    #[test]
    fn rate_window_accepts_largest_ttl_safe_boundary() {
        let (start, end, expires) = rate_window_bounds(
            0,
            std::time::Duration::from_secs(crate::engine::rate_limit::MAX_WINDOW_SECS),
        )
        .unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, crate::engine::rate_limit::MAX_WINDOW_SECS as i64);
        assert_eq!(expires, i64::MAX);
    }

    #[test]
    fn rate_window_rejects_zero_and_overflowing_values() {
        assert!(rate_window_bounds(0, std::time::Duration::ZERO).is_err());
        assert!(
            rate_window_bounds(
                0,
                std::time::Duration::from_secs(crate::engine::rate_limit::MAX_WINDOW_SECS + 1),
            )
            .is_err()
        );
        assert!(rate_window_bounds(0, std::time::Duration::from_secs(u64::MAX)).is_err());
    }

    #[test]
    fn item_to_turn_round_trips_user_role() {
        let mut item = HashMap::new();
        item.insert(col::ROLE.to_string(), AttributeValue::S("user".to_string()));
        item.insert(col::TEXT.to_string(), AttributeValue::S("hi".to_string()));
        let ts = Utc.with_ymd_and_hms(2026, 5, 19, 17, 0, 0).unwrap();
        item.insert(col::TS.to_string(), AttributeValue::S(ts.to_rfc3339()));

        let turn = item_to_turn(&item).expect("parse turn");
        assert_eq!(turn.role, TurnRole::User);
        assert_eq!(turn.text, "hi");
        assert_eq!(turn.ts, ts);
        assert!(turn.chunk_ids.is_empty(), "missing chunk_ids attribute → empty Vec");
    }

    #[test]
    fn item_to_turn_preserves_chunk_ids() {
        let mut item = HashMap::new();
        item.insert(col::ROLE.to_string(), AttributeValue::S("assistant".to_string()));
        item.insert(col::TEXT.to_string(), AttributeValue::S("...".to_string()));
        item.insert(col::TS.to_string(), AttributeValue::S(Utc::now().to_rfc3339()));
        item.insert(
            col::CHUNK_IDS.to_string(),
            AttributeValue::Ss(vec!["docs/a.md".into(), "docs/b.md".into()]),
        );
        let turn = item_to_turn(&item).expect("parse");
        assert_eq!(turn.chunk_ids, vec!["docs/a.md", "docs/b.md"]);
    }

    #[test]
    fn item_to_turn_rejects_unknown_role() {
        let mut item = HashMap::new();
        item.insert(col::ROLE.to_string(), AttributeValue::S("not-a-role".to_string()));
        item.insert(col::TEXT.to_string(), AttributeValue::S("x".to_string()));
        item.insert(col::TS.to_string(), AttributeValue::S(Utc::now().to_rfc3339()));
        assert!(item_to_turn(&item).is_none());
    }

    #[test]
    fn peer_url_uses_override_when_set() {
        let cfg = aws_config::SdkConfig::builder()
            .region(aws_config::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        let coord = DynamoCoordinator::new(Client::new(&cfg), "l", "t", "d", "self")
            .with_peer_base_url("http://127.0.0.1:9999");
        assert_eq!(coord.peer_url("ignored"), "http://127.0.0.1:9999/dispatch");
    }

    #[test]
    fn peer_url_falls_back_to_peer_when_no_override() {
        let cfg = aws_config::SdkConfig::builder()
            .region(aws_config::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        let coord = DynamoCoordinator::new(Client::new(&cfg), "l", "t", "d", "self");
        assert_eq!(coord.peer_url("10.0.0.1:8080"), "http://10.0.0.1:8080/dispatch");
    }
}
