//! Live HTTP sources for GitHub issues and releases.
//!
//! These wrap `parse_issues` / `parse_releases` from [`crate::github_source`]
//! with the actual `reqwest` HTTP fetch + GitHub PAT auth. The pure parser
//! stays separately testable; this module is exercised by integration tests
//! against a mock server (`tests/integration.rs`).

use anyhow::Context;
use reqwest::header::{
    ACCEPT,
    AUTHORIZATION,
    USER_AGENT,
};

use crate::github_source::{
    parse_issues,
    parse_releases,
};
use crate::{
    Partition,
    RawChunk,
    Source,
};

/// `Source` impl that pulls all open + closed GitHub issues for a repo and
/// drops PRs. Pagination follows the `Link` header until exhausted. Default
/// page size is 100 (the API max).
pub struct GithubIssuesSource {
    client: reqwest::Client,
    base_url: String,
    repo: String,
    token: Option<String>,
}

impl GithubIssuesSource {
    pub fn new(repo: impl Into<String>, token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.github.com".to_string(),
            repo: repo.into(),
            token,
        }
    }

    /// Test seam: point at a wiremock instead of `api.github.com`.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(USER_AGENT, "kiro-help-corpus-ingest/1.0".parse().unwrap());
        h.insert(ACCEPT, "application/vnd.github+json".parse().unwrap());
        if let Some(token) = &self.token {
            h.insert(
                AUTHORIZATION,
                format!("Bearer {token}").parse().expect("PAT was not ASCII"),
            );
        }
        h
    }
}

#[async_trait::async_trait]
impl Source for GithubIssuesSource {
    fn partition(&self) -> Partition {
        Partition::Issues
    }

    async fn fetch(&self) -> anyhow::Result<Vec<RawChunk>> {
        let mut out = Vec::new();
        let mut url = format!("{}/repos/{}/issues?state=all&per_page=100", self.base_url, self.repo);
        loop {
            let resp = self
                .client
                .get(&url)
                .headers(self.auth_headers())
                .send()
                .await
                .with_context(|| format!("GET {url}"))?;
            let next = next_page_from_link(resp.headers().get(reqwest::header::LINK));
            let body = resp.text().await.context("read response body")?;
            let mut chunks =
                parse_issues(&self.repo, &body).with_context(|| format!("parsing GitHub issues page from {url}"))?;
            out.append(&mut chunks);
            match next {
                Some(n) => url = n,
                None => break,
            }
        }
        Ok(out)
    }
}

/// `Source` impl that pulls every published release.
pub struct GithubReleasesSource {
    client: reqwest::Client,
    base_url: String,
    repo: String,
    token: Option<String>,
}

impl GithubReleasesSource {
    pub fn new(repo: impl Into<String>, token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.github.com".to_string(),
            repo: repo.into(),
            token,
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert(USER_AGENT, "kiro-help-corpus-ingest/1.0".parse().unwrap());
        h.insert(ACCEPT, "application/vnd.github+json".parse().unwrap());
        if let Some(token) = &self.token {
            h.insert(
                AUTHORIZATION,
                format!("Bearer {token}").parse().expect("PAT was not ASCII"),
            );
        }
        h
    }
}

#[async_trait::async_trait]
impl Source for GithubReleasesSource {
    fn partition(&self) -> Partition {
        Partition::Releases
    }

    async fn fetch(&self) -> anyhow::Result<Vec<RawChunk>> {
        let url = format!("{}/repos/{}/releases?per_page=100", self.base_url, self.repo);
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let body = resp.text().await.context("read response body")?;
        parse_releases(&self.repo, &body).with_context(|| format!("parsing GitHub releases from {url}"))
    }
}

/// Extract the `rel="next"` URL from a GitHub `Link` header. Returns `None`
/// when there is no next page.
pub fn next_page_from_link(link: Option<&reqwest::header::HeaderValue>) -> Option<String> {
    let value = link?.to_str().ok()?;
    for part in value.split(',') {
        let part = part.trim();
        // Format: `<url>; rel="next"`.
        let mut it = part.splitn(2, ';');
        let url_part = it.next()?.trim();
        let rel_part = it.next()?.trim();
        if rel_part != "rel=\"next\"" {
            continue;
        }
        let url = url_part.strip_prefix('<')?.strip_suffix('>')?.to_string();
        return Some(url);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_page_from_link_returns_url_when_rel_next_present() {
        let link = reqwest::header::HeaderValue::from_str(
            r#"<https://api.github.com/repositories/1/issues?page=2>; rel="next", <https://api.github.com/repositories/1/issues?page=10>; rel="last""#,
        )
        .unwrap();
        let got = next_page_from_link(Some(&link)).expect("next URL");
        assert_eq!(got, "https://api.github.com/repositories/1/issues?page=2");
    }

    #[test]
    fn next_page_returns_none_on_last_page() {
        let link = reqwest::header::HeaderValue::from_str(
            r#"<https://api.github.com/repositories/1/issues?page=1>; rel="prev", <https://api.github.com/repositories/1/issues?page=1>; rel="first""#,
        )
        .unwrap();
        assert!(next_page_from_link(Some(&link)).is_none());
    }

    #[test]
    fn next_page_returns_none_when_no_link_header() {
        assert!(next_page_from_link(None).is_none());
    }
}
