use std::collections::HashMap;

use agent::mcp::types::Prompt;

use super::schema::PromptArgumentInfo;

/// Resolve the argument schema for a named prompt from the full prompt map.
pub fn resolve_prompt_schema(prompts: HashMap<String, Vec<Prompt>>, name: &str) -> Option<Vec<PromptArgumentInfo>> {
    prompts.into_values().flatten().find(|p| p.name == name).and_then(|p| {
        p.arguments.map(|args| {
            args.into_iter()
                .map(|a| PromptArgumentInfo {
                    name: a.name,
                    description: a.description,
                    required: a.required.unwrap_or(false),
                })
                .collect()
        })
    })
}

/// Convert positional args to a HashMap for MCP prompt arguments.
/// When a schema is provided, maps positional args to schema parameter names.
/// Falls back to generic arg0/arg1 names when no schema is available.
pub fn args_to_mcp_map(args: &[String], schema: Option<&[PromptArgumentInfo]>) -> HashMap<String, String> {
    match schema {
        Some(schema) => schema
            .iter()
            .zip(args.iter())
            .map(|(param, val)| (param.name.clone(), val.clone()))
            .collect(),
        None => args
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("arg{i}"), v.clone()))
            .collect(),
    }
}

/// Extract text content from resolved MCP prompt messages.
pub fn extract_prompt_text(messages: &[serde_json::Value]) -> String {
    messages
        .iter()
        .filter_map(|m| m.get("content")?.get("text")?.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use agent::mcp::types::PromptArgument;

    use super::*;

    // --- args_to_mcp_map ---

    #[test]
    fn test_args_to_mcp_map_no_schema() {
        let args = vec!["a".to_string(), "b".to_string()];
        let map = args_to_mcp_map(&args, None);
        assert_eq!(map.get("arg0"), Some(&"a".to_string()));
        assert_eq!(map.get("arg1"), Some(&"b".to_string()));
    }

    #[test]
    fn test_args_to_mcp_map_with_schema() {
        let args = vec!["my-test-id".to_string()];
        let schema = vec![PromptArgumentInfo {
            name: "testRunIdentifier".to_string(),
            description: None,
            required: true,
        }];
        let map = args_to_mcp_map(&args, Some(&schema));
        assert_eq!(map.get("testRunIdentifier"), Some(&"my-test-id".to_string()));
        assert_eq!(map.get("arg0"), None);
    }

    #[test]
    fn test_args_to_mcp_map_empty() {
        let map = args_to_mcp_map(&[], None);
        assert!(map.is_empty());
    }

    #[test]
    fn test_args_to_mcp_map_more_args_than_schema() {
        let args = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let schema = vec![PromptArgumentInfo {
            name: "first".to_string(),
            description: None,
            required: true,
        }];
        let map = args_to_mcp_map(&args, Some(&schema));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("first"), Some(&"a".to_string()));
    }

    #[test]
    fn test_args_to_mcp_map_more_schema_than_args() {
        let args = vec!["a".to_string()];
        let schema = vec![
            PromptArgumentInfo {
                name: "first".to_string(),
                description: None,
                required: true,
            },
            PromptArgumentInfo {
                name: "second".to_string(),
                description: None,
                required: false,
            },
        ];
        let map = args_to_mcp_map(&args, Some(&schema));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("first"), Some(&"a".to_string()));
        assert_eq!(map.get("second"), None);
    }

    // --- resolve_prompt_schema ---

    fn make_prompt(name: &str, args: Option<Vec<(&str, Option<bool>)>>) -> Prompt {
        Prompt {
            name: name.to_string(),
            description: None,
            arguments: args.map(|a| {
                a.into_iter()
                    .map(|(n, req)| PromptArgument {
                        name: n.to_string(),
                        description: None,
                        required: req,
                    })
                    .collect()
            }),
        }
    }

    #[test]
    fn test_resolve_prompt_schema_found() {
        let mut prompts = HashMap::new();
        prompts.insert("server1".to_string(), vec![make_prompt(
            "fix-test",
            Some(vec![("testId", Some(true))]),
        )]);
        let schema = resolve_prompt_schema(prompts, "fix-test").unwrap();
        assert_eq!(schema.len(), 1);
        assert_eq!(schema[0].name, "testId");
        assert!(schema[0].required);
    }

    #[test]
    fn test_resolve_prompt_schema_not_found() {
        let mut prompts = HashMap::new();
        prompts.insert("server1".to_string(), vec![make_prompt("other", None)]);
        assert!(resolve_prompt_schema(prompts, "fix-test").is_none());
    }

    #[test]
    fn test_resolve_prompt_schema_no_arguments() {
        let mut prompts = HashMap::new();
        prompts.insert("server1".to_string(), vec![make_prompt("fix-test", None)]);
        assert!(resolve_prompt_schema(prompts, "fix-test").is_none());
    }

    #[test]
    fn test_resolve_prompt_schema_across_servers() {
        let mut prompts = HashMap::new();
        prompts.insert("server1".to_string(), vec![make_prompt("other", None)]);
        prompts.insert("server2".to_string(), vec![make_prompt(
            "fix-test",
            Some(vec![("id", Some(true))]),
        )]);
        let schema = resolve_prompt_schema(prompts, "fix-test").unwrap();
        assert_eq!(schema[0].name, "id");
    }

    #[test]
    fn test_resolve_prompt_schema_required_defaults_false() {
        let mut prompts = HashMap::new();
        prompts.insert("s".to_string(), vec![make_prompt("p", Some(vec![("arg", None)]))]);
        let schema = resolve_prompt_schema(prompts, "p").unwrap();
        assert!(!schema[0].required);
    }

    // --- extract_prompt_text ---

    #[test]
    fn test_extract_prompt_text() {
        let messages = vec![
            serde_json::json!({"content": {"text": "Hello"}}),
            serde_json::json!({"content": {"text": "World"}}),
        ];
        assert_eq!(extract_prompt_text(&messages), "Hello\nWorld");
    }

    #[test]
    fn test_extract_prompt_text_empty() {
        assert_eq!(extract_prompt_text(&[]), "");
    }

    #[test]
    fn test_extract_prompt_text_skips_non_text() {
        let messages = vec![
            serde_json::json!({"content": {"text": "Hello"}}),
            serde_json::json!({"content": {"image": "data"}}),
            serde_json::json!({"role": "user"}),
        ];
        assert_eq!(extract_prompt_text(&messages), "Hello");
    }
}
