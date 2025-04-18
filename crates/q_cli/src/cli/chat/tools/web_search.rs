use std::io::Write;


use crossterm::queue;
use crossterm::style::{
    self,
    Stylize,
};
use eyre::Result;
use fig_os_shim::Context;
use fig_request::reqwest;
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
    Scrape
}

impl WebSearch {
    pub async fn invoke(&self, _updates: impl Write) -> Result<InvokeOutput> {
        let query = self.query.as_deref().unwrap_or("");
        let target_url = self.target_url.as_deref().unwrap_or("");

        // Perform web search or scrape based on the mode
        match self.mode {
            WebSearchMode::Search => {
                if query.is_empty() {
                    return Err(eyre::eyre!("Query is required for web search"));
                }
                // Perform web search using the query
                // ...
            },
            WebSearchMode::Scrape => {
                if target_url.is_empty() {
                    return Err(eyre::eyre!("Target URL is required for scraping"));
                }
                // Perform web scraping using the target URL

                let client = reqwest::Client::new();
                // Send a GET request to the target URL
                let response = client
                    .get(target_url)
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

                return Ok(InvokeOutput {
                    output: OutputKind::Json(serde_json::json!({
                        "html_content": html_string,
                        "target_url": target_url,
                    })),
                })
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
                    "Scraping"
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
        if self.mode == WebSearchMode::Scrape && self.target_url.is_none() {
            return Err(eyre::eyre!("Target URL is required for scraping"));
        }

        Ok(())
    }
}
