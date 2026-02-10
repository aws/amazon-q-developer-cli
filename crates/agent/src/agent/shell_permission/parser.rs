//! Shell command parsing using tree-sitter to extract commands from chains and detect redirections.

use tree_sitter::Parser;

// Tree-sitter bash grammar node types
mod node {
    // Command structures
    pub const LIST: &str = "list";
    pub const PIPELINE: &str = "pipeline";
    pub const COMMAND: &str = "command";
    pub const SIMPLE_COMMAND: &str = "simple_command";
    pub const SUBSHELL: &str = "subshell";
    pub const REDIRECTED_STATEMENT: &str = "redirected_statement";
    pub const COMPOUND_STATEMENT: &str = "compound_statement";
    pub const PROGRAM: &str = "program";
    pub const COMMAND_NAME: &str = "command_name";

    // Operators
    pub const AND: &str = "&&";
    pub const OR: &str = "||";
    pub const SEMICOLON: &str = ";";
    pub const PIPE: &str = "|";
    pub const NEWLINE: &str = "\n";
    pub const COMMENT: &str = "comment";

    // Redirections
    pub const FILE_REDIRECT: &str = "file_redirect";
    pub const HEREDOC_REDIRECT: &str = "heredoc_redirect";
    pub const HERESTRING_REDIRECT: &str = "herestring_redirect";

    // Other
    pub const HEREDOC_BODY: &str = "heredoc_body";
    pub const VARIABLE_ASSIGNMENT: &str = "variable_assignment";

    // Substitutions
    pub const COMMAND_SUBSTITUTION: &str = "command_substitution";
    pub const PROCESS_SUBSTITUTION: &str = "process_substitution";

    // Variable expansion
    pub const SIMPLE_EXPANSION: &str = "simple_expansion";
    pub const EXPANSION: &str = "expansion";
    pub const ARITHMETIC_EXPANSION: &str = "arithmetic_expansion";
    pub const VARIABLE_EXPANSION_NODES: &[&str] = &[SIMPLE_EXPANSION, EXPANSION, ARITHMETIC_EXPANSION];

    // ANSI-C string (e.g., $'\x41')
    pub const ANSI_C_STRING: &str = "ansi_c_string";

    // Grouped constants
    pub const HEREDOC_NODES: &[&str] = &[HEREDOC_REDIRECT, HEREDOC_BODY, HERESTRING_REDIRECT];

    /// Nodes that are direct children of a command but not arguments
    pub const NON_ARG_NODES: &[&str] = &[HERESTRING_REDIRECT, VARIABLE_ASSIGNMENT];
}
use serde::Deserialize;

/// Operator connecting chained commands.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum ChainOperator {
    /// `&&` - AND operator
    And,
    /// `||` - OR operator
    Or,
    /// `;` - Sequence operator
    Sequence,
    /// `|` - Pipe operator
    Pipe,
}

/// A parsed command from a shell command string.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ParsedCommand {
    /// The full command text.
    pub command: String,
    /// The full command path as written (e.g., "/usr/bin/git", "./script.sh").
    #[serde(default)]
    pub command_path: String,
    /// The base command name without path (e.g., "git", "script.sh").
    #[serde(default)]
    pub command_name: String,
    /// Arguments to the command (excludes command name).
    #[serde(default)]
    pub args: Vec<String>,
    /// Operator following this command (if any).
    pub operator: Option<ChainOperator>,
    /// File redirections (`>`, `>>`, `<`, `2>&1`). Does not include heredocs.
    #[serde(default)]
    pub has_redirection: bool,
    /// Whether this command is inside a subshell `()`.
    #[serde(default)]
    pub is_subshell: bool,
    /// Command substitution (`$()` or backticks).
    #[serde(default)]
    pub has_command_substitution: bool,
    /// Heredocs or herestrings (`<<`, `<<<`).
    #[serde(default)]
    pub has_heredoc: bool,
    /// Process substitution (`<()` or `>()`).
    #[serde(default)]
    pub has_process_substitution: bool,
    /// Variable expansion (`$VAR`, `${VAR}`, `$((...))`).
    #[serde(default)]
    pub has_variable_expansion: bool,
    /// Variable assignment (`VAR=value`, `IFS=...`).
    #[serde(default)]
    pub has_variable_assignment: bool,
    /// ANSI-C string (`$'\x41'`, `$'\n'`).
    #[serde(default)]
    pub has_ansi_c_string: bool,
}

/// Result of parsing a shell command string.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used by tests and future Layer 2/3
pub struct ParseResult {
    /// Individual commands extracted from the input.
    pub commands: Vec<ParsedCommand>,
    /// Whether parsing failed (commands will be empty).
    pub parse_failed: bool,
    /// The raw input string.
    pub raw_input: String,
}

