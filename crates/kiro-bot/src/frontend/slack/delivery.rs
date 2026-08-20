use std::borrow::Cow;
use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    Ordering,
};
use std::sync::{
    LazyLock,
    Mutex,
};
use std::time::{
    Duration,
    SystemTime,
};

use async_trait::async_trait;
use regex::Regex;
use slack_morphism::errors::SlackClientError;
use slack_morphism::prelude::*;
use tracing::{
    error,
    warn,
};

use super::SlackFrontend;
use super::attachments::AttachmentFiles;
use super::events::log_thread_context_error;
use super::feedback::feedback_block;
use crate::engine::acp::{
    ProgressStatus,
    ProgressUpdate,
};
use crate::engine::core::{
    Frontend,
    GENAI_DISCLAIMER,
    Reply,
};

pub(super) const MARKDOWN_CHAR_LIMIT: usize = 11_500;
pub(super) const MAX_FENCE_HEADER_CHARS: usize = 128;
pub(super) const FALLBACK_CHAR_LIMIT: usize = 4_000;
const FALLBACK_SOURCE_CHAR_LIMIT: usize = 1_000;
const DELIVERY_TTL: Duration = Duration::from_secs(30 * 60);
const DELIVERY_RETRY_ATTEMPTS: usize = 4;
const DELIVERY_RETRY_BASE: Duration = Duration::from_millis(200);
pub(super) const DELIVERY_RETRY_MAX_DELAY: Duration = Duration::from_secs(60);
pub(super) const PROGRESS_ACK: &str = "_Looking into it..._";
pub(super) const RESPONSE_TASK_ID: &str = "kiro-response";
pub(super) const DISCLAIMER_CONTEXT: &str = "AI-generated; verify before acting.";

