//! Core types for the kiro-github MCP server. Three tools:
//!
//! - `search_github_issues` — read-only: keyword search across issues + PRs.
//! - `create_github_issue`  — write: opens a new issue with the given title and body. Optional
//!   labels.
//! - `comment_on_existing`  — write: posts a comment on an existing issue or PR.
//!
//! The `GithubClient` trait lets tests stub out HTTP — the production impl
//! (`HttpGithubClient`) lives in [`crate::http_client`] and wraps `reqwest`.
//!
//! A token-bucket [`RateLimiter`] (default 30 calls/min per spec) is shared
//! across the three tools so a runaway loop can't blow our PAT quota.

pub mod http_client;
pub mod rate_limit;

use serde::{
    Deserialize,
    Serialize,
};

/// Reference to a GitHub repository in `owner/repo` form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRef {
    pub owner: String,
    pub repo: String,
}

impl RepoRef {
    pub fn parse(slug: &str) -> anyhow::Result<Self> {
        let (owner, repo) = slug
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("expected owner/repo, got {slug}"))?;
        if owner.is_empty() || repo.is_empty() {
            anyhow::bail!("expected non-empty owner/repo, got {slug}");
        }
        Ok(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }
}

/// Search result row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueSummary {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub html_url: String,
    /// Truncated to 240 chars to keep search payloads manageable for the LLM.
    pub body_excerpt: String,
}

/// Result of `create_github_issue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueCreated {
    pub number: u64,
    pub html_url: String,
}

/// Result of `comment_on_existing`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentCreated {
    pub html_url: String,
}

/// HTTP layer abstracted out of the MCP server so tests don't need a wiremock.
#[async_trait::async_trait]
pub trait GithubClient: Send + Sync {
    async fn search_issues(&self, repo: &RepoRef, query: &str, limit: u32) -> anyhow::Result<Vec<IssueSummary>>;
    async fn create_issue(
        &self,
        repo: &RepoRef,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> anyhow::Result<IssueCreated>;
    async fn create_comment(&self, repo: &RepoRef, issue_number: u64, body: &str) -> anyhow::Result<CommentCreated>;
}

/// Truncate `s` to at most `max_chars` Unicode chars, appending '…' if cut.
/// Used by [`IssueSummary`] body excerpts.
pub fn truncate_excerpt(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_ref_parses_owner_slash_repo() {
        let r = RepoRef::parse("kiro-team/kiro-cli").unwrap();
        assert_eq!(r.owner, "kiro-team");
        assert_eq!(r.repo, "kiro-cli");
    }

    #[test]
    fn repo_ref_rejects_garbage() {
        assert!(RepoRef::parse("no-slash").is_err());
        assert!(RepoRef::parse("/repo").is_err());
        assert!(RepoRef::parse("owner/").is_err());
        assert!(RepoRef::parse("").is_err());
    }

    #[test]
    fn truncate_excerpt_short_string_unchanged() {
        assert_eq!(truncate_excerpt("hello", 10), "hello");
    }

    #[test]
    fn truncate_excerpt_long_string_clipped_with_ellipsis() {
        let s = "a".repeat(100);
        let out = truncate_excerpt(&s, 10);
        assert_eq!(out.chars().count(), 11); // 10 + ellipsis
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_excerpt_preserves_unicode_boundaries() {
        // Should clip on character boundaries, not bytes.
        let s = "日本語日本語日本語";
        let out = truncate_excerpt(s, 3);
        assert_eq!(out, "日本語…");
    }
}
