//! 👍/👎 feedback persistence. Backed by the `kiro-bot-feedback-<stage>`
//! DynamoDB table provisioned in `KiroBotStorageStack`.
//!
//! The Slack frontend calls [`FeedbackWriter::record`] when a thumbs reaction
//! lands on a bot-authored message. The nightly metrics Lambda
//! (`crates/kiro-bot-metrics`) reads this table to publish
//! `KiroHelpBot::NegativeFeedbackRate`, which feeds the
//! `kiro-bot-<stage>-negative-feedback-high` alarm.

use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};

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

/// Pick `chunk_ids` for a feedback row given a window of recent transcript
/// turns (oldest first, as `Coordinator::load_history` returns them). Walks
/// from newest to oldest looking for the first assistant turn that carries
/// chunk_ids — that's the answer the user just reacted to.
///
/// Returns `Vec::new()` if no such turn exists in the window. Empty
/// chunk_ids on the recorded feedback row are still useful as a thumbs
/// signal; they just lose the per-doc attribution.
pub fn chunk_ids_for_recent_assistant_turn(turns: &[crate::engine::coordinator::Turn]) -> Vec<String> {
    use crate::engine::coordinator::TurnRole;
    for turn in turns.iter().rev() {
        if turn.role == TurnRole::Assistant && !turn.chunk_ids.is_empty() {
            return turn.chunk_ids.clone();
        }
    }
    Vec::new()
}

