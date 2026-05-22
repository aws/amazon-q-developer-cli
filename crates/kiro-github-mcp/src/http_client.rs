//! `reqwest`-based [`GithubClient`]. Lives separately so unit tests can stub
//! the trait without dragging the HTTP layer in.

use anyhow::{
    Context,
    bail,
};
use reqwest::header::{
    ACCEPT,
    AUTHORIZATION,
    USER_AGENT,
};
use serde::Deserialize;

use crate::{
    CommentCreated,
    GithubClient,
    IssueCreated,
    IssueSummary,
    RepoRef,
    truncate_excerpt,
};

const DEFAULT_BASE_URL: &str = "https://api.github.com";
const USER_AGENT_VAL: &str = "kiro-github-mcp/1.0";
const BODY_EXCERPT_CHARS: usize = 240;

pub struct HttpGithubClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl HttpGithubClient {
    pub fn new(token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            token,
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(USER_AGENT, USER_AGENT_VAL.parse().unwrap());
        h.insert(ACCEPT, "application/vnd.github+json".parse().unwrap());
        if let Some(t) = &self.token
            && let Ok(v) = format!("Bearer {t}").parse()
        {
            h.insert(AUTHORIZATION, v);
        }
        h
    }
}

#[async_trait::async_trait]
impl GithubClient for HttpGithubClient {
    async fn search_issues(&self, repo: &RepoRef, query: &str, limit: u32) -> anyhow::Result<Vec<IssueSummary>> {
        // GitHub's /search/issues uses a `q=` parameter that supports
        // `repo:<owner>/<repo>` qualifiers. Per_page caps results.
        let q_raw = format!("repo:{}/{} {}", repo.owner, repo.repo, query);
        let q = urlencode(&q_raw);
        let per_page = limit.min(30);
        let url = format!("{}/search/issues?q={q}&per_page={per_page}", self.base_url);
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("search_issues failed: {status}: {body}");
        }
        let body = resp.text().await.context("read search_issues body")?;
        parse_search_response(&body)
    }

    async fn create_issue(
        &self,
        repo: &RepoRef,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> anyhow::Result<IssueCreated> {
        let url = format!("{}/repos/{}/{}/issues", self.base_url, repo.owner, repo.repo);
        let payload = serde_json::json!({
            "title": title,
            "body": body,
            "labels": labels,
        });
        let resp = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("create_issue failed: {status}: {body}");
        }
        let body = resp.text().await.context("read create_issue body")?;
        parse_issue_created(&body)
    }

    async fn create_comment(&self, repo: &RepoRef, issue_number: u64, body: &str) -> anyhow::Result<CommentCreated> {
        let url = format!(
            "{}/repos/{}/{}/issues/{}/comments",
            self.base_url, repo.owner, repo.repo, issue_number
        );
        let payload = serde_json::json!({ "body": body });
        let resp = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("create_comment failed: {status}: {body}");
        }
        let body = resp.text().await.context("read create_comment body")?;
        parse_comment_created(&body)
    }
}

#[derive(Deserialize)]
struct SearchPayload {
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    number: u64,
    title: String,
    state: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
}

pub fn parse_search_response(json: &str) -> anyhow::Result<Vec<IssueSummary>> {
    let payload: SearchPayload = serde_json::from_str(json).context("parsing GitHub /search/issues JSON")?;
    Ok(payload
        .items
        .into_iter()
        .map(|i| IssueSummary {
            number: i.number,
            title: i.title,
            state: i.state,
            html_url: i.html_url,
            body_excerpt: truncate_excerpt(i.body.as_deref().unwrap_or(""), BODY_EXCERPT_CHARS),
        })
        .collect())
}

#[derive(Deserialize)]
struct IssueCreatedPayload {
    number: u64,
    html_url: String,
}

pub fn parse_issue_created(json: &str) -> anyhow::Result<IssueCreated> {
    let p: IssueCreatedPayload = serde_json::from_str(json).context("parsing GitHub create-issue response")?;
    Ok(IssueCreated {
        number: p.number,
        html_url: p.html_url,
    })
}

#[derive(Deserialize)]
struct CommentCreatedPayload {
    html_url: String,
}

/// Minimal percent-encoder for query-string values. We avoid pulling in
/// `urlencoding`/`percent-encoding` for one call site: encode anything that's
/// not unreserved per RFC 3986 plus space → '+'.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub fn parse_comment_created(json: &str) -> anyhow::Result<CommentCreated> {
    let p: CommentCreatedPayload = serde_json::from_str(json).context("parsing GitHub create-comment response")?;
    Ok(CommentCreated { html_url: p.html_url })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_BODY: &str = r#"{
        "total_count": 2,
        "items": [
            {"number":42,"title":"Login hangs","state":"open","html_url":"https://github.com/o/r/issues/42","body":"loooong body"},
            {"number":43,"title":"PR foo","state":"open","html_url":"https://github.com/o/r/pull/43"}
        ]
    }"#;

    #[test]
    fn parses_search_response_into_issue_summaries() {
        let out = parse_search_response(SEARCH_BODY).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].number, 42);
        assert_eq!(out[0].title, "Login hangs");
        assert_eq!(out[0].body_excerpt, "loooong body");
        assert_eq!(out[1].body_excerpt, "");
    }

    #[test]
    fn parses_issue_created_response() {
        let body = r#"{"number":99,"html_url":"https://github.com/o/r/issues/99"}"#;
        let out = parse_issue_created(body).unwrap();
        assert_eq!(out.number, 99);
        assert_eq!(out.html_url, "https://github.com/o/r/issues/99");
    }

    #[test]
    fn parses_comment_created_response() {
        let body = r#"{"html_url":"https://github.com/o/r/issues/99#issuecomment-1"}"#;
        let out = parse_comment_created(body).unwrap();
        assert_eq!(out.html_url, "https://github.com/o/r/issues/99#issuecomment-1");
    }

    #[test]
    fn rejects_garbage_search_payload() {
        assert!(parse_search_response("not json").is_err());
    }

    #[test]
    fn urlencode_passes_unreserved_through() {
        assert_eq!(urlencode("hello-world.txt"), "hello-world.txt");
        assert_eq!(urlencode("ABCabc012._-~"), "ABCabc012._-~");
    }

    #[test]
    fn urlencode_replaces_space_with_plus() {
        assert_eq!(urlencode("hello world"), "hello+world");
    }

    #[test]
    fn urlencode_percent_encodes_specials() {
        assert_eq!(urlencode("repo:o/r"), "repo%3Ao%2Fr");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
    }
}
