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
    ToolExecutionResult,
};
use crate::agent::agent_loop::model::Model;
use crate::agent::util::truncate_safe;

const WEB_SEARCH_DESCRIPTION: &str = r#"
WebSearch looks up information that is outside the model's training data or cannot be reliably inferred from the current codebase/context.
"#;

const MAX_QUERY_LENGTH: usize = 200;

const WEB_SEARCH_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "query": {
            "type": "string",
            "maxLength": 200,
            "description": "Search query (max 200 chars) - use concise keywords, not full sentences"
        }
    },
    "required": ["query"]
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearch {
    query: String,
}

impl BuiltInToolTrait for WebSearch {
    fn name() -> BuiltInToolName {
        BuiltInToolName::WebSearch
    }

    fn description() -> Cow<'static, str> {
        WEB_SEARCH_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        WEB_SEARCH_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["web_search"])
    }
}

impl WebSearch {
    pub async fn execute(&self, model: &dyn Model) -> ToolExecutionResult {
        let query = truncate_safe(&self.query, MAX_QUERY_LENGTH);
        let arguments = serde_json::json!({ "query": query });

        let result = model
            .invoke_mcp("web_search", arguments)
            .await
            .map_err(ToolExecutionError::Custom)?;

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(result)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_built_in_tool_trait() {
        assert!(matches!(WebSearch::name(), BuiltInToolName::WebSearch));
        assert!(!WebSearch::description().is_empty());
        assert!(!WebSearch::input_schema().is_empty());
        let aliases = WebSearch::aliases().unwrap();
        assert!(aliases.contains(&"web_search"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let search = WebSearch {
            query: "rust programming".to_string(),
        };
        let json = serde_json::to_string(&search).unwrap();
        let parsed: WebSearch = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.query, search.query);
    }

    #[test]
    fn test_deserialize_query_only() {
        let json = r#"{"query":"my search"}"#;
        let s: WebSearch = serde_json::from_str(json).unwrap();
        assert_eq!(s.query, "my search");
    }

    #[test]
    fn test_max_query_length_const() {
        assert_eq!(MAX_QUERY_LENGTH, 200);
    }

    #[test]
    fn test_description_constants() {
        assert!(WEB_SEARCH_DESCRIPTION.contains("WebSearch"));
        assert!(WEB_SEARCH_SCHEMA.contains("query"));
        assert!(WEB_SEARCH_SCHEMA.contains("maxLength"));
    }
}
