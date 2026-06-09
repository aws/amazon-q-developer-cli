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
    pub fn url(&self) -> &str {
        &self.url
    }

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
        let html_owned = html.to_string();
        match std::panic::catch_unwind(move || html2text::from_read(html_owned.as_bytes(), usize::MAX)) {
            Ok(Ok(text)) => text,
            _ => {
                tracing::warn!("html2text panicked, falling back to raw HTML");
                html.to_string()
            },
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_renders_basic_html() {
        let html = "<p>Hello <b>world</b></p>";
        let result = WebFetch::strip_html(html);
        assert!(result.contains("Hello"));
        assert!(result.contains("world"));
        assert!(!result.contains("<p>"));
    }

    #[test]
    fn strip_html_fallback_on_empty() {
        let result = WebFetch::strip_html("");
        assert!(result.is_empty() || result.trim().is_empty());
    }

    #[test]
    fn strip_html_does_not_panic_on_malformed_html() {
        let malformed = "<div><p>unclosed<table><tr><td>nested</div>";
        let result = WebFetch::strip_html(malformed);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_truncate_content_short() {
        let text = "short text";
        let result = WebFetch::truncate_content(text, 100);
        assert_eq!(result, "short text");
    }

    #[test]
    fn test_truncate_content_long() {
        let text = "a".repeat(10000);
        let result = WebFetch::truncate_content(&text, 100);
        assert!(result.contains("[Content truncated"));
        assert!(result.starts_with(&"a".repeat(100)));
    }

    #[test]
    fn test_extract_snippets_no_search_terms() {
        let wf = WebFetch {
            url: "http://example.com".to_string(),
            mode: FetchMode::Selective,
            search_terms: None,
        };
        let text = (1..=30).map(|i| format!("sentence {i}")).collect::<Vec<_>>().join(". ");
        let result = wf.extract_snippets(&text);
        assert!(result.contains("sentence 1"));
        assert!(!result.is_empty());
    }

    #[test]
    fn test_extract_snippets_with_matching_terms() {
        let wf = WebFetch {
            url: "http://example.com".to_string(),
            mode: FetchMode::Selective,
            search_terms: Some("target".to_string()),
        };
        let text = "intro. filler. more filler. target content here. ending.";
        let result = wf.extract_snippets(text);
        assert!(result.contains("target content here"));
    }

    #[test]
    fn test_extract_snippets_no_matching_terms_fallback() {
        let wf = WebFetch {
            url: "http://example.com".to_string(),
            mode: FetchMode::Selective,
            search_terms: Some("nonexistent_xyz".to_string()),
        };
        let text = "first sentence. second sentence. third sentence.";
        let result = wf.extract_snippets(text);
        // Falls back to first DEFAULT_SNIPPET_LINES sentences
        assert!(result.contains("first sentence"));
    }

    #[test]
    fn test_serde_default_mode() {
        let json = r#"{"url": "http://example.com"}"#;
        let wf: WebFetch = serde_json::from_str(json).unwrap();
        assert!(matches!(wf.mode, FetchMode::Selective));
        assert!(wf.search_terms.is_none());
    }

    #[test]
    fn test_serde_full_mode() {
        let json = r#"{"url": "http://example.com", "mode": "full"}"#;
        let wf: WebFetch = serde_json::from_str(json).unwrap();
        assert!(matches!(wf.mode, FetchMode::Full));
    }

    #[test]
    fn test_serde_truncated_mode() {
        let json = r#"{"url": "http://example.com", "mode": "truncated", "search_terms": "rust"}"#;
        let wf: WebFetch = serde_json::from_str(json).unwrap();
        assert!(matches!(wf.mode, FetchMode::Truncated));
        assert_eq!(wf.search_terms.as_deref(), Some("rust"));
    }

    #[test]
    fn test_built_in_tool_trait() {
        assert!(matches!(WebFetch::name(), BuiltInToolName::WebFetch));
        assert!(!WebFetch::description().is_empty());
        assert!(!WebFetch::input_schema().is_empty());
        assert_eq!(WebFetch::aliases(), Some(["web_fetch"].as_slice()));
    }

    #[test]
    fn test_strip_html_complex() {
        let html = r#"<html><head><title>Test</title><script>var x=1;</script><style>.a{}</style></head>
        <body><h1>Title</h1><p>Paragraph with <a href="url">link</a></p></body></html>"#;
        let result = WebFetch::strip_html(html);
        assert!(result.contains("Title"));
        assert!(result.contains("Paragraph"));
        assert!(result.contains("link"));
    }

    #[test]
    fn test_extract_snippets_case_insensitive_search() {
        let wf = WebFetch {
            url: "http://example.com".to_string(),
            mode: FetchMode::Selective,
            search_terms: Some("TARGET".to_string()),
        };
        let text = "intro. filler. target content here. ending.";
        let result = wf.extract_snippets(text);
        assert!(result.contains("target content here"));
    }

    #[tokio::test]
    async fn test_execute_with_mockito() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/test")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<p>Hello from mock</p>")
            .create_async()
            .await;

        let wf = WebFetch {
            url: format!("{}/test", server.url()),
            mode: FetchMode::Full,
            search_terms: None,
        };

        let result = wf.execute().await.unwrap();
        if let ToolExecutionOutputItem::Text(text) = &result.items[0] {
            assert!(text.contains("Hello from mock"));
        } else {
            panic!("Expected text output");
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_execute_truncated_mode() {
        let body = "word ".repeat(5000);
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/big")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(&body)
            .create_async()
            .await;

        let wf = WebFetch {
            url: format!("{}/big", server.url()),
            mode: FetchMode::Truncated,
            search_terms: None,
        };

        let result = wf.execute().await.unwrap();
        if let ToolExecutionOutputItem::Text(text) = &result.items[0] {
            assert!(text.contains("[Content truncated"));
        } else {
            panic!("Expected text output");
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_execute_selective_mode() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/doc")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<p>intro. filler. rust programming. ending.</p>")
            .create_async()
            .await;

        let wf = WebFetch {
            url: format!("{}/doc", server.url()),
            mode: FetchMode::Selective,
            search_terms: Some("rust".to_string()),
        };

        let result = wf.execute().await.unwrap();
        if let ToolExecutionOutputItem::Text(text) = &result.items[0] {
            assert!(text.contains("rust programming"));
        } else {
            panic!("Expected text output");
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_execute_http_error() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server.mock("GET", "/fail").with_status(404).create_async().await;

        let wf = WebFetch {
            url: format!("{}/fail", server.url()),
            mode: FetchMode::Full,
            search_terms: None,
        };

        let result = wf.execute().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_unsupported_content_type() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/binary")
            .with_status(200)
            .with_header("content-type", "application/octet-stream")
            .with_body("binary data")
            .create_async()
            .await;

        let wf = WebFetch {
            url: format!("{}/binary", server.url()),
            mode: FetchMode::Full,
            search_terms: None,
        };

        let result = wf.execute().await;
        assert!(result.is_err());
    }
}