static CORRELATION_COUNTER: AtomicU64 = AtomicU64::new(1);
static WARNED_MESSAGES_TAB_DISABLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub(super) struct RenderedMessage {
    pub(super) chunks: Vec<SlackMessageContent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SlackStreamTaskStatus {
    Pending,
    InProgress,
    Complete,
    Error,
}

impl From<ProgressStatus> for SlackStreamTaskStatus {
    fn from(status: ProgressStatus) -> Self {
        match status {
            ProgressStatus::Pending => Self::Pending,
            ProgressStatus::InProgress => Self::InProgress,
            ProgressStatus::Complete => Self::Complete,
            ProgressStatus::Error => Self::Error,
        }
    }
}

impl SlackStreamTaskStatus {
    fn terminal(self, outcome: ProgressStatus) -> Self {
        match self {
            Self::Complete | Self::Error => self,
            Self::Pending | Self::InProgress => outcome.into(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum SlackStreamChunk {
    TaskUpdate {
        id: String,
        title: String,
        status: SlackStreamTaskStatus,
    },
    MarkdownText {
        text: String,
    },
}

#[derive(Debug, serde::Serialize)]
pub(super) struct SlackStartStreamRequest {
    channel: String,
    thread_ts: String,
    recipient_user_id: String,
    recipient_team_id: String,
    task_display_mode: &'static str,
    chunks: Vec<SlackStreamChunk>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct SlackAppendStreamRequest {
    pub(super) channel: String,
    pub(super) ts: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) markdown_text: Option<String>,
    pub(super) chunks: Vec<SlackStreamChunk>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct SlackStopStreamRequest {
    channel: String,
    ts: String,
    chunks: Vec<SlackStreamChunk>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blocks: Vec<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct SlackStopStreamFallbackRequest {
    channel: String,
    ts: String,
    markdown_text: String,
    chunks: Vec<SlackStreamChunk>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blocks: Vec<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
struct SlackStreamResponse {
    ts: SlackTs,
}

fn response_task(status: SlackStreamTaskStatus) -> SlackStreamChunk {
    SlackStreamChunk::TaskUpdate {
        id: RESPONSE_TASK_ID.into(),
        title: "Working on your request".into(),
        status,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SemanticPlanPhase {
    Context,
    Work,
    Verification,
}

impl SemanticPlanPhase {
    fn id(self) -> &'static str {
        match self {
            Self::Context => "kiro-context",
            Self::Work => "kiro-work",
            Self::Verification => "kiro-verification",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Context => "Gathering context",
            Self::Work => "Working through the request",
            Self::Verification => "Checking the result",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TrackedProgressTask {
    phase: SemanticPlanPhase,
    status: SlackStreamTaskStatus,
}

#[derive(Debug, Default)]
struct StreamPlan {
    tasks: HashMap<String, TrackedProgressTask>,
    phase_order: Vec<SemanticPlanPhase>,
}

impl StreamPlan {
    fn record(&mut self, update: ProgressUpdate) -> SlackStreamChunk {
        let phase = self
            .tasks
            .get(&update.id)
            .map(|task| task.phase)
            .unwrap_or_else(|| semantic_phase(&update.title));
        if !self.phase_order.contains(&phase) {
            self.phase_order.push(phase);
        }
        self.tasks.insert(update.id, TrackedProgressTask {
            phase,
            status: update.status.into(),
        });
        self.phase_chunk(phase, self.phase_status(phase))
    }

    fn terminal_chunks(&self, outcome: ProgressStatus) -> Vec<SlackStreamChunk> {
        self.phase_order
            .iter()
            .map(|phase| self.phase_chunk(*phase, self.terminal_phase_status(*phase, outcome)))
            .collect()
    }

    fn phase_status(&self, phase: SemanticPlanPhase) -> SlackStreamTaskStatus {
        let statuses = self
            .tasks
            .values()
            .filter(|task| task.phase == phase)
            .map(|task| task.status);
        let mut aggregate = SlackStreamTaskStatus::Complete;
        for status in statuses {
            match status {
                SlackStreamTaskStatus::InProgress => return SlackStreamTaskStatus::InProgress,
                SlackStreamTaskStatus::Pending => aggregate = SlackStreamTaskStatus::Pending,
                SlackStreamTaskStatus::Error if aggregate == SlackStreamTaskStatus::Complete => {
                    aggregate = SlackStreamTaskStatus::Error;
                },
                SlackStreamTaskStatus::Complete | SlackStreamTaskStatus::Error => {},
            }
        }
        aggregate
    }

    fn terminal_phase_status(&self, phase: SemanticPlanPhase, outcome: ProgressStatus) -> SlackStreamTaskStatus {
        let mut has_unfinished = false;
        for status in self
            .tasks
            .values()
            .filter(|task| task.phase == phase)
            .map(|task| task.status)
        {
            match status {
                SlackStreamTaskStatus::Error => return SlackStreamTaskStatus::Error,
                SlackStreamTaskStatus::Pending | SlackStreamTaskStatus::InProgress => has_unfinished = true,
                SlackStreamTaskStatus::Complete => {},
            }
        }
        if has_unfinished {
            SlackStreamTaskStatus::InProgress.terminal(outcome)
        } else {
            SlackStreamTaskStatus::Complete
        }
    }

    fn phase_chunk(&self, phase: SemanticPlanPhase, status: SlackStreamTaskStatus) -> SlackStreamChunk {
        SlackStreamChunk::TaskUpdate {
            id: phase.id().into(),
            title: phase.title().into(),
            status,
        }
    }
}

fn semantic_phase(title: &str) -> SemanticPlanPhase {
    let words = title
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if words.iter().any(|word| {
        matches!(
            word.as_str(),
            "read"
                | "reading"
                | "search"
                | "searching"
                | "fetch"
                | "fetching"
                | "find"
                | "finding"
                | "inspect"
                | "inspecting"
                | "list"
                | "listing"
                | "query"
                | "querying"
                | "lookup"
                | "browse"
                | "browsing"
                | "load"
                | "loading"
                | "retrieve"
                | "retrieving"
        )
    }) {
        return SemanticPlanPhase::Context;
    }
    if words.iter().any(|word| {
        matches!(
            word.as_str(),
            "test"
                | "tests"
                | "testing"
                | "check"
                | "checks"
                | "checking"
                | "verify"
                | "verifying"
                | "validate"
                | "validating"
                | "build"
                | "builds"
                | "building"
                | "compile"
                | "compiling"
                | "lint"
                | "linting"
                | "format"
                | "formatting"
        )
    }) {
        return SemanticPlanPhase::Verification;
    }
    SemanticPlanPhase::Work
}

fn terminal_progress_tasks(message_id: &str, outcome: ProgressStatus) -> Vec<SlackStreamChunk> {
    DELIVERIES
        .lock()
        .unwrap()
        .stream_plans
        .get(message_id)
        .map(|plan| plan.terminal_chunks(outcome))
        .unwrap_or_default()
}

fn finish_stream_tracking(message_id: &str) {
    DELIVERIES.lock().unwrap().stream_plans.remove(message_id);
}

fn track_stream(message_id: String) {
    DELIVERIES
        .lock()
        .unwrap()
        .stream_plans
        .insert(message_id.clone(), StreamPlan::default());
    tokio::spawn(async move {
        tokio::time::sleep(DELIVERY_TTL).await;
        DELIVERIES.lock().unwrap().stream_plans.remove(&message_id);
    });
}

fn track_progress(message_id: &str, update: ProgressUpdate) -> SlackStreamChunk {
    let mut deliveries = DELIVERIES.lock().unwrap();
    let plan = deliveries.stream_plans.entry(message_id.to_string()).or_default();
    plan.record(update)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct DeliveryKey {
    pub(super) channel: String,
    pub(super) thread_ts: Option<String>,
}

impl DeliveryKey {
    pub(super) fn new(channel: String, thread_ts: Option<String>) -> Self {
        Self { channel, thread_ts }
    }
}

#[derive(Default)]
pub(super) struct DeliveryTracker {
    acknowledgements: HashMap<String, DeliveryKey>,
    pub(super) replacements: HashMap<DeliveryKey, String>,
    updates_in_flight: HashSet<DeliveryKey>,
    completed: HashMap<String, DeliveryKey>,
    attachments: HashMap<DeliveryKey, Vec<AttachmentFiles>>,
    citations: HashMap<String, Vec<String>>,
    pub(super) citation_order: VecDeque<String>,
    stream_plans: HashMap<String, StreamPlan>,
}

impl DeliveryTracker {
    pub(super) fn record_ack(&mut self, message_id: String, key: DeliveryKey) {
        self.acknowledgements.insert(message_id, key);
    }

    pub(super) fn attach(&mut self, key: DeliveryKey, files: AttachmentFiles) {
        self.attachments.entry(key).or_default().push(files);
    }

    pub(super) fn complete(&mut self, message_id: &str) -> bool {
        let Some(key) = self.acknowledgements.remove(message_id) else {
            return false;
        };
        self.attachments.remove(&key);
        self.completed.insert(message_id.to_string(), key.clone());
        self.replacements.insert(key, message_id.to_string());
        true
    }

    pub(super) fn begin_update(&mut self, key: &DeliveryKey) -> std::result::Result<Option<String>, ()> {
        let Some(message_id) = self.replacements.get(key).cloned() else {
            return Ok(None);
        };
        if !self.updates_in_flight.insert(key.clone()) {
            return Err(());
        }
        Ok(Some(message_id))
    }

    pub(super) fn finish_update(&mut self, key: &DeliveryKey, message_id: &str) {
        self.updates_in_flight.remove(key);
        if self
            .replacements
            .get(key)
            .is_some_and(|replacement| replacement == message_id)
        {
            self.replacements.remove(key);
        }
    }

    pub(super) fn fail_update(&mut self, key: &DeliveryKey) {
        self.updates_in_flight.remove(key);
    }

    pub(super) fn is_completed(&self, message_id: &str) -> bool {
        self.completed.contains_key(message_id)
    }

    pub(super) fn expire_delivery(&mut self, message_id: &str) {
        if let Some(key) = self.acknowledgements.remove(message_id)
            && !self.acknowledgements.values().any(|active| active == &key)
        {
            self.attachments.remove(&key);
        }
        if let Some(key) = self.completed.remove(message_id)
            && self
                .replacements
                .get(&key)
                .is_some_and(|replacement| replacement == message_id)
        {
            self.replacements.remove(&key);
            self.updates_in_flight.remove(&key);
        }
    }

    pub(super) fn expire_citations(&mut self, message_id: &str) {
        if self.citations.remove(message_id).is_some() {
            self.citation_order.retain(|active| active != message_id);
        }
    }

    pub(super) fn expire_key(&mut self, key: &DeliveryKey) {
        self.attachments.remove(key);
    }

    fn release_attachments(&mut self, key: &DeliveryKey) {
        self.attachments.remove(key);
    }

    pub(super) fn record_citations(&mut self, message_id: String, citations: Vec<String>) {
        if citations.is_empty() {
            return;
        }
        if self.citations.contains_key(&message_id) {
            self.citation_order.retain(|active| active != &message_id);
        } else if self.citations.len() >= 1_000
            && let Some(oldest) = self.citation_order.pop_front()
        {
            self.citations.remove(&oldest);
        }
        self.citations.insert(message_id.clone(), citations);
        self.citation_order.push_back(message_id);
    }

    pub(super) fn citations(&self, message_id: &str) -> Vec<String> {
        self.citations.get(message_id).cloned().unwrap_or_default()
    }
}

pub(super) static DELIVERIES: LazyLock<Mutex<DeliveryTracker>> =
    LazyLock::new(|| Mutex::new(DeliveryTracker::default()));

pub(super) fn correlation_id() -> String {
    let millis = SystemTime::UNIX_EPOCH.elapsed().unwrap_or_default().as_millis();
    let sequence = CORRELATION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("KB-{millis:x}-{sequence:x}")
}

fn user_safe_text(text: &str) -> Cow<'_, str> {
    let trimmed = text.trim();
    if trimmed == "Error" || trimmed.starts_with("Error: Internal error:") || trimmed.starts_with("Error: no response")
    {
        let reference = correlation_id();
        error!(
            correlation_id = reference,
            internal_error = trimmed,
            "Kiro bot request failed"
        );
        return Cow::Owned(format!(
            "I hit an internal error while handling that request. Please try again.\n\nReference: `{reference}`"
        ));
    }
    Cow::Borrowed(text)
}

fn strip_thinking_blocks(text: &str) -> String {
    let mut remaining = text;
    let mut visible = String::with_capacity(text.len());
    while let Some(start) = remaining.find("<thinking>") {
        visible.push_str(&remaining[..start]);
        let after_start = &remaining[start + "<thinking>".len()..];
        let Some(end) = after_start.find("</thinking>") else {
            remaining = "";
            break;
        };
        remaining = &after_start[end + "</thinking>".len()..];
    }
    visible.push_str(remaining);
    visible.trim_start().to_string()
}

pub(super) fn neutralize_slack_controls(text: &str) -> String {
    text.replace("<@", "&lt;@")
        .replace("<!", "&lt;!")
        .replace("<#", "&lt;#")
}

fn split_sources_and_disclaimer(text: &str) -> (String, Option<String>, bool) {
    let mut body = text.trim().to_string();
    let mut has_disclaimer = false;
    if body.ends_with(GENAI_DISCLAIMER) {
        body.truncate(body.len() - GENAI_DISCLAIMER.len());
        body = body.trim_end().to_string();
        has_disclaimer = true;
    }

    let source_start = if body.starts_with("Sources:") {
        Some(0)
    } else {
        body.rfind("\nSources:").map(|index| index + 1)
    };
    let sources = source_start.map(|index| {
        let source_text = body[index..]
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        body.truncate(index);
        source_text
    });
    (body.trim().to_string(), sources, has_disclaimer)
}

pub(super) fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn split_at_char(text: &str, max_chars: usize) -> (&str, &str) {
    if char_len(text) <= max_chars {
        return (text, "");
    }
    let mut boundary = text
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    if let Some(relative) = text[..boundary].rfind(char::is_whitespace)
        && relative > boundary / 2
    {
        boundary = relative + text[relative..].chars().next().map(char::len_utf8).unwrap_or(0);
    }
    text.split_at(boundary)
}

fn push_chunk(chunks: &mut Vec<String>, current: &mut String) {
    let chunk = current.trim_end();
    if !chunk.is_empty() {
        chunks.push(chunk.to_string());
    }
    current.clear();
}

fn push_exact_chunk(chunks: &mut Vec<String>, current: &mut String) {
    let chunk = std::mem::take(current);
    if !chunk.trim().is_empty() {
        chunks.push(chunk);
    }
}

fn push_exact_text(chunks: &mut Vec<String>, current: &mut String, mut text: &str, limit: usize) {
    while !text.is_empty() {
        let available = limit.saturating_sub(char_len(current));
        if available == 0 {
            push_exact_chunk(chunks, current);
            continue;
        }
        let (piece, rest) = split_at_char(text, available);
        current.push_str(piece);
        text = rest;
        if !text.is_empty() {
            push_exact_chunk(chunks, current);
        }
    }
}

fn push_fenced_content(chunks: &mut Vec<String>, current: &mut String, header: &str, content: &str, limit: usize) {
    let reserved = char_len("\n```");
    let mut rest = content;
    while !rest.is_empty() {
        let available = limit.saturating_sub(char_len(current) + reserved);
        if available == 0 {
            current.push_str("\n```");
            push_chunk(chunks, current);
            current.push_str(header);
            current.push('\n');
            continue;
        }
        let (piece, next) = split_at_char(rest, available);
        current.push_str(piece);
        rest = next;
        if !rest.is_empty() {
            current.push_str("\n```");
            push_chunk(chunks, current);
            current.push_str(header);
            current.push('\n');
        }
    }
}

fn push_oversized_self_closing_fences(
    chunks: &mut Vec<String>,
    current: &mut String,
    line: &str,
    limit: usize,
) -> bool {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let fences = line.match_indices("```").map(|(index, _)| index).collect::<Vec<_>>();
    let Some(&first_fence) = fences.first() else {
        return false;
    };
    if fences.len() % 2 != 0 || !line[..first_fence].chars().all(char::is_whitespace) {
        return false;
    }

    push_chunk(chunks, current);
    let mut cursor = 0;
    for pair in fences.chunks_exact(2) {
        push_exact_text(chunks, current, &line[cursor..pair[0]], limit);
        let pair_end = pair[1] + 3;
        let raw = &line[pair[0]..pair_end];
        if char_len(raw) <= limit {
            if char_len(current) + char_len(raw) > limit {
                push_exact_chunk(chunks, current);
            }
            current.push_str(raw);
        } else {
            push_exact_chunk(chunks, current);
            let mut content = &line[pair[0] + 3..pair[1]];
            let available = limit.saturating_sub(char_len("```\n\n```"));
            while !content.is_empty() {
                let (piece, rest) = split_at_char(content, available);
                chunks.push(format!("```\n{piece}\n```"));
                content = rest;
            }
        }
        cursor = pair_end;
    }
    push_exact_text(chunks, current, &line[cursor..], limit);
    push_exact_chunk(chunks, current);
    true
}

pub(super) fn split_markdown(text: &str, limit: usize) -> Vec<String> {
    assert!(limit > 8, "markdown chunk limit must allow fenced content");
    if char_len(text) <= limit {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut fence_header: Option<String> = None;

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let fence_count = trimmed.match_indices("```").count();
        let is_fence = trimmed.starts_with("```") && fence_count == 1;
        if fence_header.is_none() {
            if fence_count > 1
                && char_len(line) > limit
                && push_oversized_self_closing_fences(&mut chunks, &mut current, line, limit)
            {
                continue;
            }
            if is_fence {
                let reserved = char_len("\n```");
                let original_header = trimmed.trim_end();
                let max_header = limit.saturating_sub(reserved + 2);
                let header_is_oversized = char_len(original_header) > max_header.min(MAX_FENCE_HEADER_CHARS);
                let header = if header_is_oversized { "```" } else { original_header };
                if char_len(&current) + char_len(header) + 1 + reserved > limit {
                    push_chunk(&mut chunks, &mut current);
                }
                current.push_str(header);
                current.push('\n');
                fence_header = Some(header.to_string());
                if header_is_oversized {
                    let info = original_header.strip_prefix("```").unwrap_or(original_header);
                    let content = format!("{info}\n");
                    push_fenced_content(&mut chunks, &mut current, header, &content, limit);
                }
                continue;
            }
            if char_len(&current) + char_len(line) > limit {
                push_chunk(&mut chunks, &mut current);
            }
            if char_len(line) <= limit {
                current.push_str(line);
            } else {
                let mut rest = line;
                while !rest.is_empty() {
                    let (piece, next) = split_at_char(rest, limit);
                    current.push_str(piece);
                    rest = next;
                    if !rest.is_empty() {
                        push_chunk(&mut chunks, &mut current);
                    }
                }
            }
            continue;
        }

        if is_fence {
            current.push_str("```");
            if line.ends_with('\n') {
                current.push('\n');
            }
            fence_header = None;
            continue;
        }

        let header = fence_header.as_ref().unwrap();
        push_fenced_content(&mut chunks, &mut current, header, line, limit);
    }

    if fence_header.is_some() {
        current.push_str("\n```");
    }
    push_chunk(&mut chunks, &mut current);
    chunks
}

pub(super) fn truncate_chars(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

fn plain_fallback_text(markdown: &str) -> String {
    let plain = markdown
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("```"))
        .map(|line| {
            line.trim_start_matches(['#', '-', '*', '>', ' '])
                .replace(['`', '*', '_', '~'], "")
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let plain = if plain.is_empty() {
        "Kiro Help response"
    } else {
        plain.as_str()
    };
    plain.to_string()
}

fn fallback_text(markdown: &str, sources: Option<&str>, has_disclaimer: bool) -> String {
    let mut details = Vec::new();
    if let Some(sources) = sources {
        details.push(truncate_chars(
            &plain_fallback_text(sources),
            FALLBACK_SOURCE_CHAR_LIMIT,
        ));
    }
    if has_disclaimer {
        details.push(DISCLAIMER_CONTEXT.to_string());
    }
    let details = details.join("\n");
    let separator_chars = usize::from(!details.is_empty());
    let body_limit = FALLBACK_CHAR_LIMIT.saturating_sub(char_len(&details) + separator_chars);
    let mut fallback = truncate_chars(&plain_fallback_text(markdown), body_limit);
    if !details.is_empty() {
        fallback.push('\n');
        fallback.push_str(&details);
    }
    fallback
}

/// Recover the readable text of a message we previously posted, walking the raw
/// block JSON by `type` tag rather than a typed enum.
///
/// This must stay untyped. `slack-morphism`'s `SlackBlock` is an internally
/// tagged enum with no catch-all variant, and the bot writes two block types it
/// cannot represent: the `plan` card (no such variant at all) and
/// `context_actions` carrying `feedback_buttons` (the crate expects
/// `positive`/`negative`, Slack sends `positive_button`/`negative_button`).
/// Either one makes serde reject the *whole* enclosing payload, so reading our
/// own messages back through the typed model is not possible.
///
/// `fallback` is the message's top-level `text`, which Slack always populates
/// and which the bot already writes as the accessibility fallback. Any block
/// tag we don't understand degrades to that, which covers future card chrome
/// without needing to model it.
pub(super) fn context_message_text(fallback: Option<&str>, blocks: Option<&[serde_json::Value]>) -> Option<String> {
    let mut answer = Vec::new();
    let mut metadata = Vec::new();
    // Set when any tag was not understood, at block *or* inline level. Partial
    // recovery is worse than the fallback, because it silently drops content.
    let mut lost_content = false;
    for block in blocks.into_iter().flatten() {
        match block.get("type").and_then(serde_json::Value::as_str) {
            Some("markdown") => match block.get("text").and_then(serde_json::Value::as_str) {
                Some(text) => answer.push(text.to_string()),
                None => lost_content = true,
            },
            Some("rich_text") => match rich_text_block_text(block, &mut lost_content) {
                Some(text) if !text.is_empty() => answer.push(text),
                Some(_) => {},
                None => lost_content = true,
            },
            Some("context") => metadata.extend(context_block_text(block)),
            // Card chrome the bot writes itself: progress plan and the feedback
            // buttons. Neither carries answer text, so both are simply skipped.
            Some("plan" | "context_actions") => {},
            _ => lost_content = true,
        }
    }
    // Either nothing recognisable carried the answer, or we only recovered part
    // of it — the accessibility fallback is then the faithful source.
    if answer.is_empty() || lost_content {
        if let Some(fallback) = fallback.filter(|text| !text.is_empty()) {
            return Some(fallback.to_string());
        }
        if answer.is_empty() {
            return None;
        }
    }
    answer.extend(metadata);
    Some(answer.join("\n"))
}

fn context_block_text(block: &serde_json::Value) -> Vec<String> {
    block
        .get("elements")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|element| {
            matches!(
                element.get("type").and_then(serde_json::Value::as_str),
                Some("plain_text" | "mrkdwn")
            )
        })
        .filter_map(|element| element.get("text").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect()
}

fn rich_text_block_text(block: &serde_json::Value, lost_content: &mut bool) -> Option<String> {
    let elements = block.get("elements")?.as_array()?;
    Some(
        elements
            .iter()
            .map(|element| rich_text_element_text(element, lost_content))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn rich_text_element_text(element: &serde_json::Value, lost_content: &mut bool) -> String {
    let mut inline = |key| match element.get(key).and_then(serde_json::Value::as_array) {
        Some(elements) => rich_text_inline_text(elements, lost_content),
        None => {
            *lost_content = true;
            String::new()
        },
    };
    match element.get("type").and_then(serde_json::Value::as_str) {
        Some("rich_text_section") => inline("elements"),
        Some("rich_text_preformatted") => format!("```\n{}\n```", inline("elements")),
        Some("rich_text_quote") => inline("elements")
            .lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Some("rich_text_list") => rich_text_list_text(element, lost_content),
        _ => {
            *lost_content = true;
            String::new()
        },
    }
}

fn rich_text_list_text(element: &serde_json::Value, lost_content: &mut bool) -> String {
    let Some(items) = element.get("elements").and_then(serde_json::Value::as_array) else {
        *lost_content = true;
        return String::new();
    };
    let ordered = element.get("style").and_then(serde_json::Value::as_str) == Some("ordered");
    let indent = "  ".repeat(
        element
            .get("indent")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default() as usize,
    );
    let offset = element
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default();
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if ordered {
                format!("{}.", offset + index as u64 + 1)
            } else {
                "-".to_string()
            };
            format!("{indent}{marker} {}", rich_text_element_text(item, lost_content))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rich_text_inline_text(elements: &[serde_json::Value], lost_content: &mut bool) -> String {
    elements
        .iter()
        .map(|element| {
            let text = |key: &str| element.get(key).and_then(serde_json::Value::as_str);
            let rendered = match element.get("type").and_then(serde_json::Value::as_str) {
                Some("text") => text("text").map(str::to_string),
                Some("link") => text("url").map(|url| match text("text") {
                    Some(label) => format!("<{url}|{label}>"),
                    None => url.to_string(),
                }),
                Some("user") => text("user_id").map(|user| format!("<@{user}>")),
                Some("channel") => text("channel_id").map(|channel| format!("<#{channel}>")),
                Some("usergroup") => text("usergroup_id").map(|group| format!("<!subteam^{group}>")),
                Some("emoji") => text("name").map(|name| format!(":{name}:")),
                Some("broadcast") => text("range").map(|range| format!("@{range}")),
                _ => None,
            };
            match rendered {
                Some(text) => apply_rich_text_style(text, element.get("style")),
                None => {
                    *lost_content = true;
                    String::new()
                },
            }
        })
        .collect()
}

fn apply_rich_text_style(text: String, style: Option<&serde_json::Value>) -> String {
    let Some(style) = style else {
        return text;
    };
    let enabled = |key| style.get(key).and_then(serde_json::Value::as_bool) == Some(true);
    if enabled("code") {
        return format!("`{text}`");
    }
    [("bold", "*"), ("italic", "_"), ("strike", "~")]
        .into_iter()
        .filter(|(key, _)| enabled(key))
        .fold(text, |text, (_, marker)| format!("{marker}{text}{marker}"))
}

fn slack_context_markdown(text: &str) -> String {
    static LINK_PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\[([^\]\|]+)\]\((https?://(?:[^\s()]|\([^\s()]*\))+)\)").unwrap());
    let safe = neutralize_slack_controls(text);
    LINK_PATTERN.replace_all(&safe, "<$2|$1>").into_owned()
}

pub(super) fn render_message(text: &str) -> RenderedMessage {
    let safe = user_safe_text(text);
    let visible = strip_thinking_blocks(&safe);
    let (body, sources, has_disclaimer) = split_sources_and_disclaimer(&visible);
    let body = neutralize_slack_controls(if body.is_empty() { "No response." } else { &body });
    let mut markdown_chunks = split_markdown(&body, MARKDOWN_CHAR_LIMIT);
    if markdown_chunks.is_empty() {
        markdown_chunks.push("No response.".to_string());
    }

    let last = markdown_chunks.len() - 1;
    let chunks = markdown_chunks
        .into_iter()
        .enumerate()
        .map(|(index, markdown)| {
            let mut blocks = vec![SlackBlock::Markdown(SlackMarkdownBlock {
                block_id: None,
                text: markdown.clone(),
            })];
            if index == last {
                let mut context_elements = Vec::new();
                if let Some(sources) = sources.as_deref() {
                    context_elements.push(SlackContextBlockElement::MarkDown(SlackBlockMarkDownText {
                        text: truncate_chars(&slack_context_markdown(sources), 2_500),
                        verbatim: Some(true),
                    }));
                }
                if has_disclaimer {
                    context_elements.push(SlackContextBlockElement::MarkDown(SlackBlockMarkDownText {
                        text: DISCLAIMER_CONTEXT.into(),
                        verbatim: Some(true),
                    }));
                }
                if !context_elements.is_empty() {
                    blocks.push(SlackBlock::Context(SlackContextBlock {
                        block_id: None,
                        elements: context_elements,
                    }));
                }
            }
            SlackMessageContent::new()
                .with_text(fallback_text(
                    &markdown,
                    (index == last).then_some(sources.as_deref()).flatten(),
                    index == last && has_disclaimer,
                ))
                .with_blocks(blocks)
        })
        .collect();
    RenderedMessage { chunks }
}

#[derive(Debug, Clone)]
pub(super) struct DegradedFinalChunk {
    pub(super) content: SlackMessageContent,
    pub(super) feedback_enabled: bool,
}

pub(super) fn degraded_final_chunks(text: &str, feedback_enabled: bool) -> VecDeque<DegradedFinalChunk> {
    let mut chunks = render_message(text)
        .chunks
        .into_iter()
        .map(|content| DegradedFinalChunk {
            content,
            feedback_enabled: false,
        })
        .collect::<VecDeque<_>>();
    if feedback_enabled && let Some(last) = chunks.back_mut() {
        last.feedback_enabled = true;
    }
    chunks
}

pub(super) fn request_with_feedback<T: serde::Serialize>(request: &T, feedback_enabled: bool) -> serde_json::Value {
    let mut request = serde_json::to_value(request).expect("Slack request is serializable");
    if feedback_enabled {
        let blocks = request
            .as_object_mut()
            .expect("Slack request is an object")
            .entry("blocks")
            .or_insert_with(|| serde_json::Value::Array(Vec::new()))
            .as_array_mut()
            .expect("Slack request blocks are an array");
        blocks.push(feedback_block());
    }
    request
}

pub(super) fn start_stream_request(
    channel: String,
    thread_ts: String,
    recipient_user_id: String,
    recipient_team_id: String,
) -> SlackStartStreamRequest {
    SlackStartStreamRequest {
        channel,
        thread_ts,
        recipient_user_id,
        recipient_team_id,
        task_display_mode: "plan",
        chunks: vec![response_task(SlackStreamTaskStatus::InProgress)],
    }
}

pub(super) fn stop_stream_request(
    channel: String,
    ts: String,
    text: &str,
    status: ProgressStatus,
    progress_chunks: Vec<SlackStreamChunk>,
    feedback_enabled: bool,
) -> SlackStopStreamRequest {
    let rendered = render_message(text);
    let mut chunks = vec![response_task(status.into())];
    chunks.extend(progress_chunks);
    let mut blocks = Vec::new();
    let content_count = rendered.chunks.len();
    for (index, content) in rendered.chunks.into_iter().enumerate() {
        let mut content_blocks = content.blocks.unwrap_or_default();
        let markdown_text = match content_blocks.first() {
            Some(SlackBlock::Markdown(markdown)) => markdown.text.clone(),
            _ => "No response.".into(),
        };
        chunks.push(SlackStreamChunk::MarkdownText { text: markdown_text });
        if index + 1 == content_count && matches!(content_blocks.first(), Some(SlackBlock::Markdown(_))) {
            content_blocks.remove(0);
            blocks = content_blocks
                .into_iter()
                .map(|block| serde_json::to_value(block).expect("Slack block is serializable"))
                .collect();
        }
    }
    if feedback_enabled {
        blocks.push(feedback_block());
    }
    SlackStopStreamRequest {
        channel,
        ts,
        chunks,
        blocks,
    }
}

pub(super) fn stop_stream_fallback_request(
    channel: String,
    ts: String,
    text: &str,
    status: ProgressStatus,
    progress_chunks: Vec<SlackStreamChunk>,
) -> Option<SlackStopStreamFallbackRequest> {
    let mut rendered = render_message(text);
    if rendered.chunks.len() != 1 {
        return None;
    }
    let content = rendered.chunks.remove(0);
    let mut blocks = content.blocks.unwrap_or_default();
    let markdown_text = match blocks.first() {
        Some(SlackBlock::Markdown(markdown)) => markdown.text.clone(),
        _ => "No response.".into(),
    };
    if matches!(blocks.first(), Some(SlackBlock::Markdown(_))) {
        blocks.remove(0);
    }
    let blocks = blocks
        .into_iter()
        .map(|block| serde_json::to_value(block).expect("Slack block is serializable"))
        .collect::<Vec<_>>();
    Some(SlackStopStreamFallbackRequest {
        channel,
        ts,
        markdown_text,
        chunks: {
            let mut chunks = vec![response_task(status.into())];
            chunks.extend(progress_chunks);
            chunks
        },
        blocks,
    })
}

pub(super) fn post_request(
    channel: SlackChannelId,
    content: SlackMessageContent,
    thread_ts: Option<String>,
) -> SlackApiChatPostMessageRequest {
    let mut request = SlackApiChatPostMessageRequest::new(channel, content);
    request.unfurl_links = Some(false);
    request.unfurl_media = Some(false);
    if let Some(ts) = thread_ts {
        request.thread_ts = Some(ts.into());
    }
    request
}

fn track_ack(message_id: String, key: DeliveryKey) {
    DELIVERIES.lock().unwrap().record_ack(message_id.clone(), key);
    tokio::spawn(async move {
        tokio::time::sleep(DELIVERY_TTL).await;
        DELIVERIES.lock().unwrap().expire_delivery(&message_id);
    });
}

pub(super) fn track_attachments(key: DeliveryKey, files: AttachmentFiles) {
    if files.paths.is_empty() {
        return;
    }
    DELIVERIES.lock().unwrap().attach(key.clone(), files);
    tokio::spawn(async move {
        tokio::time::sleep(DELIVERY_TTL).await;
        DELIVERIES.lock().unwrap().expire_key(&key);
    });
}

fn release_attachments(key: &DeliveryKey) {
    DELIVERIES.lock().unwrap().release_attachments(key);
}

fn track_citations(message_id: &str, text: &str) {
    let citations = crate::engine::feedback::extract_cited_sources(text);
    if citations.is_empty() {
        return;
    }
    let message_id = message_id.to_string();
    DELIVERIES
        .lock()
        .unwrap()
        .record_citations(message_id.clone(), citations);
    tokio::spawn(async move {
        tokio::time::sleep(DELIVERY_TTL).await;
        DELIVERIES.lock().unwrap().expire_citations(&message_id);
    });
}

fn track_all_citations(message_ids: &[String], text: &str) {
    for message_id in message_ids {
        track_citations(message_id, text);
    }
}

fn failed_delivery_content(reference: &str) -> SlackMessageContent {
    render_message(&format!(
        "I couldn't deliver the response. Please retry your request.\n\nReference: `{reference}`"
    ))
    .chunks
    .remove(0)
}

fn delivery_failure_is_definitive(error: &SlackClientError) -> bool {
    matches!(
        error,
        SlackClientError::ApiError(_) | SlackClientError::RateLimitError(_)
    ) || matches!(
        error,
        SlackClientError::HttpError(http)
            if http.status_code.is_client_error() && http.status_code != http::StatusCode::REQUEST_TIMEOUT
    )
}

pub(super) fn update_requires_new_message(error: &SlackClientError) -> bool {
    matches!(
        error,
        SlackClientError::ApiError(api)
            if matches!(
                api.code.as_str(),
                "message_not_found" | "cant_update_message" | "edit_window_closed"
            )
    )
}

pub(super) fn stream_start_can_fallback(error: &SlackClientError) -> bool {
    match error {
        SlackClientError::ApiError(api) => !matches!(api.code.as_str(), "fatal_error" | "internal_error"),
        SlackClientError::RateLimitError(_) => true,
        SlackClientError::HttpError(http) => {
            http.status_code.is_client_error() && http.status_code != http::StatusCode::REQUEST_TIMEOUT
        },
        _ => false,
    }
}

pub(super) fn log_post_error(error: &str, correlation_id: &str) {
    if error.contains("messages_tab_disabled") {
        if !WARNED_MESSAGES_TAB_DISABLED.swap(true, Ordering::Relaxed) {
            error!(
                correlation_id,
                "Slack rejected direct-message delivery: enable App Home > Messages Tab for the bot"
            );
        }
        return;
    }
    error!(correlation_id, slack_error = error, "Slack message delivery failed");
}

#[derive(Clone, Copy)]
pub(super) enum RetrySafety {
    Idempotent,
    PostMessage,
}

fn jittered_backoff(failed_attempt: usize) -> Duration {
    let backoff = DELIVERY_RETRY_BASE
        .saturating_mul(1_u32 << failed_attempt.saturating_sub(1).min(5))
        .min(DELIVERY_RETRY_MAX_DELAY);
    let jitter = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64
        % 41;
    backoff.mul_f64((80 + jitter) as f64 / 100.0)
}

pub(super) fn delivery_retry_delay(
    error: &SlackClientError,
    failed_attempt: usize,
    safety: RetrySafety,
) -> Option<Duration> {
    let backoff = jittered_backoff(failed_attempt);
    match error {
        SlackClientError::RateLimitError(rate_limit) => {
            let delay = rate_limit.retry_after.unwrap_or(backoff);
            (delay <= DELIVERY_RETRY_MAX_DELAY).then_some(delay)
        },
        SlackClientError::HttpError(http)
            if matches!(safety, RetrySafety::Idempotent)
                && (http.status_code.is_server_error() || http.status_code == http::StatusCode::REQUEST_TIMEOUT) =>
        {
            Some(backoff)
        },
        SlackClientError::HttpProtocolError(_)
        | SlackClientError::EndOfStream(_)
        | SlackClientError::SystemError(_)
            if matches!(safety, RetrySafety::Idempotent) =>
        {
            Some(backoff)
        },
        _ => None,
    }
}

async fn slack_api_with_retry<T, F, Fut>(operation: &'static str, safety: RetrySafety, mut call: F) -> ClientResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ClientResult<T>>,
{
    for attempt in 1..=DELIVERY_RETRY_ATTEMPTS {
        match call().await {
            Ok(response) => return Ok(response),
            Err(error) => {
                let Some(delay) = delivery_retry_delay(&error, attempt, safety) else {
                    return Err(error);
                };
                if attempt == DELIVERY_RETRY_ATTEMPTS {
                    return Err(error);
                }
                warn!(operation, attempt, ?delay, %error, "Retrying transient Slack API failure");
                tokio::time::sleep(delay).await;
            },
        }
    }
    unreachable!("delivery retry loop always returns")
}

#[async_trait]
impl Frontend for SlackFrontend {
    async fn send(&self, reply: Reply) -> anyhow::Result<String> {
        let session = self.client.open_session(&self.bot_token);
        match reply {
            Reply::StartProgress {
                conversation,
                reply_to,
                recipient_user_id,
                recipient_team_id,
            } => {
                let request = start_stream_request(
                    conversation.clone(),
                    reply_to.clone(),
                    recipient_user_id,
                    recipient_team_id,
                );
                match slack_api_with_retry("chat.startStream", RetrySafety::PostMessage, || {
                    session
                        .http_session_api
                        .http_post::<_, SlackStreamResponse>("chat.startStream", &request, None)
                })
                .await
                {
                    Ok(response) => {
                        let message_id = response.ts.to_string();
                        track_stream(message_id.clone());
                        Ok(message_id)
                    },
                    Err(error) if stream_start_can_fallback(&error) => {
                        warn!(%error, "Slack stream unavailable; using a threaded progress message");
                        let content = render_message(PROGRESS_ACK).chunks.remove(0);
                        let post = post_request(conversation.into(), content, Some(reply_to));
                        let response = slack_api_with_retry("chat.postMessage", RetrySafety::PostMessage, || {
                            session.chat_post_message(&post)
                        })
                        .await?;
                        let message_id = response.ts.to_string();
                        track_stream(message_id.clone());
                        Ok(message_id)
                    },
                    Err(error) => Err(error.into()),
                }
            },
            Reply::Progress {
                conversation,
                message_id,
                update,
            } => {
                let progress = track_progress(&message_id, update);
                let request = SlackAppendStreamRequest {
                    channel: conversation,
                    ts: message_id.clone(),
                    markdown_text: None,
                    chunks: vec![progress],
                };
                let result: ClientResult<SlackStreamResponse> =
                    slack_api_with_retry("chat.appendStream", RetrySafety::Idempotent, || {
                        session.http_session_api.http_post("chat.appendStream", &request, None)
                    })
                    .await;
                match result {
                    Ok(_) => Ok(message_id),
                    Err(SlackClientError::ApiError(error)) if error.code == "message_not_in_streaming_state" => {
                        Ok(message_id)
                    },
                    Err(error) => Err(error.into()),
                }
            },
            Reply::FinishProgress {
                conversation,
                message_id,
                reply_to,
                text,
                status,
            } => {
                release_attachments(&DeliveryKey::new(conversation.clone(), Some(reply_to.clone())));
                let progress_chunks = terminal_progress_tasks(&message_id, status);
                let request = stop_stream_request(
                    conversation.clone(),
                    message_id.clone(),
                    &text,
                    status,
                    progress_chunks.clone(),
                    self.feedback_enabled,
                );
                let result: ClientResult<SlackStreamResponse> =
                    slack_api_with_retry("chat.stopStream", RetrySafety::Idempotent, || {
                        session.http_session_api.http_post("chat.stopStream", &request, None)
                    })
                    .await;
                if result.is_ok() {
                    track_citations(&message_id, &text);
                    finish_stream_tracking(&message_id);
                    return Ok(message_id);
                }

                let stop_error = result.unwrap_err();
                warn!(%stop_error, %message_id, "Falling back after Slack stream finalization failed");
                if let Some(fallback) = stop_stream_fallback_request(
                    conversation.clone(),
                    message_id.clone(),
                    &text,
                    status,
                    progress_chunks,
                ) {
                    let fallback_result: ClientResult<SlackStreamResponse> =
                        slack_api_with_retry("chat.stopStream", RetrySafety::Idempotent, || {
                            session.http_session_api.http_post("chat.stopStream", &fallback, None)
                        })
                        .await;
                    if fallback_result.is_ok() {
                        track_citations(&message_id, &text);
                        finish_stream_tracking(&message_id);
                        return Ok(message_id);
                    }
                }

                let mut rendered = degraded_final_chunks(&text, self.feedback_enabled);
                let first = rendered.pop_front().expect("rendered response has at least one chunk");
                let update = request_with_feedback(
                    &SlackApiChatUpdateRequest::new(
                        conversation.clone().into(),
                        first.content.clone(),
                        message_id.clone().into(),
                    ),
                    first.feedback_enabled,
                );
                let mut delivered_ids = Vec::new();
                let update_result: ClientResult<SlackApiChatUpdateResponse> =
                    slack_api_with_retry("chat.update", RetrySafety::Idempotent, || {
                        session.http_session_api.http_post("chat.update", &update, None)
                    })
                    .await;
                if update_result.is_ok() {
                    delivered_ids.push(message_id.clone());
                } else {
                    rendered.push_front(first);
                }

                while let Some(chunk) = rendered.pop_front() {
                    let post = request_with_feedback(
                        &post_request(conversation.clone().into(), chunk.content, Some(reply_to.clone())),
                        chunk.feedback_enabled,
                    );
                    let response: SlackApiChatPostMessageResponse =
                        slack_api_with_retry("chat.postMessage", RetrySafety::PostMessage, || {
                            session.http_session_api.http_post("chat.postMessage", &post, None)
                        })
                        .await?;
                    delivered_ids.push(response.ts.to_string());
                }
                track_all_citations(&delivered_ids, &text);

                if !delivered_ids.iter().any(|id| id == &message_id) {
                    let delete = SlackApiChatDeleteRequest::new(conversation.into(), message_id.clone().into());
                    if let Err(error) =
                        slack_api_with_retry("chat.delete", RetrySafety::Idempotent, || session.chat_delete(&delete))
                            .await
                    {
                        warn!(%error, %message_id, "Failed to clean up unfinished Slack stream");
                    }
                }
                finish_stream_tracking(&message_id);
                delivered_ids
                    .last()
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Slack final response produced no messages"))
            },
            Reply::Send {
                conversation,
                reply_to,
                text,
            } => {
                let key = DeliveryKey::new(conversation.clone(), reply_to.clone());
                let mut chunks = render_message(&text).chunks;
                let first = chunks.remove(0);

                let replacement = {
                    DELIVERIES
                        .lock()
                        .unwrap()
                        .begin_update(&key)
                        .map_err(|()| anyhow::anyhow!("final Slack update is already in progress"))?
                };
                let mut delivered_ids = Vec::new();
                if let Some(message_id) = replacement {
                    let update = SlackApiChatUpdateRequest::new(
                        conversation.clone().into(),
                        first.clone(),
                        message_id.clone().into(),
                    );
                    if let Err(error) =
                        slack_api_with_retry("chat.update", RetrySafety::Idempotent, || session.chat_update(&update))
                            .await
                    {
                        let correlation = correlation_id();
                        log_post_error(&error.to_string(), &correlation);
                        if update_requires_new_message(&error) {
                            DELIVERIES.lock().unwrap().finish_update(&key, &message_id);
                            chunks.insert(0, first);
                        } else {
                            if delivery_failure_is_definitive(&error) {
                                let failure_update = SlackApiChatUpdateRequest::new(
                                    conversation.clone().into(),
                                    failed_delivery_content(&correlation),
                                    message_id.clone().into(),
                                );
                                match slack_api_with_retry("chat.update", RetrySafety::Idempotent, || {
                                    session.chat_update(&failure_update)
                                })
                                .await
                                {
                                    Ok(_) => DELIVERIES.lock().unwrap().finish_update(&key, &message_id),
                                    Err(failure_error) => {
                                        DELIVERIES.lock().unwrap().fail_update(&key);
                                        error!(
                                            correlation_id = correlation,
                                            %failure_error,
                                            "Failed to replace stale Slack progress message"
                                        );
                                    },
                                }
                            } else {
                                DELIVERIES.lock().unwrap().fail_update(&key);
                            }
                            return Err(error.into());
                        }
                    } else {
                        DELIVERIES.lock().unwrap().finish_update(&key, &message_id);
                        delivered_ids.push(message_id);
                    }
                } else {
                    chunks.insert(0, first);
                }

                let correlation = correlation_id();
                for content in chunks {
                    let request = post_request(conversation.clone().into(), content, reply_to.clone());
                    let response = slack_api_with_retry("chat.postMessage", RetrySafety::PostMessage, || {
                        session.chat_post_message(&request)
                    })
                    .await
                    .inspect_err(|error| log_post_error(&error.to_string(), &correlation))?;
                    delivered_ids.push(response.ts.to_string());
                }
                track_all_citations(&delivered_ids, &text);
                let message_id = delivered_ids
                    .last()
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Slack response produced no messages"))?;
                if text == PROGRESS_ACK {
                    track_ack(message_id.clone(), key);
                }
                Ok(message_id)
            },
            Reply::Update {
                conversation,
                message_id,
                text,
            } => {
                let completed = { DELIVERIES.lock().unwrap().is_completed(&message_id) };
                if completed {
                    return Ok(message_id);
                }
                let mut rendered = render_message(&text).chunks;
                anyhow::ensure!(
                    rendered.len() == 1,
                    "Slack message update exceeds the single-message update limit"
                );
                let request =
                    SlackApiChatUpdateRequest::new(conversation.into(), rendered.remove(0), message_id.clone().into());
                slack_api_with_retry("chat.update", RetrySafety::Idempotent, || session.chat_update(&request)).await?;
                Ok(message_id)
            },
            Reply::Delete {
                conversation,
                message_id,
            } => {
                let completed = { DELIVERIES.lock().unwrap().complete(&message_id) };
                if completed {
                    return Ok(message_id);
                }
                let request = SlackApiChatDeleteRequest::new(conversation.into(), message_id.clone().into());
                slack_api_with_retry("chat.delete", RetrySafety::Idempotent, || session.chat_delete(&request)).await?;
                Ok(message_id)
            },
        }
    }

    async fn fetch_context(&self, conversation: &str, before: &str, thread_ts: Option<&str>) -> Vec<String> {
        if self.conversation_history == 0 {
            return vec![];
        }
        let Some(ts) = thread_ts else {
            self.mark_seen(conversation, before);
            return vec![];
        };

        let result = match self.fetch_thread_snapshot(conversation, before, ts, &[], "").await {
            Ok(snapshot) => snapshot.context,
            Err(error) => {
                log_thread_context_error(&error);
                vec![]
            },
        };
        self.mark_seen(conversation, before);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_chunk_fields(chunk: &SlackStreamChunk) -> (&str, &str, SlackStreamTaskStatus) {
        match chunk {
            SlackStreamChunk::TaskUpdate { id, title, status } => (id, title, *status),
            SlackStreamChunk::MarkdownText { .. } => panic!("expected task update"),
        }
    }

    #[test]
    fn progress_uses_stable_semantic_phases_instead_of_raw_tool_titles() {
        let cases = [
            (
                "tool-read",
                "Reading crates/kiro-bot/src/frontend/slack.rs",
                "kiro-context",
                "Gathering context",
            ),
            (
                "tool-edit",
                "Editing the Slack renderer",
                "kiro-work",
                "Working through the request",
            ),
            (
                "tool-test",
                "Running cargo test",
                "kiro-verification",
                "Checking the result",
            ),
        ];

        for (id, raw_title, expected_id, expected_title) in cases {
            let mut plan = StreamPlan::default();
            let chunk = plan.record(ProgressUpdate {
                id: id.into(),
                title: raw_title.into(),
                status: ProgressStatus::InProgress,
            });
            let (actual_id, actual_title, status) = task_chunk_fields(&chunk);
            assert_eq!(actual_id, expected_id);
            assert_eq!(actual_title, expected_title);
            assert_eq!(status, SlackStreamTaskStatus::InProgress);
            assert!(!serde_json::to_string(&chunk).unwrap().contains(raw_title));
        }
    }

    #[test]
    fn concurrent_tools_keep_their_semantic_phase_in_progress() {
        let mut plan = StreamPlan::default();
        plan.record(ProgressUpdate {
            id: "read-1".into(),
            title: "Reading the implementation".into(),
            status: ProgressStatus::InProgress,
        });
        let chunk = plan.record(ProgressUpdate {
            id: "read-2".into(),
            title: "Searching the tests".into(),
            status: ProgressStatus::Pending,
        });

        assert_eq!(task_chunk_fields(&chunk).2, SlackStreamTaskStatus::InProgress);
    }

    #[test]
    fn finalization_retains_every_observed_phase_after_append_failures() {
        let mut plan = StreamPlan::default();
        for update in [
            ProgressUpdate {
                id: "read-1".into(),
                title: "Reading the implementation".into(),
                status: ProgressStatus::InProgress,
            },
            ProgressUpdate {
                id: "edit-1".into(),
                title: "Editing the renderer".into(),
                status: ProgressStatus::Complete,
            },
            ProgressUpdate {
                id: "test-1".into(),
                title: "Running focused tests".into(),
                status: ProgressStatus::Pending,
            },
        ] {
            plan.record(update);
        }
        let tasks = plan.terminal_chunks(ProgressStatus::Error);
        let fields = tasks.iter().map(task_chunk_fields).collect::<Vec<_>>();

        assert_eq!(fields, [
            ("kiro-context", "Gathering context", SlackStreamTaskStatus::Error),
            (
                "kiro-work",
                "Working through the request",
                SlackStreamTaskStatus::Complete
            ),
            ("kiro-verification", "Checking the result", SlackStreamTaskStatus::Error),
        ]);
        assert!(
            fields
                .iter()
                .all(|(_, _, status)| matches!(status, SlackStreamTaskStatus::Complete | SlackStreamTaskStatus::Error))
        );
    }

    #[test]
    fn successful_stop_terminalizes_unfinished_semantic_phase() {
        let mut plan = StreamPlan::default();
        plan.record(ProgressUpdate {
            id: "tool-1".into(),
            title: "Searching Kiro docs".into(),
            status: ProgressStatus::InProgress,
        });
        let chunk = plan.terminal_chunks(ProgressStatus::Complete).remove(0);

        assert_eq!(
            task_chunk_fields(&chunk),
            ("kiro-context", "Gathering context", SlackStreamTaskStatus::Complete)
        );
    }

    #[test]
    fn oversized_self_closing_fence_pairs_preserve_separator_text() {
        let first = "a".repeat(30);
        let second = "b".repeat(30);
        let reply = format!("```{first}``` and ```{second}```");
        let chunks = split_markdown(&reply, 40);

        assert!(chunks.len() > 1);
        assert_eq!(chunks.concat(), reply);
        for chunk in chunks {
            assert!(char_len(&chunk) <= 40);
            assert_eq!(chunk.match_indices("```").count() % 2, 0);
        }
    }

    #[test]
    fn oversized_self_closing_fences_do_not_emit_blank_chunks() {
        let content = "y".repeat(MARKDOWN_CHAR_LIMIT);
        let reply = format!("Intro line\n  ```{content}``` \nAfter the snippet.");
        let chunks = split_markdown(&reply, MARKDOWN_CHAR_LIMIT);

        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| !chunk.trim().is_empty()));
        assert!(chunks.iter().all(|chunk| char_len(chunk) <= MARKDOWN_CHAR_LIMIT));
        assert!(chunks.iter().all(|chunk| chunk.match_indices("```").count() % 2 == 0));
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.chars().filter(|character| *character == 'y').count())
                .sum::<usize>(),
            MARKDOWN_CHAR_LIMIT
        );
        assert!(chunks.iter().any(|chunk| chunk.contains("Intro line")));
        assert!(chunks.iter().any(|chunk| chunk.contains("After the snippet.")));
    }

    #[test]
    fn releasing_delivery_cleans_every_attachment_root_immediately() {
        let directory = tempfile::tempdir().unwrap();
        let first_root = directory.path().join("first");
        let second_root = directory.path().join("second");
        std::fs::create_dir(&first_root).unwrap();
        std::fs::create_dir(&second_root).unwrap();
        let first_file = first_root.join("first.txt");
        let second_file = second_root.join("second.txt");
        std::fs::write(&first_file, b"first").unwrap();
        std::fs::write(&second_file, b"second").unwrap();
        let key = DeliveryKey::new("C123".into(), Some("170.1".into()));
        let mut tracker = DeliveryTracker::default();
        tracker.attach(key.clone(), AttachmentFiles::for_test(first_root.clone(), first_file));
        tracker.attach(key.clone(), AttachmentFiles::for_test(second_root.clone(), second_file));

        tracker.release_attachments(&key);

        assert!(!first_root.exists());
        assert!(!second_root.exists());
    }
}
