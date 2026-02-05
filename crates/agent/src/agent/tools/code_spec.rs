//! Dynamic tool spec generation for the Code tool based on LSP state.

use crate::agent_loop::types::ToolSpec;

/// Get tool spec based on LSP initialization state
pub fn get_code_tool_spec(lsp_initialized: bool) -> ToolSpec {
    ToolSpec {
        name: "code".to_string(),
        description: if lsp_initialized {
            CODE_TOOL_DESCRIPTION.to_string()
        } else {
            TREESITTER_ONLY_DESCRIPTION.to_string()
        },
        input_schema: serde_json::from_str(if lsp_initialized {
            CODE_TOOL_SCHEMA
        } else {
            TREESITTER_ONLY_SCHEMA
        })
        .expect("valid schema"),
    }
}

pub const TREESITTER_ONLY_DESCRIPTION: &str = r#"
Code intelligence with AST parsing and fuzzy search. Language auto-detected from file extension.

CORE FEATURES:
• Fuzzy search for symbols (classes, functions, methods)
• Extracts function/class signatures via AST
• Structural AST search and rewrite (ast-grep)
• Codebase overview and directory exploration

NOTE: LSP operations (find_references, goto_definition, get_hover, get_completions, get_diagnostics, rename_symbol) require LSP initialization.

## Available Operations
- search_symbols: Find symbol definitions by name
- lookup_symbols: Batch lookup specific symbols
- get_document_symbols: List all symbols in a file
- pattern_search: AST-based structural search
- pattern_rewrite: AST-based code transformation
- generate_codebase_overview: High-level codebase structure
- search_codebase_map: Focused directory exploration
"#;

pub const TREESITTER_ONLY_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "operation": {
            "type": "string",
            "enum": ["search_symbols", "lookup_symbols", "get_document_symbols", "pattern_search", "pattern_rewrite", "generate_codebase_overview", "search_codebase_map"],
            "description": "The code intelligence operation to perform"
        },
        "symbol_name": {
            "type": "string",
            "description": "Symbol name (required for search_symbols)"
        },
        "symbols": {
            "type": "array",
            "items": {"type": "string"},
            "description": "List of symbol names (required for lookup_symbols, max 10)"
        },
        "include_source": {
            "type": "boolean",
            "description": "Include source code in results (optional for lookup_symbols)"
        },
        "file_path": {
            "type": "string",
            "description": "File path (required for get_document_symbols, optional for pattern_search/pattern_rewrite/search_codebase_map)"
        },
        "path": {
            "type": "string",
            "description": "Directory path (optional for search_symbols, generate_codebase_overview, search_codebase_map)"
        },
        "pattern": {
            "type": "string",
            "description": "AST pattern (required for pattern_search/pattern_rewrite)"
        },
        "replacement": {
            "type": "string",
            "description": "Replacement pattern (required for pattern_rewrite)"
        },
        "language": {
            "type": "string",
            "description": "Programming language (required for pattern_search/pattern_rewrite, optional for search_symbols)"
        },
        "limit": {
            "type": "integer",
            "description": "Maximum results (optional)"
        },
        "dry_run": {
            "type": "boolean",
            "description": "Preview changes without writing (optional for pattern_rewrite, defaults to true)"
        },
        "top_level_only": {
            "type": "boolean",
            "description": "Only return top-level symbols (optional for get_document_symbols)"
        }
    },
    "required": ["operation"]
}
"#;

pub const CODE_TOOL_DESCRIPTION: &str = r#"
Code intelligence with full LSP support for semantic code analysis, navigation, and refactoring.

CORE FEATURES:
• Fuzzy search for symbols (classes, functions, methods)
• LSP-powered: find_references, goto_definition, get_hover, get_completions, rename_symbol
• Structural AST search and rewrite (ast-grep)
• Codebase overview and directory exploration

## Read Operations
- search_symbols: Find symbol definitions by name
- lookup_symbols: Batch lookup specific symbols
- find_references: Find all usages of symbol at position
- goto_definition: Navigate to symbol definition
- get_document_symbols: List all symbols in a file
- get_diagnostics: Get compiler errors/warnings
- get_hover: Get type info at position
- get_completions: Get completion suggestions
- pattern_search: AST-based structural search
- generate_codebase_overview: High-level codebase structure
- search_codebase_map: Focused directory exploration

## Write Operations
- rename_symbol: Rename symbol across codebase
- format: Format code
- pattern_rewrite: AST-based code transformation
"#;

pub const CODE_TOOL_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "operation": {
            "type": "string",
            "enum": [
                "search_symbols", "lookup_symbols", "find_references", "goto_definition",
                "get_document_symbols", "get_diagnostics", "get_hover", "get_completions",
                "initialize_workspace", "pattern_search", "pattern_rewrite",
                "generate_codebase_overview", "search_codebase_map",
                "rename_symbol", "format"
            ],
            "description": "The code intelligence operation to perform"
        },
        "symbol_name": {
            "type": "string",
            "description": "Symbol name (required for search_symbols)"
        },
        "symbols": {
            "type": "array",
            "items": {"type": "string"},
            "description": "List of symbol names (required for lookup_symbols, max 10)"
        },
        "include_source": {
            "type": "boolean",
            "description": "Include source code in results (optional for lookup_symbols)"
        },
        "file_path": {
            "type": "string",
            "description": "File path (required for most operations)"
        },
        "path": {
            "type": "string",
            "description": "Directory path (optional)"
        },
        "row": {
            "type": "integer",
            "description": "Line number 1-based"
        },
        "column": {
            "type": "integer",
            "description": "Column number 1-based"
        },
        "pattern": {
            "type": "string",
            "description": "AST pattern (required for pattern_search/pattern_rewrite)"
        },
        "replacement": {
            "type": "string",
            "description": "Replacement pattern (required for pattern_rewrite)"
        },
        "language": {
            "type": "string",
            "description": "Programming language (required for pattern operations)"
        },
        "new_name": {
            "type": "string",
            "description": "New name (required for rename_symbol)"
        },
        "limit": {
            "type": "integer",
            "description": "Maximum results"
        },
        "dry_run": {
            "type": "boolean",
            "description": "Preview changes without writing (defaults to true)"
        }
    },
    "required": ["operation"]
}
"#;
