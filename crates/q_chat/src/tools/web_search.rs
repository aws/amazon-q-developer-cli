use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Stylize,
};
use eyre::Result;
use fig_os_shim::Context;
use fig_request::reqwest;
use htmd::HtmlToMarkdown;
use serde::Deserialize;

use super::{
    InvokeOutput,
    OutputKind,
};

#[derive(Debug, Clone, Deserialize)]
pub struct WebSearch {
    pub query: Option<String>,
    pub mode: WebSearchMode,
    pub target_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub enum WebSearchMode {
    Search,
    Retrieve,
}

impl WebSearch {
    pub async fn invoke(&self, _updates: impl Write) -> Result<InvokeOutput> {
        let query = self.query.as_deref().unwrap_or("");
        let target_url = self.target_url.as_deref().unwrap_or("");

        // Perform web search or retrieve based on the mode
        match self.mode {
            // TODO - need to align on what search engine to use
            WebSearchMode::Search => {
                if query.is_empty() {
                    return Err(eyre::eyre!("Query is required for web search"));
                }
                // Perform web search using the query
                // ...
            },
            WebSearchMode::Retrieve => {
                if target_url.is_empty() {
                    return Err(eyre::eyre!("Target URL is required for retrieving"));
                }

                // Parse the target URL to get the base domain for robots.txt
                let parsed_url =
                    url::Url::parse(target_url).map_err(|e| eyre::eyre!("Failed to parse target URL: {}", e))?;

                // Construct robots.txt URL
                let robots_url = format!(
                    "{}://{}/robots.txt",
                    parsed_url.scheme(),
                    parsed_url
                        .host_str()
                        .ok_or_else(|| eyre::eyre!("Invalid host in URL"))?
                );

                let user_agent = "AmazonQCLI/1.0";
                let client = reqwest::Client::new();

                // Check robots.txt first
                let robots_resp = client.get(&robots_url).send().await;
                // If robots.txt exists, check if we're allowed to access
                if let Ok(robots_resp) = robots_resp {
                    if robots_resp.status().is_success() {
                        let robots_content = robots_resp
                            .text()
                            .await
                            .map_err(|e| eyre::eyre!("Failed to read robots.txt: {}", e))?;

                        // Simple robots.txt parsing
                        let path = parsed_url.path();
                        if !Self::is_allowed_by_robots_txt(&robots_content, user_agent, path) {
                            return Err(eyre::eyre!("Access to this URL is disallowed by robots.txt"));
                        }
                    }
                }

                // Send a GET request to the target URL with a custom User-Agent header
                let response = client
                    .get(target_url)
                    .header(reqwest::header::USER_AGENT, user_agent)
                    .send()
                    .await
                    .map_err(|e| eyre::eyre!("Failed to connect to target URL: {}", e))?;

                // Check if the request was successful
                if !response.status().is_success() {
                    return Err(eyre::eyre!("Request failed with status: {}", response.status()));
                }
                // Get the response body as text
                let html_string = response
                    .text()
                    .await
                    .map_err(|e| eyre::eyre!("Failed to read response body: {}", e))?;

                // Convert HTML to Markdown
                let converter = HtmlToMarkdown::builder().skip_tags(vec!["script", "style"]).build();

                return Ok(InvokeOutput {
                    output: OutputKind::Json(serde_json::json!({
                        "mkd_content": converter.convert(&html_string).unwrap(),
                        "target_url": target_url,
                    })),
                });
            },
        }

        Ok(Default::default())
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print(format!(
                "{} {}...",
                if self.mode == WebSearchMode::Search {
                    "Searching"
                } else {
                    "Retrieving"
                },
                if self.mode == WebSearchMode::Search {
                    self.query.as_ref().unwrap_or(&"".to_string()).clone().dark_green()
                } else {
                    self.target_url.as_ref().unwrap_or(&"".to_string()).clone().dark_green()
                }
            )),
        )?;
        Ok(())
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        if self.mode == WebSearchMode::Search && self.query.is_none() {
            return Err(eyre::eyre!("Query is required for web search"));
        }
        if self.mode == WebSearchMode::Retrieve && self.target_url.is_none() {
            return Err(eyre::eyre!("Target URL is required for retrieving"));
        }

        Ok(())
    }

    // Simple function to check if a path is allowed by robots.txt
    fn is_allowed_by_robots_txt(robots_content: &str, user_agent: &str, path: &str) -> bool {
        let mut current_agent;
        let mut disallowed_paths = Vec::new();
        let mut is_relevant_agent = false;

        // Very basic robots.txt parser
        for line in robots_content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse User-agent line
            if let Some(agent) = line.strip_prefix("User-agent:") {
                current_agent = agent.trim();
                is_relevant_agent = current_agent == "*" || current_agent == user_agent;
                continue;
            }

            // Parse Disallow line if it's for our user agent
            if is_relevant_agent {
                if let Some(disallow_path) = line.strip_prefix("Disallow:") {
                    let disallow_path = disallow_path.trim();
                    if !disallow_path.is_empty() {
                        disallowed_paths.push(disallow_path);
                    }
                }
            }
        }

        // Check if the path is disallowed
        for disallow in &disallowed_paths {
            if path.starts_with(disallow) || *disallow == "/" {
                return false;
            }
        }

        true
    }
}
