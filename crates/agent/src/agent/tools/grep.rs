#![allow(dead_code)]

use serde::{
    Deserialize,
    Serialize,
};

const GREP_TOOL_DESCRIPTION: &str = r#"
A tool for searching file content.
"#;

const GREP_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "base": {
            "type": "string",
            "description": "Path to the directory to start the search from. Defaults to current working directory"
        },
        "pattern": {
            "type": "integer",
            "description": "Regex to search files for",
            "default": 0
        },
        "paths": {
            "type": "array",
            "description": "List of file paths to search. Supports glob matching",
            "items": {
                "type": "string",
                "description": "Glob pattern"
            }
        }
    },
    "required": [
        "pattern"
    ]
}
"#;

// impl BuiltInToolTrait for Grep {
//     const DESCRIPTION: &str = GREP_TOOL_DESCRIPTION;
//     const INPUT_SCHEMA: &str = GREP_SCHEMA;
//     const NAME: BuiltInToolName = BuiltInToolName::Grep;
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grep {
    pattern: String,
    base: Option<String>,
    paths: Option<String>,
}

impl Grep {}