pub fn extract_cited_sources(reply: &str) -> Vec<String> {
    let Some((_, sources)) = sources_line(reply) else {
        return Vec::new();
    };

    parse_cited_sources(sources)
        .into_iter()
        .map(|source| source.identifier)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedReply {
    pub text: String,
    pub sources: Vec<String>,
}

#[derive(Debug)]
struct CitedSource {
    rendered: String,
    identifier: String,
}

/// Keep only citations backed by trusted conversation provenance.
pub fn validate_cited_sources(reply: &str, authorized: &[String]) -> ValidatedReply {
    let Some((source_line, sources)) = sources_line(reply) else {
        return ValidatedReply {
            text: reply.to_string(),
            sources: Vec::new(),
        };
    };
    let authorized = authorized.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let valid = parse_cited_sources(sources)
        .into_iter()
        .filter_map(|source| {
            authorized_source(&source.identifier, &authorized)
                .and_then(|identifier| seen.insert(identifier.clone()).then_some((source.rendered, identifier)))
        })
        .collect::<Vec<_>>();

    let mut lines = reply.lines().map(str::to_string).collect::<Vec<_>>();
    if valid.is_empty() {
        lines.remove(source_line);
        if source_line > 0
            && source_line < lines.len()
            && lines[source_line - 1].trim().is_empty()
            && lines[source_line].trim().is_empty()
        {
            lines.remove(source_line);
        } else if source_line == lines.len() && lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
    } else {
        lines[source_line] = format!(
            "Sources: {}",
            valid
                .iter()
                .map(|(rendered, _)| rendered.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    ValidatedReply {
        text: lines.join("\n"),
        sources: valid.into_iter().map(|(_, identifier)| identifier).collect(),
    }
}

/// Extract source identifiers from one successful ACP tool result.
pub fn successful_tool_sources(
    server_name: Option<&str>,
    tool_name: &str,
    raw_input: &serde_json::Value,
    raw_output: &serde_json::Value,
) -> Vec<String> {
    if output_reports_error(raw_output) {
        return Vec::new();
    }

    let mut sources = BTreeSet::new();
    match (server_name, tool_name) {
        (None, "read" | "fs_read" | "fsRead") => collect_read_paths(raw_input, &mut sources),
        (Some("kiro-github-read"), "search_github_issues") => {
            visit_mcp_payloads(raw_output, &mut |key, value| {
                if matches!(key, "html_url" | "htmlUrl")
                    && let Some(url) = value.as_str()
                    && url.starts_with("https://github.com/")
                {
                    sources.insert(normalize_source_identifier(url));
                    if let Some(alias) = github_source_alias(url) {
                        sources.insert(alias);
                    }
                }
            });
        },
        (Some("kiro-mcp"), tool) if tool.starts_with("Taskei___") => {
            visit_mcp_payloads(raw_output, &mut |key, value| {
                if taskei_source_key(tool, key)
                    && let Some(identifier) = value.as_str()
                    && !identifier.trim().is_empty()
                {
                    sources.insert(format!("taskei:{}", identifier.trim()));
                }
            });
        },
        _ => {},
    }

    sources.into_iter().collect()
}

fn sources_line(reply: &str) -> Option<(usize, &str)> {
    reply
        .lines()
        .enumerate()
        .filter_map(|(index, line)| line.trim().strip_prefix("Sources:").map(|sources| (index, sources)))
        .last()
}

fn parse_cited_sources(sources: &str) -> Vec<CitedSource> {
    sources
        .split([',', '|'])
        .map(|value| value.trim().trim_matches(['*', '_']))
        .filter(|value| !value.is_empty())
        .map(|value| {
            let quoted = value
                .strip_prefix('`')
                .and_then(|value| value.strip_suffix('`'))
                .filter(|value| !value.is_empty());
            CitedSource {
                rendered: value.to_string(),
                identifier: quoted
                    .or_else(|| markdown_link_target(value))
                    .map(normalize_source_identifier)
                    .unwrap_or_else(|| normalize_source_identifier(value)),
            }
        })
        .collect()
}

fn markdown_link_target(value: &str) -> Option<&str> {
    let (_, target) = value.strip_prefix('[')?.split_once("](")?;
    target.strip_suffix(')').filter(|target| !target.is_empty())
}

fn authorized_source(identifier: &str, authorized: &HashSet<&str>) -> Option<String> {
    if authorized.contains(identifier) && !identifier.ends_with("-*") {
        return Some(identifier.to_string());
    }

    let (path, cited_start, cited_end) = parse_line_qualifier(identifier)?;
    authorized
        .iter()
        .filter_map(|source| parse_authorized_line_scope(source))
        .any(|(authorized_path, authorized_start, authorized_end)| {
            path == authorized_path
                && cited_start >= authorized_start
                && authorized_end.is_none_or(|authorized_end| cited_end <= authorized_end)
        })
        .then(|| identifier.to_string())
}

fn parse_line_qualifier(identifier: &str) -> Option<(&str, u64, u64)> {
    let (path, lines) = identifier.rsplit_once(':')?;
    let mut ranges = lines.split('-');
    let start = ranges.next()?.parse().ok()?;
    let end = ranges.next().map(str::parse).transpose().ok()?.unwrap_or(start);
    if ranges.next().is_some() || end < start {
        return None;
    }
    Some((path, start, end))
}

fn parse_authorized_line_scope(identifier: &str) -> Option<(&str, u64, Option<u64>)> {
    let (path, lines) = identifier.rsplit_once(':')?;
    let (start, end) = lines.split_once('-')?;
    let start = start.parse().ok()?;
    let end = if end == "*" { None } else { Some(end.parse().ok()?) };
    Some((path, start, end))
}

fn normalize_source_identifier(identifier: &str) -> String {
    identifier
        .trim()
        .strip_prefix("./")
        .unwrap_or(identifier.trim())
        .to_string()
}

fn collect_read_paths(input: &serde_json::Value, sources: &mut BTreeSet<String>) {
    let Some(operations) = input.get("operations").and_then(serde_json::Value::as_array) else {
        return;
    };
    for operation in operations {
        if let Some(path) = operation.get("path").and_then(serde_json::Value::as_str) {
            let path = normalize_source_identifier(path);
            sources.insert(path.clone());
            if operation.get("mode").and_then(serde_json::Value::as_str) == Some("Line") {
                let offset = operation
                    .get("offset")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default();
                let start = offset.saturating_add(1);
                let end = operation
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .filter(|limit| *limit > 0)
                    .map_or_else(|| "*".to_string(), |limit| offset.saturating_add(limit).to_string());
                sources.insert(format!("{path}:{start}-{end}"));
            }
        }
        if let Some(paths) = operation.get("paths").and_then(serde_json::Value::as_array) {
            sources.extend(
                paths
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(normalize_source_identifier),
            );
        }
    }
}

fn output_reports_error(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => {
            object.get("isError").and_then(serde_json::Value::as_bool) == Some(true)
                || object.values().any(output_reports_error)
        },
        serde_json::Value::Array(values) => values.iter().any(output_reports_error),
        _ => false,
    }
}

fn visit_mcp_payloads(value: &serde_json::Value, visitor: &mut impl FnMut(&str, &serde_json::Value)) {
    match value {
        serde_json::Value::Object(object) => {
            if object.get("type").and_then(serde_json::Value::as_str) == Some("text")
                && let Some(text) = object.get("text").and_then(serde_json::Value::as_str)
                && let Ok(payload) = serde_json::from_str(text)
            {
                visit_json_payload(&payload, visitor);
            }
            for value in object.values() {
                visit_mcp_payloads(value, visitor);
            }
        },
        serde_json::Value::Array(values) => {
            for value in values {
                visit_mcp_payloads(value, visitor);
            }
        },
        _ => {},
    }
}

fn visit_json_payload(value: &serde_json::Value, visitor: &mut impl FnMut(&str, &serde_json::Value)) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                visitor(key, value);
                visit_json_payload(value, visitor);
            }
        },
        serde_json::Value::Array(values) => {
            for value in values {
                visit_json_payload(value, visitor);
            }
        },
        _ => {},
    }
}

