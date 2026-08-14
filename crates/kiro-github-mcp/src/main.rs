//! kiro-github-mcp — MCP stdio server exposing three GitHub-backed tools to
//! the kiro-help bot. Splits read vs write surface so the bot can launch
//! distinct instances with read-only vs write-capable PATs.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use kiro_github_mcp::http_client::HttpGithubClient;
use kiro_github_mcp::rate_limit::RateLimiter;
use kiro_github_mcp::{
    GithubClient,
    RepoRef,
};
use rmcp::model::*;
use rmcp::service::{
    RequestContext,
    RoleServer,
};
use rmcp::transport::stdio;
use rmcp::{
    ServerHandler,
    ServiceExt,
};
use serde::Deserialize;

#[derive(Parser, Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// Read-only: only `search_github_issues` is exposed.
    Read,
    /// Write-capable: all three tools are exposed.
    Write,
}

impl std::str::FromStr for Scope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read" => Ok(Scope::Read),
            "write" => Ok(Scope::Write),
            other => Err(format!("expected `read` or `write`, got `{other}`")),
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "kiro-github-mcp", version, about)]
struct Args {
    /// `read` exposes only the search tool; `write` adds create_github_issue
    /// and comment_on_existing.
    #[arg(long, default_value = "read")]
    scope: Scope,

    /// GitHub PAT. Required for any non-public repo or for write operations.
    /// Honors GH_PAT env (matches the secret name we provision in Secrets
    /// Manager — kiro-bot/github-pat-{read,write}).
    #[arg(long, env = "GH_PAT")]
    token: Option<String>,

    /// Default `owner/repo` to search/comment in. Tool callers can override
    /// per call.
    #[arg(long, env = "KIRO_GITHUB_DEFAULT_REPO", default_value = "kiro-team/kiro-cli")]
    default_repo: String,
}

#[derive(Clone)]
struct GithubServer {
    scope: Scope,
    client: Arc<dyn GithubClient>,
    default_repo: RepoRef,
    rate_limiter: Arc<RateLimiter>,
}

impl ServerHandler for GithubServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "Search GitHub issues for kiro-cli, and (when --scope=write) file new issues or post comments. \
             Always confirm with the user before using a write tool."
                .to_string(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools = vec![tool_search_issues()];
        if self.scope == Scope::Write {
            tools.push(tool_create_issue());
            tools.push(tool_comment_on_existing());
        }
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args_value: serde_json::Value = match request.arguments {
            Some(map) => serde_json::Value::Object(map.into_iter().collect()),
            None => serde_json::Value::Object(Default::default()),
        };

        // Rate limiter applies to every call regardless of scope. The free
        // budget for read tools tracks alongside writes intentionally.
        if !self.rate_limiter.try_take() {
            return Err(ErrorData::invalid_params(
                "rate limit exhausted (30 calls/min); slow down or try again shortly".to_string(),
                None,
            ));
        }

        let result = match request.name.as_ref() {
            "search_github_issues" => {
                let input: SearchInput = serde_json::from_value(args_value)
                    .map_err(|e| ErrorData::invalid_params(format!("invalid arguments: {e}"), None))?;
                let repo = self.resolve_repo(input.repo.as_deref())?;
                let limit = input.limit.unwrap_or(10).min(30);
                let issues = self
                    .client
                    .search_issues(&repo, &input.query, limit)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                let body = serde_json::to_value(&issues).map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                Ok(json_result(body))
            },
            "create_github_issue" if self.scope == Scope::Write => {
                let input: CreateIssueInput = serde_json::from_value(args_value)
                    .map_err(|e| ErrorData::invalid_params(format!("invalid arguments: {e}"), None))?;
                let repo = self.resolve_repo(input.repo.as_deref())?;
                let labels: Vec<String> = input.labels.unwrap_or_default();
                let created = self
                    .client
                    .create_issue(&repo, &input.title, &input.body, &labels)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                let body =
                    serde_json::to_value(&created).map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                Ok(json_result(body))
            },
            "comment_on_existing" if self.scope == Scope::Write => {
                let input: CommentInput = serde_json::from_value(args_value)
                    .map_err(|e| ErrorData::invalid_params(format!("invalid arguments: {e}"), None))?;
                let repo = self.resolve_repo(input.repo.as_deref())?;
                let created = self
                    .client
                    .create_comment(&repo, input.number, &input.body)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                let body =
                    serde_json::to_value(&created).map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                Ok(json_result(body))
            },
            other => Err(ErrorData::invalid_params(
                format!(
                    "unknown or scope-restricted tool: {other} (current scope: {:?})",
                    self.scope
                ),
                None,
            )),
        }?;
        Ok(result.into())
    }
}