/// Parse a shell command string into individual commands.
pub fn parse_command(input: &str) -> ParseResult {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_bash::LANGUAGE.into()).is_err() {
        return ParseResult {
            commands: vec![],
            parse_failed: true,
            raw_input: input.to_string(),
        };
    }

    let Some(tree) = parser.parse(input, None) else {
        return ParseResult {
            commands: vec![],
            parse_failed: true,
            raw_input: input.to_string(),
        };
    };

    let mut commands = Vec::new();
    extract_commands(&tree.root_node(), input, &mut commands);

    // If no commands extracted from non-empty input, treat as parse failure
    let parse_failed = commands.is_empty() && !input.trim().is_empty();

    ParseResult {
        commands,
        parse_failed,
        raw_input: input.to_string(),
    }
}

fn extract_commands(node: &tree_sitter::Node<'_>, source: &str, commands: &mut Vec<ParsedCommand>) {
    match node.kind() {
        node::LIST => {
            // Handle command lists (cmd1 && cmd2, cmd1 || cmd2, cmd1; cmd2)
            let mut child_cursor = node.walk();
            let children: Vec<_> = node.children(&mut child_cursor).collect();

            let mut i = 0;
            while i < children.len() {
                let child = &children[i];
                let child_kind = child.kind();

                if child_kind == node::AND || child_kind == node::OR || child_kind == node::SEMICOLON {
                    // This is an operator, update the previous command
                    if let Some(last) = commands.last_mut() {
                        last.operator = Some(match child_kind {
                            node::AND => ChainOperator::And,
                            node::OR => ChainOperator::Or,
                            _ => ChainOperator::Sequence,
                        });
                    }
                } else if !matches!(child_kind, node::NEWLINE | node::COMMENT) {
                    extract_commands(child, source, commands);
                }
                i += 1;
            }
        },
        node::PIPELINE => {
            // Handle pipelines (cmd1 | cmd2)
            let mut child_cursor = node.walk();
            let children: Vec<_> = node.children(&mut child_cursor).collect();

            for (i, child) in children.iter().enumerate() {
                if child.kind() == node::PIPE {
                    if let Some(last) = commands.last_mut() {
                        last.operator = Some(ChainOperator::Pipe);
                    }
                } else if !matches!(child.kind(), node::NEWLINE | node::COMMENT) {
                    extract_commands(child, source, commands);
                    // If not the last command and next is pipe, mark it
                    if i + 1 < children.len()
                        && children.get(i + 1).is_some_and(|n| n.kind() == node::PIPE)
                        && let Some(last) = commands.last_mut()
                    {
                        last.operator = Some(ChainOperator::Pipe);
                    }
                }
            }
        },
        node::COMMAND | node::SIMPLE_COMMAND => {
            let cmd_text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            let (command_path, command_name, args) = extract_command_parts(node, source);

            commands.push(ParsedCommand {
                command: cmd_text,
                command_path,
                command_name,
                args,
                operator: None,
                has_redirection: has_descendant(node, &[node::FILE_REDIRECT]),
                is_subshell: false,
                has_command_substitution: has_descendant(node, &[node::COMMAND_SUBSTITUTION]),
                has_heredoc: has_descendant(node, node::HEREDOC_NODES),
                has_process_substitution: has_descendant(node, &[node::PROCESS_SUBSTITUTION]),
                has_variable_expansion: has_descendant(node, node::VARIABLE_EXPANSION_NODES),
                has_variable_assignment: has_descendant(node, &[node::VARIABLE_ASSIGNMENT]),
                has_ansi_c_string: has_descendant(node, &[node::ANSI_C_STRING]),
            });
        },
        node::SUBSHELL => {
            // Extract commands from inside the subshell, marking them as is_subshell
            let start_len = commands.len();
            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                if !matches!(child.kind(), "(" | ")") {
                    extract_commands(&child, source, commands);
                }
            }
            // Mark all extracted commands as being in a subshell
            for cmd in commands.iter_mut().skip(start_len) {
                cmd.is_subshell = true;
            }
        },
        node::REDIRECTED_STATEMENT => {
            // A command/pipeline with redirection - recurse into children and mark last as redirected
            let start_len = commands.len();
            let node_has_heredoc = has_descendant(node, node::HEREDOC_NODES);
            let node_has_process_sub = has_descendant(node, &[node::PROCESS_SUBSTITUTION]);

            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                let kind = child.kind();
                // Recurse into command structures, skip redirect nodes
                if matches!(
                    kind,
                    node::COMMAND | node::SIMPLE_COMMAND | node::PIPELINE | node::LIST | node::SUBSHELL
                ) {
                    extract_commands(&child, source, commands);
                }
            }
            // Mark the last command as having redirection and propagate heredoc/process_sub
            if let Some(last) = commands.last_mut() {
                last.has_redirection = true;
                if node_has_heredoc {
                    last.has_heredoc = true;
                }
                if node_has_process_sub {
                    last.has_process_substitution = true;
                }
            }
            // If no commands extracted, treat whole thing as single command
            if commands.len() == start_len {
                let (command_path, command_name, args) = extract_command_parts(node, source);
                commands.push(ParsedCommand {
                    command: node.utf8_text(source.as_bytes()).unwrap_or("").to_string(),
                    command_path,
                    command_name,
                    args,
                    operator: None,
                    has_redirection: true,
                    is_subshell: false,
                    has_command_substitution: has_descendant(node, &[node::COMMAND_SUBSTITUTION]),
                    has_heredoc: node_has_heredoc,
                    has_process_substitution: node_has_process_sub,
                    has_variable_expansion: has_descendant(node, node::VARIABLE_EXPANSION_NODES),
                    has_variable_assignment: has_descendant(node, &[node::VARIABLE_ASSIGNMENT]),
                    has_ansi_c_string: has_descendant(node, &[node::ANSI_C_STRING]),
                });
            }
        },
        node::COMPOUND_STATEMENT | node::PROGRAM => {
            // Handle operators at program/compound level (e.g., cmd1; cmd2)
            let mut child_cursor = node.walk();
            let children: Vec<_> = node.children(&mut child_cursor).collect();

            for child in &children {
                let child_kind = child.kind();
                if child_kind == node::SEMICOLON {
                    // Mark previous command with Sequence operator
                    if let Some(last) = commands.last_mut()
                        && last.operator.is_none()
                    {
                        last.operator = Some(ChainOperator::Sequence);
                    }
                } else if !matches!(child_kind, node::NEWLINE | node::COMMENT) {
                    extract_commands(child, source, commands);
                }
            }
        },
        _ => {
            // For other node types, recurse into children
            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                extract_commands(&child, source, commands);
            }
        },
    }
}

