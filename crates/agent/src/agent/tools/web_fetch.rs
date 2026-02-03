use std::borrow::Cow;
use std::time::Duration;

use serde::{
    Deserialize,
    Serialize,
};

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};

const USER_AGENT: &str = "Kiro-CLI";
const MAX_TRUNCATE_CHARS: usize = 8000;
const DEFAULT_SNIPPET_LINES: usize = 20;
const CONTEXT_LINES: usize = 10;
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024; // 10MB
const MAX_REDIRECTS: usize = 10;
const MAX_RETRIES: u32 = 3;

const WEB_FETCH_DESCRIPTION: &str = r#"
Fetch and extract content from a specific URL. Supports three modes: 'selective' (default, extracts relevant sections around search terms), 'truncated' (first 8000 chars), 'full' (complete content).
"#;

const WEB_FETCH_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "url": {
            "type": "string",
            "description": "URL to fetch content from"
        },
        "mode": {
            "type": "string",
            "enum": ["selective", "truncated", "full"],
            "description": "Extraction mode: 'selective' for smart extraction (default), 'truncated' for first 8000 chars, 'full' for complete content"
        },
        "search_terms": {
            "type": "string",
            "description": "Optional: Keywords to find in selective mode. Returns ~10 lines before and after matches."
        }
    },
    "required": ["url"]
}
"#;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
enum FetchMode {
    #[default]
    Selective,
    Truncated,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetch {
    url: String,
    #[serde(default)]
    mode: FetchMode,
    search_terms: Option<String>,
}

impl BuiltInToolTrait for WebFetch {
    fn name() -> BuiltInToolName {
        BuiltInToolName::WebFetch
    }

    fn description() -> Cow<'static, str> {
        WEB_FETCH_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        WEB_FETCH_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["web_fetch"])
    }
}

impl WebFetch {
    pub async fn execute(&self) -> ToolExecutionResult {
        let content = self.fetch_url_content().await?;
        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(content)]))
    }

    async fn fetch_url_content(&self) -> Result<String, ToolExecutionError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
            .build()
            .map_err(|e| ToolExecutionError::Custom(format!("Failed to build HTTP client: {e}")))?;

        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            match self.fetch_with_client(&client).await {
                Ok(content) => return Ok(content),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(Duration::from_secs(2u64.pow(attempt - 1))).await;
                    }
                },
            }
        }

        Err(last_error.unwrap())
    }

    async fn fetch_with_client(&self, client: &reqwest::Client) -> Result<String, ToolExecutionError> {
        let response = client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| ToolExecutionError::Custom(format!("Failed to fetch URL {}: {e}", self.url)))?;

        if !response.status().is_success() {
            return Err(ToolExecutionError::Custom(format!(
                "HTTP error {}: {}",
                response.status(),
                self.url
            )));
        }

        if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
            let content_type_str = content_type.to_str().unwrap_or("");
            if !content_type_str.contains("text/") && !content_type_str.contains("html") {
                return Err(ToolExecutionError::Custom(format!(
                    "Unsupported content type: {content_type_str}"
                )));
            }
        }

        if let Some(content_length) = response.content_length()
            && content_length > MAX_RESPONSE_SIZE as u64
        {
            return Err(ToolExecutionError::Custom(format!(
                "Response too large: {content_length} bytes (max: {MAX_RESPONSE_SIZE})"
            )));
        }

        let html = response
            .text()
            .await
            .map_err(|e| ToolExecutionError::Custom(format!("Failed to read response: {e}")))?;

        if html.len() > MAX_RESPONSE_SIZE {
            return Err(ToolExecutionError::Custom(format!(
                "Response too large: {} bytes (max: {MAX_RESPONSE_SIZE})",
                html.len()
            )));
        }

        let cleaned = Self::strip_html(&html);

        match self.mode {
            FetchMode::Full => Ok(cleaned),
            FetchMode::Truncated => Ok(Self::truncate_content(&cleaned, MAX_TRUNCATE_CHARS)),
            FetchMode::Selective => Ok(self.extract_snippets(&cleaned)),
        }
    }

    fn strip_html(html: &str) -> String {
        html2text::from_read(html.as_bytes(), usize::MAX)
    }

    fn truncate_content(text: &str, max_chars: usize) -> String {
        let char_count = text.chars().count();
        if char_count > max_chars {
            let truncated: String = text.chars().take(max_chars).collect();
            format!("{truncated}[Content truncated - showing first {max_chars} characters]")
        } else {
            text.to_string()
        }
    }

    fn extract_snippets(&self, text: &str) -> String {
        let lines: Vec<&str> = text.split('.').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

        if let Some(search_terms) = &self.search_terms {
            let terms: Vec<&str> = search_terms.split_whitespace().collect();
            let mut relevant_indices = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                let line_lower = line.to_lowercase();
                if terms.iter().any(|term| line_lower.contains(&term.to_lowercase())) {
                    relevant_indices.push(i);
                }
            }

            if relevant_indices.is_empty() {
                let joined = lines
                    .iter()
                    .take(DEFAULT_SNIPPET_LINES)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(". ");
                return format!("{joined}.");
            }

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
            let joined = lines
                .iter()
                .take(DEFAULT_SNIPPET_LINES)
                .copied()
                .collect::<Vec<_>>()
                .join(". ");
            format!("{joined}.")
        }
    }
}
