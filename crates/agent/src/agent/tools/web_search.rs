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