/// Extract command path, name, and arguments from a command node.
fn extract_command_parts(node: &tree_sitter::Node<'_>, source: &str) -> (String, String, Vec<String>) {
    let mut command_path = String::new();
    let mut args = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        let text = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();

        if kind == node::COMMAND_NAME {
            command_path = strip_quotes(&text);
        } else if !node::NON_ARG_NODES.contains(&kind) {
            args.push(text);
        }
    }

    // Extract filename from path: /usr/bin/git -> git
    let command_name = std::path::Path::new(&command_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&command_path)
        .to_string();

    (command_path, command_name, args)
}

/// Strip surrounding quotes from a command
fn strip_quotes(s: &str) -> String {
    s.trim()
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| s.trim().strip_prefix('"').and_then(|s| s.strip_suffix('"')))
        .unwrap_or(s.trim())
        .to_string()
}

fn has_descendant(node: &tree_sitter::Node<'_>, kinds: &[&str]) -> bool {
    if kinds.contains(&node.kind()) {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| has_descendant(&c, kinds))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct TestCase {
        name: String,
        input: String,
        parse_failed: bool,
        commands: Vec<ParsedCommand>,
    }

    fn load_test_cases() -> Vec<TestCase> {
        let json = include_str!("test_data/parser_tests.json");
        serde_json::from_str(json).expect("Failed to parse parser_tests.json")
    }

    #[test]
    fn test_parser_cases() {
        let cases = load_test_cases();
        let total = cases.len();
        for tc in cases {
            let result = parse_command(&tc.input);

            assert_eq!(
                result.parse_failed, tc.parse_failed,
                "[{}] parse_failed mismatch",
                tc.name
            );
            assert_eq!(
                result.commands, tc.commands,
                "[{}] commands mismatch for input: {:?}",
                tc.name, tc.input
            );
        }
        println!("parser_tests.json: {total} test cases passed");
    }

    /// Debug helper to visualize tree-sitter AST for a command.
    /// To use: uncomment the #[test] attribute and change the input string.
    #[test]
    #[ignore]
    #[allow(dead_code)]
    fn debug_tree_structure() {
        fn print_tree(input: &str) {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_bash::LANGUAGE.into()).unwrap();
            let tree = parser.parse(input, None).unwrap();
            fn print_node(node: tree_sitter::Node<'_>, source: &str, indent: usize) {
                let prefix = "  ".repeat(indent);
                println!(
                    "{}[{}] {:?}",
                    prefix,
                    node.kind(),
                    node.utf8_text(source.as_bytes()).unwrap_or("")
                );
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    print_node(child, source, indent + 1);
                }
            }
            println!("Tree for {:?}:", input);
            print_node(tree.root_node(), input, 0);
        }

        // Test backtick command substitution
        print_tree("echo `whoami`");
    }
}