impl GithubServer {
    fn resolve_repo(&self, repo: Option<&str>) -> Result<RepoRef, ErrorData> {
        match repo {
            Some(slug) => RepoRef::parse(slug).map_err(|e| ErrorData::invalid_params(e.to_string(), None)),
            None => Ok(self.default_repo.clone()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchInput {
    query: String,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CreateIssueInput {
    title: String,
    body: String,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CommentInput {
    number: u64,
    body: String,
    #[serde(default)]
    repo: Option<String>,
}

fn tool_search_issues() -> Tool {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "GitHub issue search keywords. Don't include the `repo:` qualifier — pass `repo` separately." },
            "repo":  { "type": "string", "description": "owner/repo. Defaults to the bot's configured repo." },
            "limit": { "type": "integer", "default": 10, "maximum": 30 }
        },
        "required": ["query"]
    });
    Tool::new(
        "search_github_issues",
        "Search GitHub issues / PRs for the kiro-cli repo by keyword. Returns the top-N matches with title, state, URL, and an excerpt.",
        Arc::new(serde_json::from_value(schema).expect("static schema")),
    )
    .with_annotations(ToolAnnotations::new().read_only(true))
}

fn tool_create_issue() -> Tool {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "body":  { "type": "string" },
            "repo":  { "type": "string", "description": "owner/repo. Defaults to the bot's configured repo." },
            "labels": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["title", "body"]
    });
    Tool::new(
        "create_github_issue",
        "File a new GitHub issue. The bot must always confirm with the user via reaction before invoking this tool.",
        Arc::new(serde_json::from_value(schema).expect("static schema")),
    )
    .with_annotations(ToolAnnotations::new().read_only(false).idempotent(false))
}

fn tool_comment_on_existing() -> Tool {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "number": { "type": "integer" },
            "body":   { "type": "string" },
            "repo":   { "type": "string" }
        },
        "required": ["number", "body"]
    });
    Tool::new(
        "comment_on_existing",
        "Add a comment to an existing GitHub issue or PR. The bot must always confirm with the user via reaction before invoking this tool.",
        Arc::new(serde_json::from_value(schema).expect("static schema")),
    )
    .with_annotations(ToolAnnotations::new().read_only(false).idempotent(false))
}

fn json_result(value: serde_json::Value) -> CallToolResult {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(value);
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    let default_repo = RepoRef::parse(&args.default_repo)?;

    let client: Arc<dyn GithubClient> = Arc::new(HttpGithubClient::new(args.token));

    let server = GithubServer {
        scope: args.scope,
        client,
        default_repo,
        rate_limiter: Arc::new(RateLimiter::default_per_spec()),
    };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_tools_advertise_their_mutation_behavior() {
        let search = tool_search_issues().annotations.unwrap();
        assert_eq!(search.read_only_hint, Some(true));

        for mutation in [tool_create_issue(), tool_comment_on_existing()] {
            let annotations = mutation.annotations.unwrap();
            assert_eq!(annotations.read_only_hint, Some(false));
            assert_eq!(annotations.idempotent_hint, Some(false));
        }
    }
}
