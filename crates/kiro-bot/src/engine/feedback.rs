//! 👍/👎 feedback persistence. Backed by the `kiro-bot-feedback-<stage>`
//! DynamoDB table provisioned in `KiroBotStorageStack`.
//!
//! The Slack frontend calls [`FeedbackWriter::record`] when a thumbs reaction
//! lands on a bot-authored message. The nightly metrics Lambda
//! (`crates/kiro-bot-metrics`) reads this table to publish
//! `KiroHelpBot::NegativeFeedbackRate`, which feeds the
//! `kiro-bot-<stage>-negative-feedback-high` alarm.

use std::collections::HashMap;

use anyhow::Context;
use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use chrono::{
    DateTime,
    Duration,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

const FEEDBACK_RETENTION_DAYS: i64 = 90;

/// 👍 or 👎. Keep the wire form to "+1" / "-1" matching Slack reaction names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reaction {
    /// 👍 / `:+1:` / `:thumbsup:`
    ThumbsUp,
    /// 👎 / `:-1:` / `:thumbsdown:`
    ThumbsDown,
}

impl Reaction {
    /// Map the literal Slack reaction name to a canonical [`Reaction`].
    /// Returns `None` for any reaction we don't track (we only persist
    /// thumbs-up and thumbs-down).
    pub fn from_slack(name: &str) -> Option<Self> {
        match name {
            "+1" | "thumbsup" => Some(Self::ThumbsUp),
            "-1" | "thumbsdown" => Some(Self::ThumbsDown),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ThumbsUp => "+1",
            Self::ThumbsDown => "-1",
        }
    }
}

/// One feedback row, ready to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackRecord {
    pub slack_msg_id: String,
    pub reaction: Reaction,
    pub chunk_ids: Vec<String>,
    pub ts: DateTime<Utc>,
}

/// Trait so unit tests can substitute a fake without a DDB client.
#[async_trait::async_trait]
pub trait FeedbackWriter: Send + Sync {
    async fn record(&self, record: FeedbackRecord) -> anyhow::Result<()>;
}

/// Live writer that talks to the DDB feedback table. The table name is
/// stage-suffixed by `KiroBotStorageStack` and threaded into the bot via the
/// `KIRO_BOT_FEEDBACK_TABLE` env var.
pub struct DynamoFeedbackWriter {
    client: Client,
    pub table: String,
}

impl DynamoFeedbackWriter {
    pub fn new(client: Client, table: impl Into<String>) -> Self {
        Self {
            client,
            table: table.into(),
        }
    }
}

#[async_trait::async_trait]
impl FeedbackWriter for DynamoFeedbackWriter {
    async fn record(&self, record: FeedbackRecord) -> anyhow::Result<()> {
        let item = build_item(&record);
        self.client
            .put_item()
            .table_name(&self.table)
            .set_item(Some(item))
            .send()
            .await
            .with_context(|| format!("PutItem {} for msg {}", self.table, record.slack_msg_id))?;
        Ok(())
    }
}

fn build_item(record: &FeedbackRecord) -> HashMap<String, AttributeValue> {
    let mut m = HashMap::new();
    m.insert(
        "slack_msg_id".to_string(),
        AttributeValue::S(record.slack_msg_id.clone()),
    );
    m.insert(
        "reaction".to_string(),
        AttributeValue::S(record.reaction.as_str().to_string()),
    );
    m.insert("ts".to_string(), AttributeValue::S(record.ts.to_rfc3339()));
    let expires = (record.ts + Duration::days(FEEDBACK_RETENTION_DAYS)).timestamp();
    m.insert(
        "expires_at".to_string(),
        AttributeValue::N(expires.to_string()),
    );
    if !record.chunk_ids.is_empty() {
        m.insert(
            "chunk_ids".to_string(),
            AttributeValue::Ss(record.chunk_ids.clone()),
        );
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_from_slack_recognizes_canonical_names() {
        assert_eq!(Reaction::from_slack("+1"), Some(Reaction::ThumbsUp));
        assert_eq!(Reaction::from_slack("thumbsup"), Some(Reaction::ThumbsUp));
        assert_eq!(Reaction::from_slack("-1"), Some(Reaction::ThumbsDown));
        assert_eq!(Reaction::from_slack("thumbsdown"), Some(Reaction::ThumbsDown));
    }

    #[test]
    fn reaction_from_slack_ignores_anything_else() {
        for unrelated in ["heart", "eyes", "fire", "pray", ""] {
            assert!(Reaction::from_slack(unrelated).is_none(), "got Some for {unrelated:?}");
        }
    }

    #[test]
    fn build_item_includes_required_columns_and_ttl() {
        let rec = FeedbackRecord {
            slack_msg_id: "C123:1700000000.0".into(),
            reaction: Reaction::ThumbsDown,
            chunk_ids: vec!["docs/auth.md".into(), "docs/login.md".into()],
            ts: chrono::DateTime::parse_from_rfc3339("2026-05-19T17:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };
        let item = build_item(&rec);
        assert_eq!(
            item.get("slack_msg_id").unwrap().as_s().unwrap(),
            "C123:1700000000.0"
        );
        assert_eq!(item.get("reaction").unwrap().as_s().unwrap(), "-1");
        assert_eq!(
            item.get("ts").unwrap().as_s().unwrap(),
            "2026-05-19T17:00:00+00:00"
        );
        let chunks = item.get("chunk_ids").unwrap().as_ss().unwrap();
        assert!(chunks.contains(&"docs/auth.md".to_string()));
        assert!(chunks.contains(&"docs/login.md".to_string()));
        // expires_at = ts + 90d
        let expires_str = item.get("expires_at").unwrap().as_n().unwrap();
        let expires: i64 = expires_str.parse().unwrap();
        let expected = (rec.ts + Duration::days(90)).timestamp();
        assert_eq!(expires, expected);
    }

    #[test]
    fn build_item_omits_chunk_ids_when_empty() {
        let rec = FeedbackRecord {
            slack_msg_id: "C123:1".into(),
            reaction: Reaction::ThumbsUp,
            chunk_ids: vec![],
            ts: Utc::now(),
        };
        let item = build_item(&rec);
        assert!(
            item.get("chunk_ids").is_none(),
            "expected no chunk_ids attribute when empty"
        );
    }
}