fn taskei_source_key(tool: &str, key: &str) -> bool {
    match tool {
        "Taskei___list_tasks" | "Taskei___get_task" => matches!(key, "id" | "taskId" | "task_id"),
        "Taskei___get_room" => matches!(key, "id" | "roomId" | "room_id"),
        "Taskei___list_room_resource" => {
            matches!(key, "id" | "resourceId" | "resource_id" | "roomId" | "room_id")
        },
        _ => false,
    }
}

fn github_source_alias(url: &str) -> Option<String> {
    let path = url.strip_prefix("https://github.com/")?;
    let mut segments = path.split('/');
    let owner = segments.next()?;
    let repo = segments.next()?;
    let kind = match segments.next()? {
        "issues" => "github_issue",
        "pull" => "github_pr",
        _ => return None,
    };
    let number = segments.next()?.split(['#', '?']).next()?;
    if owner.is_empty() || repo.is_empty() || number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("{kind}:{owner}/{repo}#{number}"))
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
    m.insert("expires_at".to_string(), AttributeValue::N(expires.to_string()));
    if !record.chunk_ids.is_empty() {
        m.insert("chunk_ids".to_string(), AttributeValue::Ss(record.chunk_ids.clone()));
    }
    m
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

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
        for unrelated in ["heart", "eyes", "fire", "pray", "white_check_mark", "unlock", "x", ""] {
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
        assert_eq!(item.get("slack_msg_id").unwrap().as_s().unwrap(), "C123:1700000000.0");
        assert_eq!(item.get("reaction").unwrap().as_s().unwrap(), "-1");
        assert_eq!(item.get("ts").unwrap().as_s().unwrap(), "2026-05-19T17:00:00+00:00");
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
            !item.contains_key("chunk_ids"),
            "expected no chunk_ids attribute when empty"
        );
    }

    use crate::engine::coordinator::{
        Turn,
        TurnRole,
    };

    fn turn(role: TurnRole, text: &str, chunks: &[&str], secs: i64) -> Turn {
        Turn {
            role,
            text: text.to_string(),
            ts: chrono::Utc.timestamp_opt(secs, 0).single().unwrap(),
            chunk_ids: chunks.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn chunk_ids_picks_most_recent_grounded_assistant_turn() {
        let turns = vec![
            turn(TurnRole::User, "hi", &[], 100),
            turn(TurnRole::Assistant, "ok", &["docs/old.md"], 101),
            turn(TurnRole::User, "next", &[], 102),
            turn(TurnRole::Assistant, "answer", &["docs/auth.md", "docs/login.md"], 103),
        ];
        let got = chunk_ids_for_recent_assistant_turn(&turns);
        assert_eq!(got, vec!["docs/auth.md", "docs/login.md"]);
    }

    #[test]
    fn chunk_ids_skips_assistant_turns_without_chunks() {
        let turns = vec![
            turn(TurnRole::Assistant, "grounded", &["docs/auth.md"], 100),
            turn(TurnRole::User, "follow up", &[], 101),
            turn(TurnRole::Assistant, "ungrounded reply", &[], 102),
        ];
        let got = chunk_ids_for_recent_assistant_turn(&turns);
        // Walks past the empty assistant turn back to the grounded one.
        assert_eq!(got, vec!["docs/auth.md"]);
    }

    #[test]
    fn chunk_ids_returns_empty_when_no_grounded_turn_in_window() {
        let turns = vec![
            turn(TurnRole::User, "q", &[], 100),
            turn(TurnRole::Assistant, "a", &[], 101),
        ];
        assert!(chunk_ids_for_recent_assistant_turn(&turns).is_empty());
    }

    #[test]
    fn chunk_ids_ignores_user_turns_even_if_chunks_set() {
        // Defensive: user turns shouldn't carry chunk_ids in practice, but
        // if they did (corruption / migration), don't pick them up.
        let turns = vec![turn(TurnRole::User, "q", &["docs/wrong.md"], 100)];
        assert!(chunk_ids_for_recent_assistant_turn(&turns).is_empty());
    }

    #[test]
    fn cited_sources_extracts_exact_identifiers_from_the_sources_line() {
        let reply = concat!(
            "Use the login command.\n\n",
            "Sources: `docs/auth.md`, `crates/chat-cli/src/auth.rs:40-52`, `taskei:abc-123`\n\n",
            "_Generated content may be inaccurate._"
        );
        assert_eq!(extract_cited_sources(reply), vec![
            "docs/auth.md",
            "crates/chat-cli/src/auth.rs:40-52",
            "taskei:abc-123"
        ]);
    }

    #[test]
    fn cited_sources_accepts_unquoted_source_lists() {
        assert_eq!(
            extract_cited_sources("Answer\n\nSources: docs/a.md | github_issue:#42"),
            vec!["docs/a.md", "github_issue:#42"]
        );
        assert!(extract_cited_sources("Answer without sources").is_empty());
    }

    #[test]
    fn successful_read_authorizes_requested_paths() {
        let input = serde_json::json!({
            "operations": [
                { "mode": "Line", "path": "./crates/kiro-bot/src/engine/acp.rs", "offset": 10, "limit": 20 },
                { "mode": "Image", "paths": ["docs/diagram.png"] }
            ]
        });
        let output = serde_json::json!({
            "items": [{ "Text": "source contents" }]
        });

        assert_eq!(successful_tool_sources(None, "read", &input, &output), vec![
            "crates/kiro-bot/src/engine/acp.rs",
            "crates/kiro-bot/src/engine/acp.rs:11-30",
            "docs/diagram.png"
        ]);
    }

    #[test]
    fn successful_github_search_extracts_returned_sources() {
        let output = serde_json::json!({
            "items": [{
                "Json": {
                    "content": [{
                        "type": "text",
                        "text": "[{\"number\":42,\"html_url\":\"https://github.com/kiro-team/kiro-cli/issues/42\"}]"
                    }],
                    "isError": false
                }
            }]
        });

        assert_eq!(
            successful_tool_sources(
                Some("kiro-github-read"),
                "search_github_issues",
                &serde_json::json!({ "query": "shutdown" }),
                &output,
            ),
            vec![
                "github_issue:kiro-team/kiro-cli#42",
                "https://github.com/kiro-team/kiro-cli/issues/42"
            ]
        );
    }

    #[test]
    fn mcp_application_errors_authorize_no_sources() {
        let output = serde_json::json!({
            "items": [{
                "Json": {
                    "content": [{
                        "type": "text",
                        "text": "{\"taskId\":\"fabricated\"}"
                    }],
                    "isError": true
                }
            }]
        });

        assert!(
            successful_tool_sources(Some("kiro-mcp"), "Taskei___get_task", &serde_json::json!({}), &output,).is_empty()
        );
    }

    #[test]
    fn taskei_sources_come_from_returned_schema_ids_only() {
        let output = serde_json::json!({
            "items": [{
                "Json": {
                    "content": [{
                        "type": "text",
                        "text": "{\"tasks\":[{\"taskId\":\"task-123\",\"description\":\"{\\\"id\\\":\\\"injected\\\"}\"}]}"
                    }],
                    "isError": false
                }
            }]
        });

        assert_eq!(
            successful_tool_sources(Some("kiro-mcp"), "Taskei___list_tasks", &serde_json::json!({}), &output,),
            vec!["taskei:task-123"]
        );
    }

    #[test]
    fn cited_sources_are_intersected_before_rendering_and_persistence() {
        let reply = concat!(
            "The worker stays alive.\n\n",
            "Sources: `crates/kiro-bot/src/engine/acp.rs:840-875`, `docs/invented.md`, ",
            "[Issue 42](https://github.com/kiro-team/kiro-cli/issues/42)"
        );
        let validated = validate_cited_sources(reply, &[
            "crates/kiro-bot/src/engine/acp.rs".into(),
            "crates/kiro-bot/src/engine/acp.rs:800-900".into(),
            "https://github.com/kiro-team/kiro-cli/issues/42".into(),
        ]);

        assert_eq!(validated.sources, vec![
            "crates/kiro-bot/src/engine/acp.rs:840-875",
            "https://github.com/kiro-team/kiro-cli/issues/42"
        ]);
        assert!(validated.text.contains("`crates/kiro-bot/src/engine/acp.rs:840-875`"));
        assert!(
            validated
                .text
                .contains("[Issue 42](https://github.com/kiro-team/kiro-cli/issues/42)")
        );
        assert!(!validated.text.contains("docs/invented.md"));
    }

    #[test]
    fn line_citations_must_stay_inside_the_successful_read_scope() {
        let validated = validate_cited_sources(
            "Answer.\n\nSources: `src/lib.rs:12-18`, `src/lib.rs:99`, `https://example.com:80`",
            &[
                "src/lib.rs".into(),
                "src/lib.rs:10-20".into(),
                "https://example.com".into(),
            ],
        );

        assert_eq!(validated.sources, vec!["src/lib.rs:12-18"]);
        assert!(!validated.text.contains("src/lib.rs:99"));
        assert!(!validated.text.contains("https://example.com:80"));
    }

    #[test]
    fn source_line_is_removed_when_nothing_is_authorized() {
        let validated = validate_cited_sources("Answer.\n\nSources: `docs/prior-turn.md`, `docs/fabricated.md`", &[
            "docs/current-turn.md".into(),
        ]);

        assert_eq!(validated.text, "Answer.");
        assert!(validated.sources.is_empty());
    }
}
