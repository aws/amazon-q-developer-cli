use std::borrow::Cow;

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
};
use crate::agent::agent_config::parse::CanonicalToolName;

const TOOL_SEARCH_DESCRIPTION: &str = "Find and load MCP tools. Before refusing any task, check if an MCP tool exists. Use tool_id for exact match or query for keyword search — both load matching tools in a single call. Returns tools that are immediately available for invocation.\n\nExample flow:\n1. Call tool_search with query=\"search documents\" or tool_id=\"builder-mcp::InternalSearch\"\n2. Response: {\"tools\":[{\"tool_name\":\"InternalSearch\",\"server_name\":\"builder-mcp\",...}]}\n3. Invoke the tool using ONLY the tool_name value: InternalSearch (not builder-mcp::InternalSearch)";

const TOOL_SEARCH_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "tool_id": {
            "type": "string",
            "description": "Exact tool identifier to load (from available-deferred-tools list, format: server_name::tool_name). The tool is loaded and immediately available."
        },
        "query": {
            "type": "string",
            "description": "Keywords to search for tools. All matching tools are loaded and immediately available."
        },
        "max_results": {
            "type": "integer",
            "description": "Maximum number of results to return (default: 5)"
        }
    }
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearch {
    pub tool_id: Option<String>,
    pub query: Option<String>,
    pub max_results: Option<u32>,
}

impl BuiltInToolTrait for ToolSearch {
    fn name() -> BuiltInToolName {
        BuiltInToolName::ToolSearch
    }

    fn description() -> Cow<'static, str> {
        TOOL_SEARCH_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        TOOL_SEARCH_SCHEMA.into()
    }
}

/// Side effects from execute that caller must apply
#[derive(Debug)]
pub struct ToolSearchSideEffects {
    pub tools_to_activate: Vec<CanonicalToolName>,
}

/// Parse a composite key in server_name::tool_name format.
fn parse_composite_key(name: &str) -> Result<(&str, &str), ToolExecutionError> {
    name.split_once("::").ok_or_else(|| {
        ToolExecutionError::Custom(
            "tool_id must use server_name::tool_name format (e.g. myserver::mytool). See available-deferred-tools list.".to_string(),
        )
    })
}

impl ToolSearch {
    /// Execute tool loading. Supports two modes:
    /// - `tool_id` - exact match via get_entry() (format: server_name::tool_name)
    /// - `query` - BM25 search with threshold filtering
    pub fn execute(
        tool_id: Option<&str>,
        query: Option<&str>,
        max_results: Option<u32>,
        index: &crate::agent::tool_index::ToolIndex,
        limits: &crate::agent::tool_index::ToolLoadConfig,
    ) -> Result<(ToolExecutionOutput, ToolSearchSideEffects), ToolExecutionError> {
        use crate::agent::tool_index::{
            ToolLoadResult,
            ToolSearchResponse,
        };

        // Validate: exactly one of tool_id or query must be provided
        match (tool_id, query) {
            (Some(_), Some(_)) => {
                return Err(ToolExecutionError::Custom(
                    "Provide either tool_id or query, not both".to_string(),
                ));
            },
            (None, None) => {
                return Err(ToolExecutionError::Custom(
                    "Provide either tool_id or query".to_string(),
                ));
            },
            _ => {},
        }

        let limit = max_results.unwrap_or(5) as usize;

        // Exact match mode
        if let Some(name) = tool_id {
            let name = name.trim();
            let (server_name, tool_name) = parse_composite_key(name)?;
            return if let Some(entry) = index.get_entry(server_name, tool_name) {
                let result = ToolLoadResult {
                    tool_name: entry.tool_name.clone(),
                    server_name: entry.server_name.clone(),
                    description: entry.description.clone(),
                    score: f32::MAX,
                };
                let response = ToolSearchResponse { tools: vec![result] };
                Ok((
                    ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                        serde_json::to_string_pretty(&response).unwrap_or_default(),
                    )]),
                    ToolSearchSideEffects {
                        tools_to_activate: vec![CanonicalToolName::from_mcp_parts(
                            entry.server_name.clone(),
                            entry.tool_name.clone(),
                        )],
                    },
                ))
            } else {
                Err(ToolExecutionError::Custom(format!(
                    "Tool '{}' not found. Check the available-deferred-tools list for valid server_name::tool_name entries.",
                    name
                )))
            };
        }

        // BM25 keyword search mode - only return tools above threshold
        let q = query.unwrap();
        let results = index.search(q, limit);

        let mut tools_to_activate = Vec::new();
        let tools: Vec<ToolLoadResult> = results
            .into_iter()
            .filter(|r| r.score >= limits.matching_threshold)
            .map(|r| {
                tools_to_activate.push(CanonicalToolName::from_mcp_parts(
                    r.server_name.clone(),
                    r.tool_name.clone(),
                ));
                ToolLoadResult {
                    tool_name: r.tool_name,
                    server_name: r.server_name,
                    description: r.description,
                    score: r.score,
                }
            })
            .collect();

        let response = ToolSearchResponse { tools };
        Ok((
            ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(
                serde_json::to_string_pretty(&response).unwrap_or_default(),
            )]),
            ToolSearchSideEffects { tools_to_activate },
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::agent::agent_loop::types::ToolSpec;
    use crate::agent::tool_index::{
        ToolIndex,
        ToolLoadConfig,
    };

    fn make_tool_spec(name: &str, desc: &str) -> ToolSpec {
        ToolSpec {
            name: name.to_string(),
            description: desc.to_string(),
            input_schema: serde_json::Map::new(),
        }
    }

    fn make_test_index() -> ToolIndex {
        let mut specs: HashMap<String, Vec<ToolSpec>> = HashMap::new();
        specs.insert("testserver".to_string(), vec![make_tool_spec("mytool", "A test tool")]);
        ToolIndex::from_tool_specs(&specs)
    }

    fn default_limits() -> ToolLoadConfig {
        ToolLoadConfig::default()
    }

    #[test]
    fn test_exact_match_with_composite_key_succeeds() {
        let index = make_test_index();
        let limits = default_limits();

        let result = ToolSearch::execute(Some("testserver::mytool"), None, None, &index, &limits);

        assert!(result.is_ok());
        let (_output, side_effects) = result.unwrap();
        assert_eq!(side_effects.tools_to_activate.len(), 1);
        assert_eq!(side_effects.tools_to_activate[0].tool_name(), "mytool");
    }

    #[test]
    fn test_exact_match_without_separator_fails() {
        let index = make_test_index();
        let limits = default_limits();

        let result = ToolSearch::execute(Some("mytool"), None, None, &index, &limits);

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = format!("{:?}", err);
        assert!(err_msg.contains("server_name::tool_name"));
    }

    #[test]
    fn test_exact_match_wrong_composite_key_returns_error() {
        let index = make_test_index();
        let limits = default_limits();

        let result = ToolSearch::execute(Some("wrongserver::mytool"), None, None, &index, &limits);

        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("not found"));
    }
}
