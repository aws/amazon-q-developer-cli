//! GitHub Issues (and Releases) → `RawChunk` conversion. The HTTP fetch lives
//! in `main.rs`; this module is a pure data-shape mapper so it can be unit
//! tested with fixtures.

use anyhow::Context;
use chrono::{
    DateTime,
    Utc,
};
use serde::Deserialize;

use crate::{
    Partition,
    RawChunk,
};

/// Subset of fields the GitHub REST API returns for an issue. We accept extra
/// fields silently (no `deny_unknown_fields`) — the API surface is large.
#[derive(Debug, Deserialize)]
pub struct GithubIssue {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub state: String,
    pub updated_at: DateTime<Utc>,
    /// Present when the entry is actually a pull request — those are excluded.
    #[serde(default)]
    pub pull_request: Option<serde_json::Value>,
}

/// Minimal release shape — we feed it from `gh release view --json`.
#[derive(Debug, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    pub published_at: DateTime<Utc>,
}

/// Convert one issue into a `RawChunk`. Returns `None` when the entry is a PR
/// (PRs come through `/issues` because GitHub treats them as issues, but they
/// are not what we want in the docs corpus).
pub fn issue_to_chunk(repo: &str, issue: &GithubIssue) -> Option<RawChunk> {
    if issue.pull_request.is_some() {
        return None;
    }
    let path = format!("github_issue:{repo}#{}", issue.number);
    let mut content = format!(
        "# Issue #{n}: {title}\n\nState: {state}\n\n",
        n = issue.number,
        title = issue.title,
        state = issue.state,
    );
    if let Some(body) = &issue.body {
        content.push_str(body);
    }
    Some(RawChunk {
        source_path: path,
        content,
        last_modified: issue.updated_at,
        partition: Partition::Issues,
    })
}

pub fn release_to_chunk(repo: &str, release: &GithubRelease) -> RawChunk {
    let path = format!("github_release:{repo}@{tag}", tag = release.tag_name);
    let title = release.name.as_deref().unwrap_or(&release.tag_name);
    let mut content = format!("# Release {tag}: {title}\n\n", tag = release.tag_name);
    if let Some(body) = &release.body {
        content.push_str(body);
    }
    RawChunk {
        source_path: path,
        content,
        last_modified: release.published_at,
        partition: Partition::Releases,
    }
}

/// Parse a JSON list returned by `https://api.github.com/repos/<repo>/issues`
/// (or `gh api`) into chunks, dropping any pull requests in the response.
pub fn parse_issues(repo: &str, json: &str) -> anyhow::Result<Vec<RawChunk>> {
    let issues: Vec<GithubIssue> = serde_json::from_str(json).context("parsing GitHub issues JSON")?;
    Ok(issues.iter().filter_map(|i| issue_to_chunk(repo, i)).collect())
}

/// Parse a JSON list of releases into chunks.
pub fn parse_releases(repo: &str, json: &str) -> anyhow::Result<Vec<RawChunk>> {
    let releases: Vec<GithubRelease> = serde_json::from_str(json).context("parsing GitHub releases JSON")?;
    Ok(releases.iter().map(|r| release_to_chunk(repo, r)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISSUES_JSON: &str = r#"[
        {
            "number": 42,
            "title": "Login hangs on Linux",
            "body": "It just spins forever.",
            "state": "open",
            "updated_at": "2026-05-01T12:00:00Z"
        },
        {
            "number": 43,
            "title": "feat: add foo",
            "body": "PR body",
            "state": "open",
            "updated_at": "2026-05-02T12:00:00Z",
            "pull_request": { "url": "https://api.github.com/repos/x/y/pulls/43" }
        }
    ]"#;

    #[test]
    fn parses_issues_and_drops_pull_requests() {
        let chunks = parse_issues("kiro-team/kiro-cli", ISSUES_JSON).unwrap();
        assert_eq!(chunks.len(), 1);
        let c = &chunks[0];
        assert_eq!(c.partition, Partition::Issues);
        assert_eq!(c.source_path, "github_issue:kiro-team/kiro-cli#42");
        assert!(c.content.contains("Login hangs on Linux"));
        assert!(c.content.contains("State: open"));
        assert!(c.content.contains("It just spins forever."));
    }

    #[test]
    fn parses_releases() {
        let json = r#"[{
            "tag_name": "v2.4.0",
            "name": "v2.4.0",
            "body": "- big change",
            "published_at": "2026-05-15T18:00:00Z"
        }]"#;
        let chunks = parse_releases("kiro-team/kiro-cli", json).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].source_path, "github_release:kiro-team/kiro-cli@v2.4.0");
        assert_eq!(chunks[0].partition, Partition::Releases);
        assert!(chunks[0].content.contains("- big change"));
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_issues("a/b", "not json").unwrap_err();
        assert!(err.to_string().contains("parsing GitHub issues JSON"));
    }
}
