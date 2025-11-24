use std::io::Write;

use crossterm::{
    queue,
    style,
};
use eyre::{
    Result,
    WrapErr,
};
use serde::Deserialize;

use super::{
    InvokeOutput,
    OutputKind,
    ToolInfo,
};
use crate::auth::UnifiedBearerResolver;
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::database::Database;
use crate::os::Os;

#[derive(Debug, Clone, Deserialize)]
pub struct WebSearch {
    query: String,
}

impl WebSearch {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "web_search",
        preferred_alias: "web_search",
        aliases: &["web_search"],
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

        if is_in_allowlist {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }
    }

    pub async fn invoke(&self, _os: &Os, updates: impl Write) -> Result<InvokeOutput> {
        let search_results = self.call_invoke_mcp().await?;

        let result_count = search_results
            .get("results")
            .and_then(|r| r.as_array())
            .map_or(0, |a| a.len());

        super::queue_function_result(
            &format!("Found {result_count} search results"),
            &mut std::io::BufWriter::new(updates),
            false,
            false,
        )?;

        Ok(InvokeOutput {
            output: OutputKind::Json(search_results),
        })
    }

    async fn call_invoke_mcp(&self) -> Result<serde_json::Value> {
        use amzn_codewhisperer_streaming_client::Client as CodewhispererStreamingClient;
        use amzn_codewhisperer_streaming_client::config::endpoint::{
            Endpoint as StreamingEndpoint,
            Params as StreamingParams,
            ResolveEndpoint,
        };
        use aws_smithy_runtime_api::client::endpoint::EndpointFuture;

        let database = Database::new().await?;

        // Get endpoint with region (same pattern as generateAssistantResponse)
        let endpoint = crate::api_client::Endpoint::configured_value(&database);

        // Create a static endpoint resolver
        #[derive(Debug)]
        struct StaticEndpointResolver {
            url: String,
        }

        impl StaticEndpointResolver {
            fn new(url: String) -> Self {
                Self { url }
            }
        }

        impl ResolveEndpoint for StaticEndpointResolver {
            fn resolve_endpoint<'a>(&'a self, _params: &'a StreamingParams) -> EndpointFuture<'a> {
                let url = self.url.clone();
                let endpoint = StreamingEndpoint::builder().url(url).build();
                EndpointFuture::ready(Ok(endpoint))
            }
        }

        // Build streaming client with endpoint's region and retry config
        let retry_config = aws_config::retry::RetryConfig::adaptive()
            .with_max_attempts(3)
            .with_max_backoff(std::time::Duration::from_secs(10));

        let bearer_sdk_config = aws_config::defaults(crate::aws_common::behavior_version())
            .region(endpoint.region.clone())
            .retry_config(retry_config)
            .load()
            .await;

        let client = CodewhispererStreamingClient::from_conf(
            amzn_codewhisperer_streaming_client::config::Builder::from(&bearer_sdk_config)
                .http_client(crate::aws_common::http_client::client())
                .bearer_token_resolver(UnifiedBearerResolver)
                .app_name(crate::aws_common::app_name())
                .endpoint_resolver(StaticEndpointResolver::new(endpoint.url().to_string()))
                .build(),
        );

        // Build invoke_mcp request
        let params = aws_smithy_types::Document::Object(
            [
                (
                    "name".to_string(),
                    aws_smithy_types::Document::String("web_search".to_string()),
                ),
                (
                    "arguments".to_string(),
                    aws_smithy_types::Document::Object(
                        [(
                            "query".to_string(),
                            aws_smithy_types::Document::String(self.query.clone()),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        );

        let response = client
            .invoke_mcp()
            .jsonrpc("2.0")
            .id("1")
            .method(amzn_codewhisperer_streaming_client::types::McpMethod::ToolsCall)
            .params(params)
            .send()
            .await
            .wrap_err("Failed to invoke MCP")?;

        // Check for error
        if let Some(error) = response.error() {
            return Err(eyre::eyre!("MCP error: {:?}", error));
        }

        // Extract result
        let result_doc = response
            .result()
            .ok_or_else(|| eyre::eyre!("No result in MCP response"))?;

        // Check for isError field before conversion
        if let aws_smithy_types::Document::Object(map) = result_doc {
            if let Some(aws_smithy_types::Document::Bool(true)) = map.get("isError") {
                // Extract error message from Document
                let error_msg = map
                    .get("content")
                    .and_then(|c| match c {
                        aws_smithy_types::Document::Array(arr) => arr.first(),
                        _ => None,
                    })
                    .and_then(|item| match item {
                        aws_smithy_types::Document::Object(obj) => obj.get("text"),
                        _ => None,
                    })
                    .and_then(|text| match text {
                        aws_smithy_types::Document::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("Unknown error");
                return Err(eyre::eyre!("Web search failed: {}", error_msg));
            }
        }

        // Convert Document to JSON string manually
        let result_json = document_to_json(result_doc)?;

        // Try to handle both string and object responses
        let content_text: String = if let Some(result_str) = result_json.as_str() {
            // Case 1: Result is a JSON string (double-encoded)
            let first_level: serde_json::Value =
                serde_json::from_str(result_str).wrap_err("Failed to parse first level JSON")?;

            first_level
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| eyre::eyre!("No text in content array"))?
        } else if result_json.is_object() {
            // Case 2: Result is already an object (not double-encoded)
            result_json
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| eyre::eyre!("No text in content array"))?
        } else {
            return Err(eyre::eyre!("Unexpected result type: {:?}", result_json));
        };

        // Parse the second level of JSON (the actual search results)
        serde_json::from_str(&content_text)
            .wrap_err_with(|| format!("Failed to parse search results from: {content_text}"))
    }

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("Searching the web for: "))?;
        queue!(output, style::Print(&self.query))?;
        super::display_tool_use(tool, output)?;
        queue!(output, style::Print("\n"))?;
        Ok(())
    }

    pub async fn validate(&self, _os: &Os) -> Result<()> {
        if self.query.trim().is_empty() {
            return Err(eyre::eyre!("Search query cannot be empty"));
        }
        Ok(())
    }
}

// Helper function to convert aws_smithy_types::Document to serde_json::Value
fn document_to_json(doc: &aws_smithy_types::Document) -> Result<serde_json::Value> {
    match doc {
        aws_smithy_types::Document::Object(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                json_map.insert(k.clone(), document_to_json(v)?);
            }
            Ok(serde_json::Value::Object(json_map))
        },
        aws_smithy_types::Document::Array(arr) => {
            let json_arr: Result<Vec<_>> = arr.iter().map(document_to_json).collect();
            Ok(serde_json::Value::Array(json_arr?))
        },
        aws_smithy_types::Document::Number(n) => Ok(serde_json::Value::Number(
            serde_json::Number::from_f64(n.to_f64_lossy()).ok_or_else(|| eyre::eyre!("Invalid number in response"))?,
        )),
        aws_smithy_types::Document::String(s) => Ok(serde_json::Value::String(s.clone())),
        aws_smithy_types::Document::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        aws_smithy_types::Document::Null => Ok(serde_json::Value::Null),
    }
}
