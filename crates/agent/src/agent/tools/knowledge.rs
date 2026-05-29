use std::borrow::Cow;

use async_trait::async_trait;
use serde::Deserialize;

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionResult,
};

/// Abstraction over the knowledge store so the agent crate doesn't depend on
/// any concrete implementation. The host injects an implementation backed by
/// `semantic-search-client` (or any other knowledge backend it chooses).
#[async_trait]
pub trait KnowledgeProvider: std::fmt::Debug + Send + Sync {
    async fn execute(&self, command: Knowledge) -> ToolExecutionResult;

    /// Returns a formatted listing of available knowledge bases for context
    /// injection, matching V1's `format_knowledge_bases_as_context()` behavior.
    /// Returns `None` if no knowledge bases are available.
    async fn list_available(&self) -> Option<String>;
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(tag = "command", rename_all = "lowercase")]
pub enum Knowledge {
    Show,
    Status,
    Add {
        name: String,
        value: String,
    },
    Remove {
        #[serde(default)]
        name: String,
        #[serde(default)]
        context_id: String,
        #[serde(default)]
        path: String,
    },
    Clear,
    Search {
        query: String,
        #[serde(default)]
        context_id: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        snippet_length: Option<usize>,
        #[serde(default)]
        sort_by: Option<String>,
        #[serde(default)]
        file_type: Option<String>,
    },
    Update {
        #[serde(default)]
        path: String,
        #[serde(default)]
        context_id: String,
        #[serde(default)]
        name: String,
    },
    Cancel {
        #[serde(default)]
        operation_id: Option<String>,
    },
}

const KNOWLEDGE_DESCRIPTION: &str = r#"A tool for indexing and searching content across chat sessions using semantic search.

## Overview
This tool enables persistent storage and retrieval of information using semantic search (MiniLLM) or keyword search (BM25). Content remains available across sessions for later use.

## When to use
- When users ask to query your knowledge bases or kbs
- When you need to search previously indexed content
- When users request to index new content (code, markdown, CSV, PDF, and other text file formats)
- When exploring unfamiliar content to find relevant information
- When users ask about topics that might be in indexed knowledge bases

## When not to use
- When content has not been indexed yet and user hasn't requested indexing
- When you need real-time or external information not in the knowledge base

## Notes
- Use 'show' command to list available knowledge bases before searching
- Search can target specific knowledge bases (context_id) or all knowledge bases
- Use default limit values unless specifically needed; fewer results for focused search
- Pagination available via offset parameter for large result sets
- 'add' command indexes new content; 'update' command refreshes existing knowledge bases
- Unless there is a clear reason to modify the search query, use the user's original wording for better semantic matching"#;

const KNOWLEDGE_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "command": {
            "type": "string",
            "enum": ["show", "add", "remove", "clear", "search", "update", "status", "cancel"],
            "description": "The knowledge operation to perform:\n- 'show': List all knowledge contexts (no additional parameters required)\n- 'add': Add content to knowledge base (requires 'name' and 'value')\n- 'remove': Remove content from knowledge base (requires one of: 'name', 'context_id', or 'path')\n- 'clear': Remove all knowledge contexts.\n- 'search': Search across knowledge contexts (requires 'query', optional: 'context_id', 'limit', 'offset', 'snippet_length', 'sort_by', 'file_type')\n- 'update': Update existing context with new content (requires 'path' and one of: 'name', 'context_id')\n- 'status': Show background operation status and progress\n- 'cancel': Cancel background operations (optional 'operation_id' to cancel specific operation, or cancel all if not provided)"
        },
        "name": {
            "type": "string",
            "description": "A descriptive name for the knowledge context. Required for 'add' operations. Can be used for 'remove' and 'update' operations to identify the context."
        },
        "value": {
            "type": "string",
            "description": "The content to store in knowledge base. Required for 'add' operations. Can be either text content or a file/directory path. If it's a valid file or directory path, the content will be indexed; otherwise it's treated as text."
        },
        "context_id": {
            "type": "string",
            "description": "The unique context identifier for targeted operations. Can be obtained from 'show' command. Used for 'remove', 'update', and 'search' operations to specify which context to operate on."
        },
        "path": {
            "type": "string",
            "description": "File or directory path. Used in 'remove' operations to remove contexts by their source path, and required for 'update' operations to specify the new content location."
        },
        "query": {
            "type": "string",
            "description": "The search query string. Required for 'search' operations. Performs semantic search across knowledge contexts to find relevant content."
        },
        "limit": {
            "type": "integer",
            "description": "Maximum number of search results to return, use default value unless required more results or focused search. Optional for 'search' operations."
        },
        "offset": {
            "type": "integer",
            "description": "Number of results to skip for pagination. Optional for 'search' operations."
        },
        "snippet_length": {
            "type": "integer",
            "description": "Maximum character length for text snippets in results. Text longer than this will be truncated. Optional for 'search' operations."
        },
        "sort_by": {
            "type": "string",
            "enum": ["relevance", "path", "name"],
            "description": "Sort order for search results. Options: 'relevance' (default, by similarity score), 'path' or 'name' (alphabetically by file path). Optional for 'search' operations."
        },
        "file_type": {
            "type": "string",
            "description": "Filter results by file type (e.g., 'Code', 'Markdown', 'Text'). Optional for 'search' operations."
        },
        "operation_id": {
            "type": "string",
            "description": "Optional operation ID to cancel a specific operation. Used with 'cancel' command. If not provided, all active operations will be cancelled. Can be either the full operation ID or the short 8-character ID."
        }
    },
    "required": ["command"]
}"#;

