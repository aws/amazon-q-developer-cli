//! Dynamic tool spec configuration for code intelligence tools.
//!
//! This module provides dynamic tool specifications based on LSP initialization state.

use super::code::Code;
use crate::cli::chat::ToolSpec;
use crate::cli::chat::tools::InputSchema;

/// Configuration for code intelligence tool specs
pub struct CodeToolConfig {
    /// Whether LSP servers are initialized
    lsp_initialized: bool,
}

impl CodeToolConfig {
    /// Create new config with LSP initialization state
    pub fn new(lsp_initialized: bool) -> Self {
        Self { lsp_initialized }
    }

    /// Get appropriate tool spec based on LSP initialization state
    pub fn get_tool_spec(&self, _allowed_tools: Option<&[String]>) -> ToolSpec {
        if self.lsp_initialized {
            Self::get_full_lsp_spec()
        } else {
            Self::get_treesitter_only_spec()
        }
    }

    /// TreeSitter-only operations (no LSP required) - all operations
    fn get_treesitter_only_spec() -> ToolSpec {
        ToolSpec {
            name: Code::INFO.spec_name.to_string(),
            description: "Code intelligence to analyze code with AST parsing and fuzzy search. Language auto-detected from file extension.

IMPORTANT: Prefer this tool over fs_read for code files. This tool intelligently provides structured code analysis.

CORE FEATURES:
• Fuzzy search for symbols (classes, functions, methods)
• Extracts function/class signatures via AST
• Structural AST search and rewrite (ast-grep)
• Codebase overview and directory exploration

NOTE: Use fs_read with line ranges for unsupported patterns.

## Operations

**search_symbols** - Find symbol definitions by name across workspace.
Params: symbol_name (required), path, symbol_type, limit, language, exact_match
Uses fuzzy matching by default. Set exact_match=true for precise matching.

**lookup_symbols** - Batch lookup specific symbols (max 10).
Params: symbols (required), file_path, include_source
Set include_source=false for large classes/structs to avoid context overflow.
Scoped lookup: Provide file_path to search within specific file only.

**get_document_symbols** - List all symbols in a file.
Params: file_path (required), top_level_only (recommended: true)
Use this for understanding file structure without reading content.
Prefer this over fs_read when you need symbol information.

**pattern_search** - AST-based structural search using ast-grep.
Params: pattern, language (required), file_path, limit

**pattern_rewrite** - AST-based code transformation.
Params: pattern, replacement, language (required), file_path, limit, dry_run
Use dry_run=true first to preview changes.

## Pattern Search & Rewrite

Metavariables:
• $VAR - Matches single node (identifier, expression)
• $$$ - Matches zero or more nodes (statements, parameters)

Simple patterns:
• console.log($ARG) - Match any console.log call
• function $NAME($$$PARAMS) {} - Match function declaration
• $OBJ.$METHOD() - Match any method call

Structural rules (YAML):
• pattern - Match code structure
• kind - Match AST node type (function_declaration, class_declaration)
• has - Node contains descendant matching pattern
• inside - Node is inside ancestor matching pattern
• all/any/not - Logical operators

Critical: For relational rules (has, inside), always use stopBy: end

Rewrite examples:
• pattern='var $N = $V', replacement='const $N = $V'
• pattern='$O.hasOwnProperty($P)', replacement='Object.hasOwn($O, $P)'

Workflow:
1. Use pattern_search first to verify matches
2. Review matches to ensure correctness
3. Test rewrite on small subset
4. Apply to full codebase after verification

**generate_codebase_overview** - ONLY for explicit architecture questions. Not for bug fixes or finding code.
Params: path (optional)

**search_codebase_map** - Focused directory exploration.
Params: file_path, path

## Explore Unfamiliar Code
1. search_symbols for domain-specific types/functions/methods/classes
2. lookup_symbols with include_source=true on promising matches
3. grep (max 2 searches) for literal text patterns
4. Repeat 1-3 with refined terms if needed
5. generate_codebase_overview ONLY for architecture review or full codebase understanding
   - NOT for bug fixes, feature work, or finding specific code

## Tool Selection Priority
1. search_symbols or lookup_symbols for finding code - ALWAYS use for functions/methods/classes/structs/interfaces
2. get_document_symbols for file structure
3. grep - ONLY for literal text in comments/strings, config values, code
4. fs_read - raw file content when needed

## CRITICAL RULES
- Start with search_symbols
- Always follow search_symbols with lookup_symbols
- When searching for multiple related symbols, call search_symbols in parallel

## Quick Reference
• \"What's in this file?\" → get_document_symbols
• \"Show me X class\" → search_symbols, then lookup_symbol

## LSP Operations (Not Available)
These require LSP initialization (/code init):
find_references, goto_definition, get_hover, get_completions, get_diagnostics, rename_symbol".to_string(),
            input_schema: InputSchema(serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["search_symbols", "lookup_symbols", "get_document_symbols", "pattern_search", "generate_codebase_overview", "search_codebase_map", "pattern_rewrite"],
                        "description": "The code intelligence operation to perform"
                        },
                        "symbol_name": {
                            "type": "string",
                            "description": "Simple symbol name, not qualified (e.g. 'myFunction' not 'MyClass.myFunction'). Required for search_symbols"
                        },
                        "symbols": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "List of simple symbol names, not qualified (e.g. ['myFunction'] not ['MyClass.myFunction']). Required for lookup_symbols, max 10"
                        },
                        "include_source": {
                            "type": "boolean",
                            "description": "Include source code in results (optional for lookup_symbols)"
                        },
                        "file_path": {
                            "type": "string",
                            "description": "File path (required for get_document_symbols, optional for pattern_search/search_codebase_map)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory path (optional, for generate_codebase_overview: workspace root, for search_codebase_map: path filter, for search_symbols: scope search to file or directory)"
                        },
                        "pattern": {
                            "type": "string",
                            "description": "AST pattern (required for pattern_search)"
                        },
                        "language": {
                            "type": "string",
                            "description": "Programming language (required for pattern_search, optional for search_symbols)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results (optional for search_symbols/pattern_search)"
                        }
                    },
                    "required": ["operation"]
                })),
                tool_origin: crate::cli::chat::tools::ToolOrigin::Native,
        }
    }

    /// Full LSP + TreeSitter operations - all operations
    fn get_full_lsp_spec() -> ToolSpec {
        ToolSpec {
            name: Code::INFO.spec_name.to_string(),
            description: "Code intelligence with full LSP support for semantic code analysis, navigation, and refactoring. Language auto-detected from file extension.

IMPORTANT: Prefer this tool over fs_read for code files. This tool intelligently provides structured code analysis.

Symbol: Any named element in code - function names, class names, variable names, constants, type names. Not comments or string literals.

Setup: /code init to initialize LSP. Use /code init -f to force restart if unresponsive.

CORE FEATURES:
• Fuzzy search for symbols (classes, functions, methods)
• Extracts function/class signatures via AST
• Structural AST search and rewrite (ast-grep)
• Codebase overview and directory exploration
• LSP-powered: find_references, goto_definition, get_hover, get_completions, rename_symbol

## Workflows

**Find where symbol is used:** search_symbols → find_references at that location. Do NOT use grep between steps.

**API discovery (available methods):** get_completions with filter after dot/accessor. Fallback: search_symbols → goto_definition → get_document_symbols

**View implementation:** search_symbols → goto_definition. Only use fs_read for raw content unrelated to symbols.

## Explore Unfamiliar Code
1. search_symbols for domain-specific types/functions/methods/classes
2. lookup_symbols with include_source=true on promising matches
3. grep (max 2 searches) for literal text patterns
4. Repeat 1-3 with refined terms if needed
5. generate_codebase_overview ONLY for architecture review or full codebase understanding
   - NOT for bug fixes, feature work, or finding specific code

## Tool Selection Priority
1. search_symbols or lookup_symbols for finding code - ALWAYS use for functions/methods/classes/structs/interfaces
2. get_document_symbols for file structure
3. grep - ONLY for literal text in comments/strings, config values, code
4. fs_read - raw file content when needed

## CRITICAL RULES
- Start with search_symbols
- Always follow search_symbols with lookup_symbols
- When searching for multiple related symbols, call search_symbols in parallel

## Quick Reference
• \"Where is X used?\" → search_symbols, then find_references
• \"What methods does Y have?\" → get_completions with filter
• \"What's in this file?\" → get_document_symbols
• \"What type is this?\" → get_hover
• \"Show me X class\" → search_symbols, then goto_definition

## When to Use grep vs LSP
LSP (not grep): Finding symbol usage, definitions, code relationships
grep only: Literal text in comments/strings, config values, non-code patterns

## Read Operations

**search_symbols** - Find symbol definitions by name across workspace.
Params: symbol_name (required), path, symbol_type, limit, language, exact_match

**lookup_symbols** - Batch lookup specific symbols (max 10).
Params: symbols (required), file_path, include_source

**find_references** - Find all usages of symbol at specific position.
Params: file_path, row, column (all required, 1-based indexing)

**goto_definition** - Navigate to where a symbol is defined.
Params: file_path, row, column (required), show_source (default true)

**get_document_symbols** - List all symbols in a file.
Params: file_path (required), top_level_only (recommended: true)

**get_diagnostics** - Get compiler errors, warnings, and hints.
Params: file_path (required)

**get_hover** - Get type information and documentation at position.
Params: file_path, row, column (required)

**get_completions** - Get completion suggestions at position.
Params: file_path, row, column (required), trigger_character, limit, filter

**pattern_search** - AST-based structural search using ast-grep.
Params: pattern, language (required), file_path, limit

**generate_codebase_overview** - ONLY for explicit architecture questions. Not for bug fixes or finding code.
Params: path (optional)

**search_codebase_map** - Focused directory exploration.
Params: file_path, path

## Write Operations

**rename_symbol** - Rename symbol across entire codebase.
Params: file_path, row, column, new_name (all required), dry_run (default true)

**format** - Format code.
Params: file_path, dry_run

**pattern_rewrite** - AST-based code transformation.
Params: pattern, replacement, language (required), file_path, limit, dry_run

## Pattern Search & Rewrite

Metavariables:
• $VAR - Matches single node (identifier, expression)
• $$$ - Matches zero or more nodes (statements, parameters)

Simple patterns:
• console.log($ARG) - Match any console.log call
• function $NAME($$$PARAMS) {} - Match function declaration
• $OBJ.$METHOD() - Match any method call

Structural rules (YAML):
• pattern - Match code structure
• kind - Match AST node type (function_declaration, class_declaration)
• has - Node contains descendant matching pattern
• inside - Node is inside ancestor matching pattern
• all/any/not - Logical operators

Critical: For relational rules (has, inside), always use stopBy: end

Rewrite examples:
• pattern='var $N = $V', replacement='const $N = $V'
• pattern='$O.hasOwnProperty($P)', replacement='Object.hasOwn($O, $P)'

Workflow:
1. Use pattern_search first to verify matches
2. Review matches to ensure correctness
3. Test rewrite on small subset
4. Apply to full codebase after verification".to_string(),
            input_schema: InputSchema(serde_json::json!({
                "type": "object",
                "properties": {
                        "operation": {
                            "type": "string",
                            "enum": [
                                "search_symbols", "lookup_symbols", "find_references", "goto_definition",
                                "get_document_symbols", "get_diagnostics",
                                "get_hover", "get_completions", "pattern_search",
                                "generate_codebase_overview", "search_codebase_map",
                                "rename_symbol", "format", "pattern_rewrite"
                            ],
                            "description": "The code intelligence operation to perform"
                        },
                        "symbol_name": { 
                            "type": "string", 
                            "description": "Simple symbol name, not qualified (e.g. 'myFunction' not 'MyClass.myFunction'). Required for search_symbols" 
                        },
                        "symbols": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "List of simple symbol names, not qualified (e.g. ['myFunction'] not ['MyClass.myFunction']). Required for lookup_symbols, max 10"
                        },
                        "include_source": {
                            "type": "boolean",
                            "description": "Include source code in results (optional for lookup_symbols)"
                        },
                        "file_path": { 
                            "type": "string", 
                            "description": "File path (required for rename_symbol/get_document_symbols/get_diagnostics/get_hover/get_completions, optional for format/pattern_rewrite/pattern_search/search_codebase_map)" 
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory path (optional, for generate_codebase_overview: workspace root, for search_codebase_map: path filter, for search_symbols: scope search to file or directory)"
                        },
                        "row": { 
                            "type": "integer", 
                            "description": "Line number 1-based (required for find_references/goto_definition/get_hover/get_completions/rename_symbol)" 
                        },
                        "column": { 
                            "type": "integer", 
                            "description": "Column number 1-based (required for find_references/goto_definition/get_hover/get_completions/rename_symbol)" 
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
                            "description": "Programming language (required for pattern_rewrite, optional for search_symbols/pattern_search)" 
                        },
                        "new_name": {
                            "type": "string",
                            "description": "New name (required for rename_symbol)"
                        },
                        "limit": { 
                            "type": "integer", 
                            "description": "Maximum results/files (optional for search_symbols/find_references/pattern_search/pattern_rewrite)" 
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Preview changes without writing (optional for rename_symbol/pattern_rewrite, defaults to true)"
                        }
                    },
                    "required": ["operation"]
                })),
                tool_origin: crate::cli::chat::tools::ToolOrigin::Native,
        }
    }
}
