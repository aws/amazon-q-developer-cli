use std::io::Write;
use std::time::Duration;

use crossterm::{
    queue,
    style,
};
use eyre::{
    Result,
    WrapErr,
};
use serde::Deserialize;
use tracing::error;

use super::{
    InvokeOutput,
    OutputKind,
    ToolInfo,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::os::Os;
use crate::util::resource_permission::{
    ResourceSettings,
    ResourceType,
    eval_permission,
};

const USER_AGENT: &str = "Kiro-CLI";
const MAX_TRUNCATE_CHARS: usize = 8000;
const DEFAULT_SNIPPET_LINES: usize = 20;
const CONTEXT_LINES: usize = 10;
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024; // 10MB
const MAX_REDIRECTS: usize = 10;
const MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FetchMode {
    Selective,
    Truncated,
    Full,
}

impl Default for FetchMode {
    fn default() -> Self {
        Self::Selective
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebFetch {
    url: String,
    #[serde(default)]
    mode: FetchMode,
    search_terms: Option<String>,
}

impl WebFetch {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "web_fetch",
        preferred_alias: "web_fetch",
        aliases: &["web_fetch"],
    };

    pub fn is_enabled(os: &Os) -> bool {
        let endpoint = crate::api_client::Endpoint::configured_value(&os.database);
        crate::feature_flags::FeatureFlags::is_web_search_enabled_for_region(endpoint.region().as_ref())
    }

    #[allow(clippy::unused_self)]
    pub fn eval_perm(&self, _os: &Os, agent: &Agent) -> PermissionEvalResult {
        use crate::util::tool_permission_checker::is_tool_in_allowlist;

        let is_in_allowlist = Self::INFO
            .aliases
            .iter()
            .any(|alias| is_tool_in_allowlist(&agent.allowed_tools, alias, None));

        let settings = match Self::INFO
            .aliases
            .iter()
            .find_map(|alias| agent.tools_settings.get(*alias))
        {
            Some(settings) => match serde_json::from_value::<ResourceSettings>(settings.clone()) {
                Ok(settings) => settings,
                Err(e) => {
                    error!("Failed to deserialize tool settings for web_fetch: {:?}", e);
                    return PermissionEvalResult::Ask;
                },
            },
            None => ResourceSettings::default(),
        };

        eval_permission(&settings, &self.url, ResourceType::Url, is_in_allowlist)
    }

    pub async fn invoke(&self, _os: &Os, updates: impl Write) -> Result<InvokeOutput> {
        // Catch any panics and convert to Result
        let fetch_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| async {
            self.fetch_url_content().await
        }));

        let content = match fetch_result {
            Ok(future) => future.await?,
            Err(panic_err) => {
                let panic_msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic occurred".to_string()
                };
                return Err(eyre::eyre!(
                    "Tool execution panicked: {}. Please try a different mode (e.g., 'selective' or 'full' instead of 'truncated') or a different URL.",
                    panic_msg
                ));
            },
        };

        let content_size = content.len();
        let mode_desc = match self.mode {
            FetchMode::Full => "full content",
            FetchMode::Truncated => "truncated content",
            FetchMode::Selective => "selective",
        };

        super::queue_function_result(
            &format!("Fetched {content_size} bytes ({mode_desc}) from URL"),
            &mut std::io::BufWriter::new(updates),
            false,
            false,
        )?;

        Ok(InvokeOutput {
            output: OutputKind::Text(content),
        })
    }

    async fn fetch_url_content(&self) -> Result<String> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
            .build()
            .wrap_err("Failed to build HTTP client")?;

        let mut last_error = None;

        // Retry logic for transient failures
        for attempt in 1..=MAX_RETRIES {
            match self.fetch_with_client(&client).await {
                Ok(content) => return Ok(content),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < MAX_RETRIES {
                        // Exponential backoff: 1s, 2s, 4s
                        tokio::time::sleep(Duration::from_secs(2u64.pow(attempt - 1))).await;
                    }
                },
            }
        }

        Err(last_error.unwrap())
    }

    async fn fetch_with_client(&self, client: &reqwest::Client) -> Result<String> {
        let response = client
            .get(&self.url)
            .send()
            .await
            .wrap_err_with(|| format!("Failed to fetch URL: {}", self.url))?;

        if !response.status().is_success() {
            return Err(eyre::eyre!("HTTP error {}: {}", response.status(), self.url));
        }

        // Check content-type to ensure it's HTML/text
        if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            let content_type_str = content_type.to_str().unwrap_or("");
            if !content_type_str.contains("text/") && !content_type_str.contains("html") {
                return Err(eyre::eyre!(
                    "Unsupported content type: {}. Expected text/html or text/*",
                    content_type_str
                ));
            }
        }

        // Check content length before downloading
        if let Some(content_length) = response.content_length() {
            if content_length > MAX_RESPONSE_SIZE as u64 {
                return Err(eyre::eyre!(
                    "Response too large: {} bytes (max: {} bytes)",
                    content_length,
                    MAX_RESPONSE_SIZE
                ));
            }
        }

        let html = response.text().await.wrap_err("Failed to read response body")?;

        // Check actual size after download (in case Content-Length was missing)
        if html.len() > MAX_RESPONSE_SIZE {
            return Err(eyre::eyre!(
                "Response too large: {} bytes (max: {} bytes)",
                html.len(),
                MAX_RESPONSE_SIZE
            ));
        }

        let cleaned = Self::strip_html(&html);

        match self.mode {
            FetchMode::Full => Ok(cleaned),
            FetchMode::Truncated => Self::truncate_content(&cleaned, MAX_TRUNCATE_CHARS),
            FetchMode::Selective => Ok(self.extract_snippets(&cleaned)),
        }
    }

    fn strip_html(html: &str) -> String {
        // Use html2text library for proper HTML parsing and entity decoding
        html2text::from_read(html.as_bytes(), usize::MAX)
    }

    fn truncate_content(text: &str, max_chars: usize) -> Result<String> {
        let char_count = text.chars().count();
        if char_count > max_chars {
            let truncated: String = text.chars().take(max_chars).collect();
            Ok(format!(
                "{truncated}[Content truncated - showing first {max_chars} characters]"
            ))
        } else {
            Ok(text.to_string())
        }
    }

    fn extract_snippets(&self, text: &str) -> String {
        let lines: Vec<&str> = text.split('.').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

        if let Some(search_terms) = &self.search_terms {
            // Find lines containing search terms
            let terms: Vec<&str> = search_terms.split_whitespace().collect();
            let mut relevant_indices = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                let line_lower = line.to_lowercase();
                if terms.iter().any(|term| line_lower.contains(&term.to_lowercase())) {
                    relevant_indices.push(i);
                }
            }

            if relevant_indices.is_empty() {
                // No matches found, return first DEFAULT_SNIPPET_LINES lines
                let joined = lines
                    .iter()
                    .take(DEFAULT_SNIPPET_LINES)
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>()
                    .join(". ");
                return format!("{joined}.");
            }

            // Extract CONTEXT_LINES before and after each match
            let mut result_lines = Vec::new();
            for &idx in &relevant_indices {
                let start = idx.saturating_sub(CONTEXT_LINES);
                let end = (idx + CONTEXT_LINES + 1).min(lines.len());

                for i in start..end {
                    if !result_lines.contains(&i) {
                        result_lines.push(i);
                    }
                }
            }

            result_lines.sort_unstable();
            let joined = result_lines.iter().map(|&i| lines[i]).collect::<Vec<_>>().join(". ");
            format!("{joined}.")
        } else {
            // No search terms, return first DEFAULT_SNIPPET_LINES lines
            let joined = lines
                .iter()
                .take(DEFAULT_SNIPPET_LINES)
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
                .join(". ");
            format!("{joined}.")
        }
    }

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("Fetching content from: "))?;

        // Truncate URL to 80 chars
        let truncated_url = if self.url.len() > 80 {
            format!("{}...", &self.url[..77])
        } else {
            self.url.clone()
        };

        queue!(output, style::Print(&truncated_url))?;

        if let Some(terms) = &self.search_terms {
            queue!(output, style::Print(format!(" (searching for: {terms})")))?;
        }

        let mode_str = match self.mode {
            FetchMode::Selective => "selective",
            FetchMode::Truncated => "truncated",
            FetchMode::Full => "full",
        };
        queue!(output, style::Print(format!(" [mode: {mode_str}]")))?;
        super::display_tool_use(tool, output)?;
        queue!(output, style::Print("\n"))?;
        Ok(())
    }

    pub async fn validate(&self, _os: &Os) -> Result<()> {
        if self.url.trim().is_empty() {
            return Err(eyre::eyre!("URL cannot be empty"));
        }

        // Use url crate for proper URL validation
        url::Url::parse(&self.url).map_err(|e| eyre::eyre!("Invalid URL format: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::cli::agent::Agent;
    use crate::cli::agent::wrapper_types::ToolSettingTarget;

    fn create_agent(allowed_tools: Vec<&str>, settings: serde_json::Value) -> Agent {
        Agent {
            allowed_tools: allowed_tools.into_iter().map(String::from).collect(),
            tools_settings: {
                let mut map = HashMap::new();
                map.insert(ToolSettingTarget("web_fetch".to_string()), settings);
                map
            },
            ..Default::default()
        }
    }

    fn create_web_fetch(url: &str) -> WebFetch {
        WebFetch {
            url: url.to_string(),
            mode: FetchMode::default(),
            search_terms: None,
        }
    }

    #[tokio::test]
    async fn test_tool_not_in_allowed_tools_with_trusted_url_allows() {
        // Settings apply even when tool is not in allowedTools
        let agent = create_agent(
            vec![],
            serde_json::json!({
                "trusted": [".*docs\\.aws\\.amazon\\.com.*"]
            }),
        );
        let tool = create_web_fetch("https://docs.aws.amazon.com/lambda/");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        assert!(matches!(result, PermissionEvalResult::Allow));
    }

    #[tokio::test]
    async fn test_tool_not_in_allowed_tools_default_asks() {
        // Default is Ask when tool not in allowedTools and no pattern matches
        let agent = create_agent(
            vec![],
            serde_json::json!({
                "trusted": [".*docs\\.aws\\.amazon\\.com.*"]
            }),
        );
        let tool = create_web_fetch("https://example.com");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        assert!(matches!(result, PermissionEvalResult::Ask));
    }

    #[tokio::test]
    async fn test_blocked_url_denies() {
        let agent = create_agent(
            vec!["web_fetch"],
            serde_json::json!({
                "blocked": [".*pastebin\\.com.*", ".*malicious\\.org.*"]
            }),
        );
        let tool = create_web_fetch("https://pastebin.com/abc123");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        assert!(
            matches!(result, PermissionEvalResult::Deny(ref patterns) if patterns.iter().any(|p| p.contains("pastebin\\.com")))
        );
    }

    #[tokio::test]
    async fn test_trusted_url_allows() {
        let agent = create_agent(
            vec!["web_fetch"],
            serde_json::json!({
                "trusted": [".*docs\\.aws\\.amazon\\.com.*"]
            }),
        );
        let tool = create_web_fetch("https://docs.aws.amazon.com/lambda/");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        assert!(matches!(result, PermissionEvalResult::Allow));
    }

    #[tokio::test]
    async fn test_url_not_in_any_list_allows() {
        let agent = create_agent(
            vec!["web_fetch"],
            serde_json::json!({
                "trusted": [".*docs\\.aws\\.amazon\\.com.*"]
            }),
        );
        let tool = create_web_fetch("https://example.com");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        assert!(matches!(result, PermissionEvalResult::Allow));
    }

    #[tokio::test]
    async fn test_invalid_regex_in_blocked_urls_denies() {
        let agent = create_agent(
            vec!["web_fetch"],
            serde_json::json!({
                "blocked": ["(unclosed-paren"]
            }),
        );
        let tool = create_web_fetch("https://example.com");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        // Invalid regex in blocked should deny (deny-all behavior)
        assert!(matches!(result, PermissionEvalResult::Deny(_)));
    }

    #[tokio::test]
    async fn test_invalid_regex_in_trusted_urls_skips() {
        let agent = create_agent(
            vec!["web_fetch"],
            serde_json::json!({
                "trusted": ["(unclosed-paren", ".*valid\\.com.*"]
            }),
        );
        let tool = create_web_fetch("https://valid.com");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        // Invalid regex should be skipped, valid pattern should match
        assert!(matches!(result, PermissionEvalResult::Allow));
    }

    #[tokio::test]
    async fn test_blocked_urls_precedence_over_trusted() {
        let agent = create_agent(
            vec!["web_fetch"],
            serde_json::json!({
                "trusted": [".*example\\.com.*"],
                "blocked": [".*example\\.com.*"]
            }),
        );
        let tool = create_web_fetch("https://example.com");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        // blocked should take precedence
        assert!(matches!(result, PermissionEvalResult::Deny(_)));
    }

    #[tokio::test]
    async fn test_no_settings_allows_when_tool_trusted() {
        let agent = Agent {
            allowed_tools: vec!["web_fetch".to_string()].into_iter().collect(),
            tools_settings: HashMap::new(),
            ..Default::default()
        };
        let tool = create_web_fetch("https://example.com");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        assert!(matches!(result, PermissionEvalResult::Allow));
    }

    #[tokio::test]
    async fn test_pattern_sampling_with_many_blocked_urls() {
        let mut blocked_resources = Vec::new();
        for i in 0..150 {
            blocked_resources.push(format!(".*blocked{}\\.com.*", i));
        }

        let agent = create_agent(
            vec!["web_fetch"],
            serde_json::json!({
                "blocked": blocked_resources
            }),
        );
        let tool = create_web_fetch("https://blocked0.com");
        let os = Os::new().await.unwrap();

        let result = tool.eval_perm(&os, &agent);
        if let PermissionEvalResult::Deny(patterns) = result {
            // Should return at most 100 patterns
            assert!(patterns.len() <= 100);
            // Should include the matched pattern
            assert!(patterns.iter().any(|p| p.contains("blocked0")));
        } else {
            panic!("Expected Deny result");
        }
    }
}