impl BuiltInToolTrait for Knowledge {
    fn name() -> BuiltInToolName {
        BuiltInToolName::Knowledge
    }

    fn description() -> Cow<'static, str> {
        KNOWLEDGE_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        KNOWLEDGE_SCHEMA.into()
    }
}

impl Knowledge {
    pub async fn execute(self, provider: &dyn KnowledgeProvider) -> ToolExecutionResult {
        provider.execute(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_built_in_tool_trait() {
        assert!(matches!(Knowledge::name(), BuiltInToolName::Knowledge));
        assert!(!Knowledge::description().is_empty());
        assert!(!Knowledge::input_schema().is_empty());
    }

    #[test]
    fn test_serde_show() {
        let json = r#"{"command":"show"}"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        assert!(matches!(k, Knowledge::Show));
    }

    #[test]
    fn test_serde_status() {
        let json = r#"{"command":"status"}"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        assert!(matches!(k, Knowledge::Status));
    }

    #[test]
    fn test_serde_clear() {
        let json = r#"{"command":"clear"}"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        assert!(matches!(k, Knowledge::Clear));
    }

    #[test]
    fn test_serde_add() {
        let json = r#"{"command":"add","name":"mykb","value":"data"}"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        match k {
            Knowledge::Add { name, value } => {
                assert_eq!(name, "mykb");
                assert_eq!(value, "data");
            },
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_serde_search_minimal() {
        let json = r#"{"command":"search","query":"q"}"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        match k {
            Knowledge::Search { query, .. } => assert_eq!(query, "q"),
            _ => panic!("expected Search"),
        }
    }

    #[test]
    fn test_serde_search_with_options() {
        let json = r#"{
            "command":"search",
            "query":"q",
            "context_id":"x",
            "limit":10,
            "offset":5,
            "snippet_length":50,
            "sort_by":"path",
            "file_type":"Code"
        }"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        match k {
            Knowledge::Search {
                query,
                context_id,
                limit,
                offset,
                snippet_length,
                sort_by,
                file_type,
            } => {
                assert_eq!(query, "q");
                assert_eq!(context_id, Some("x".to_string()));
                assert_eq!(limit, Some(10));
                assert_eq!(offset, Some(5));
                assert_eq!(snippet_length, Some(50));
                assert_eq!(sort_by, Some("path".to_string()));
                assert_eq!(file_type, Some("Code".to_string()));
            },
            _ => panic!("expected Search"),
        }
    }

    #[test]
    fn test_serde_remove() {
        let json = r#"{"command":"remove","name":"x"}"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        match k {
            Knowledge::Remove { name, context_id, path } => {
                assert_eq!(name, "x");
                assert_eq!(context_id, "");
                assert_eq!(path, "");
            },
            _ => panic!("expected Remove"),
        }
    }

    #[test]
    fn test_serde_update() {
        let json = r#"{"command":"update","path":"/p","name":"n"}"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        match k {
            Knowledge::Update { path, name, .. } => {
                assert_eq!(path, "/p");
                assert_eq!(name, "n");
            },
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn test_serde_cancel() {
        let json = r#"{"command":"cancel"}"#;
        let k: Knowledge = serde_json::from_str(json).unwrap();
        assert!(matches!(k, Knowledge::Cancel { operation_id: None }));

        let json2 = r#"{"command":"cancel","operation_id":"abc"}"#;
        let k2: Knowledge = serde_json::from_str(json2).unwrap();
        match k2 {
            Knowledge::Cancel { operation_id } => assert_eq!(operation_id, Some("abc".to_string())),
            _ => panic!("expected Cancel"),
        }
    }
}
