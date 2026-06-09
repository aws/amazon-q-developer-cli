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

    // Declaration commands (export, declare, local, readonly, typeset)
    pub const DECLARATION_COMMAND: &str = "declaration_command";

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
    /// Whether the command writes to or reads from a file via redirection.
    /// Set for `>`, `>>`, `<`, `&>`, `2> file`, and heredocs/herestrings.
    /// NOT set for fd-to-fd duplicates like `2>&1` and `1>&2`.
    #[serde(default)]
    pub has_redirection_to_file: bool,
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
    /// Variable assignment texts (`VAR=value` → `["VAR=value"]`, `IFS=: cmd` → `["IFS=:"]`).
    #[serde(default)]
    pub variable_assignments: Vec<String>,
    /// ANSI-C string (`$'\x41'`, `$'\n'`).
    #[serde(default)]
    pub has_ansi_c_string: bool,
    /// Output redirection target paths (from `>` and `>>`). Does not include fd-to-fd redirects
    /// like `2>&1`.
    #[serde(default)]
    pub redirect_targets: Vec<String>,
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
        node::COMMAND | node::SIMPLE_COMMAND | node::DECLARATION_COMMAND => {
            let cmd_text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            let (command_path, command_name, args) = if node.kind() == node::DECLARATION_COMMAND {
                extract_declaration_parts(node, source)
            } else {
                extract_command_parts(node, source)
            };

            commands.push(ParsedCommand {
                command: cmd_text,
                command_path,
                command_name,
                args,
                operator: None,
                has_redirection_to_file: has_file_targeting_redirect(node),
                is_subshell: false,
                has_command_substitution: has_descendant(node, &[node::COMMAND_SUBSTITUTION]),
                has_heredoc: has_descendant(node, node::HEREDOC_NODES),
                has_process_substitution: has_descendant(node, &[node::PROCESS_SUBSTITUTION]),
                has_variable_expansion: has_descendant(node, node::VARIABLE_EXPANSION_NODES),
                variable_assignments: collect_matching_text(node, source, node::VARIABLE_ASSIGNMENT),
                has_ansi_c_string: has_descendant(node, &[node::ANSI_C_STRING]),
                redirect_targets: collect_redirect_targets(node, source),
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
            // Mark the last command as redirecting to a file and propagate heredoc/process_sub.
            if let Some(last) = commands.last_mut() {
                if has_file_targeting_redirect(node) || node_has_heredoc {
                    last.has_redirection_to_file = true;
                }
                last.redirect_targets = collect_redirect_targets(node, source);
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
                    has_redirection_to_file: has_file_targeting_redirect(node) || node_has_heredoc,
                    is_subshell: false,
                    has_command_substitution: has_descendant(node, &[node::COMMAND_SUBSTITUTION]),
                    has_heredoc: node_has_heredoc,
                    has_process_substitution: node_has_process_sub,
                    has_variable_expansion: has_descendant(node, node::VARIABLE_EXPANSION_NODES),
                    variable_assignments: collect_matching_text(node, source, node::VARIABLE_ASSIGNMENT),
                    has_ansi_c_string: has_descendant(node, &[node::ANSI_C_STRING]),
                    redirect_targets: collect_redirect_targets(node, source),
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

/// Extract parts from a declaration_command node (export, declare, local, readonly, typeset).
/// Unlike regular commands, the keyword is a bare token — not wrapped in a command_name node.
fn extract_declaration_parts(node: &tree_sitter::Node<'_>, source: &str) -> (String, String, Vec<String>) {
    let mut keyword = String::new();
    let mut args = Vec::new();
    let mut first = true;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let text = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        if first {
            keyword = text;
            first = false;
        } else if !node::NON_ARG_NODES.contains(&child.kind()) {
            args.push(text);
        }
    }

    (keyword.clone(), keyword, args)
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

/// Whether `node` (or any descendant) contains a `file_redirect` that targets a file.
///
/// fd-to-fd duplicates like `2>&1` / `1>&2` and fd closes like `2>&-` are excluded -
/// they shuffle or close existing file descriptors and produce no new file side
/// effects. A `file_redirect` whose only non-operator children are `file_descriptor`
/// (source fd) and `number` (target fd) is treated as fd-to-fd. Anything else
/// (`word`, `string`, expansions, substitutions, etc.) is a real file target.
fn has_file_targeting_redirect(node: &tree_sitter::Node<'_>) -> bool {
    if node.kind() == node::FILE_REDIRECT {
        return file_redirect_targets_file(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| has_file_targeting_redirect(&c))
}

/// Whether a `file_redirect` node has a file target (vs being a pure fd-to-fd dup
/// or fd close).
fn file_redirect_targets_file(node: &tree_sitter::Node<'_>) -> bool {
    debug_assert_eq!(node.kind(), node::FILE_REDIRECT);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // Source/target file descriptors in fd-to-fd dups (e.g. `2` and `1` in `2>&1`)
            // and redirect operators - never a file target. `>&-` / `<&-` are fd-close
            // operators, also fd-only.
            "file_descriptor" | "number" | ">" | ">>" | "<" | ">&" | "<&" | "&>" | "&>>" | ">&-" | "<&-" => {},
            // Anything else (`word`, `string`, `simple_expansion`, `expansion`,
            // `command_substitution`, `process_substitution`, `concatenation`,
            // `raw_string`, `ansi_c_string`, ...) is a real file target.
            _ => return true,
        }
    }
    false
}

/// Collect text of all descendant nodes matching `kind`.
fn collect_matching_text(node: &tree_sitter::Node<'_>, source: &str, kind: &str) -> Vec<String> {
    let mut results = Vec::new();
    collect_matching_text_inner(node, source, kind, &mut results);
    results
}

fn collect_matching_text_inner(node: &tree_sitter::Node<'_>, source: &str, kind: &str, results: &mut Vec<String>) {
    if node.kind() == kind {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            results.push(text.to_string());
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_matching_text_inner(&child, source, kind, results);
    }
}

/// Collect output redirection target paths from `file_redirect` nodes.
/// Only captures `>` and `>>` targets (not fd-to-fd like `2>&1`).
fn collect_redirect_targets(node: &tree_sitter::Node<'_>, source: &str) -> Vec<String> {
    let mut targets = Vec::new();
    collect_redirect_targets_inner(node, source, &mut targets);
    targets
}

/// Intentionally only extracts bare `word` targets (e.g., `> file.txt`). Quoted strings,
/// variable expansions, command substitutions, and other complex targets are not extracted,
/// causing the command to fail closed (stay dangerous) since redirect_targets will be empty.
fn collect_redirect_targets_inner(node: &tree_sitter::Node<'_>, source: &str, targets: &mut Vec<String>) {
    if node.kind() == node::FILE_REDIRECT {
        let mut has_output_op = false;
        let mut target_path = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                ">" | ">>" => has_output_op = true,
                "word" => {
                    target_path = child.utf8_text(source.as_bytes()).ok().map(|s| s.to_string());
                },
                _ => {},
            }
        }
        if has_output_op && let Some(path) = target_path {
            targets.push(path);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_redirect_targets_inner(&child, source, targets);
    }
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

    #[test]
    fn test_empty_string() {
        let r = parse_command("");
        assert!(!r.parse_failed);
        assert!(r.commands.is_empty());
    }

    #[test]
    fn test_whitespace_only() {
        let r = parse_command("   \t  ");
        assert!(!r.parse_failed);
        assert!(r.commands.is_empty());
    }

    #[test]
    fn test_comment_only() {
        // Comments-only input: non-empty but no commands → parse_failed
        let r = parse_command("# this is a comment");
        assert!(r.parse_failed);
        assert!(r.commands.is_empty());
    }

    #[test]
    fn test_multiple_comments() {
        let r = parse_command("# comment1\n# comment2");
        assert!(r.parse_failed);
        assert!(r.commands.is_empty());
    }

    #[test]
    fn test_heredoc_multiline() {
        let r = parse_command("cat <<EOF\nline1\nline2\nEOF");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
        assert!(r.commands[0].has_heredoc);
        assert!(r.commands[0].has_redirection_to_file);
        assert_eq!(r.commands[0].command_name, "cat");
    }

    #[test]
    fn test_heredoc_with_pipe() {
        let r = parse_command("cat <<EOF | grep hello\nline1\nEOF");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.has_heredoc));
    }

    #[test]
    fn test_herestring() {
        let r = parse_command("cat <<< \"hello\"");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
        assert!(r.commands[0].has_heredoc);
    }

    #[test]
    fn test_command_substitution_dollar() {
        let r = parse_command("echo $(uname -r)");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
        assert!(r.commands[0].has_command_substitution);
    }

    #[test]
    fn test_command_substitution_backtick() {
        let r = parse_command("echo `uname -r`");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
        assert!(r.commands[0].has_command_substitution);
    }

    #[test]
    fn test_nested_command_substitution() {
        let r = parse_command("echo $(cat $(find . -name '*.txt'))");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_command_substitution);
    }

    #[test]
    fn test_single_quoted_string() {
        let r = parse_command("echo 'hello world'");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].args, vec!["'hello world'"]);
    }

    #[test]
    fn test_double_quoted_string_with_var() {
        let r = parse_command("echo \"hello $USER\"");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_variable_expansion);
    }

    #[test]
    fn test_escape_sequences() {
        let r = parse_command("echo \"hello\\nworld\"");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
    }

    #[test]
    fn test_ansi_c_string() {
        let r = parse_command("echo $'\\x41\\n'");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
        assert!(r.commands[0].has_ansi_c_string);
    }

    #[test]
    fn test_glob_expansion() {
        let r = parse_command("ls *.rs");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_name, "ls");
        assert_eq!(r.commands[0].args, vec!["*.rs"]);
    }

    #[test]
    fn test_glob_recursive() {
        let r = parse_command("find . -name '*.txt'");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].args, vec![".", "-name", "'*.txt'"]);
    }

    #[test]
    fn test_env_var_simple() {
        let r = parse_command("echo $HOME");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_variable_expansion);
    }

    #[test]
    fn test_env_var_braces() {
        let r = parse_command("echo ${PATH}");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_variable_expansion);
    }

    #[test]
    fn test_arithmetic_expansion() {
        let r = parse_command("echo $((1 + 2))");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_variable_expansion);
    }

    #[test]
    fn test_subshell_simple() {
        let r = parse_command("(ls)");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
        assert!(r.commands[0].is_subshell);
    }

    #[test]
    fn test_subshell_chain() {
        let r = parse_command("(cmd1 && cmd2)");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 2);
        assert!(r.commands[0].is_subshell);
        assert!(r.commands[1].is_subshell);
        assert_eq!(r.commands[0].operator, Some(ChainOperator::And));
    }

    #[test]
    fn test_subshell_with_pipe() {
        let r = parse_command("(cat file | grep foo)");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().all(|c| c.is_subshell));
        assert_eq!(r.commands[0].operator, Some(ChainOperator::Pipe));
    }

    #[test]
    fn test_chain_and_or_sequence() {
        let r = parse_command("a && b || c; d");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 4);
        assert_eq!(r.commands[0].operator, Some(ChainOperator::And));
        assert_eq!(r.commands[1].operator, Some(ChainOperator::Or));
        assert_eq!(r.commands[2].operator, Some(ChainOperator::Sequence));
        assert_eq!(r.commands[3].operator, None);
    }

    #[test]
    fn test_process_substitution() {
        let r = parse_command("diff <(ls a) <(ls b)");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_process_substitution);
    }

    #[test]
    fn test_process_substitution_output() {
        let r = parse_command("tee >(grep err > errors.log)");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_process_substitution);
    }

    #[test]
    fn test_variable_assignment_standalone() {
        // Standalone assignment without a command - tree-sitter parses it as variable_assignment
        // at program level, which the parser doesn't extract as a command
        let r = parse_command("FOO=bar");
        assert!(r.parse_failed);
        assert!(r.commands.is_empty());
    }

    #[test]
    fn test_variable_assignment_with_command() {
        let r = parse_command("VAR=value cmd arg");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_name, "cmd");
        assert_eq!(r.commands[0].variable_assignments, vec!["VAR=value"]);
    }

    #[test]
    fn test_multiple_variable_assignments() {
        let r = parse_command("A=1 B=2 cmd");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].variable_assignments, vec!["A=1", "B=2"]);
    }

    #[test]
    fn test_export_declaration() {
        let r = parse_command("export PATH=/usr/bin");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_name, "export");
        assert!(
            r.commands[0]
                .variable_assignments
                .contains(&"PATH=/usr/bin".to_string())
        );
    }

    #[test]
    fn test_declare_command() {
        let r = parse_command("declare -i NUM=42");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_name, "declare");
    }

    #[test]
    fn test_local_declaration() {
        let r = parse_command("local x=5");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_name, "local");
    }

    #[test]
    fn test_redirect_multiple_targets() {
        let r = parse_command("cmd > out.txt 2> err.txt");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_redirection_to_file);
        assert!(r.commands[0].redirect_targets.contains(&"out.txt".to_string()));
        assert!(r.commands[0].redirect_targets.contains(&"err.txt".to_string()));
    }

    #[test]
    fn test_redirect_append() {
        let r = parse_command("echo hi >> log.txt");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_redirection_to_file);
        assert_eq!(r.commands[0].redirect_targets, vec!["log.txt"]);
    }

    #[test]
    fn test_redirect_with_fd() {
        let r = parse_command("cmd 2>&1");
        assert!(!r.parse_failed);
        // fd-to-fd duplicates (2>&1, 1>&2) are NOT file redirections.
        assert!(!r.commands[0].has_redirection_to_file);
        // fd-to-fd redirects don't produce targets
        assert!(r.commands[0].redirect_targets.is_empty());
    }

    #[test]
    fn test_redirect_quoted_target_not_extracted() {
        // Quoted redirect targets are intentionally not extracted (fail closed)
        let r = parse_command("echo hi > \"$FILE\"");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_redirection_to_file);
        // Variable in target means it won't be a bare "word" node
        assert!(r.commands[0].redirect_targets.is_empty());
    }

    #[test]
    fn test_pipe_with_chain() {
        let r = parse_command("cat f | grep x && echo ok");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 3);
        assert_eq!(r.commands[0].operator, Some(ChainOperator::Pipe));
        assert_eq!(r.commands[1].operator, Some(ChainOperator::And));
    }

    #[test]
    fn test_command_with_path() {
        let r = parse_command("/usr/bin/env python3");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_path, "/usr/bin/env");
        assert_eq!(r.commands[0].command_name, "env");
    }

    #[test]
    fn test_relative_path_command() {
        let r = parse_command("./script.sh arg1");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_path, "./script.sh");
        assert_eq!(r.commands[0].command_name, "script.sh");
    }

    #[test]
    fn test_double_quoted_command_name() {
        let r = parse_command("\"my cmd\" arg");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_path, "my cmd");
        assert_eq!(r.commands[0].command_name, "my cmd");
    }

    #[test]
    fn test_single_quoted_command_name() {
        let r = parse_command("'my cmd' arg");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_path, "my cmd");
    }

    #[test]
    fn test_newline_separated_commands() {
        let r = parse_command("cmd1\ncmd2\ncmd3");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 3);
    }

    #[test]
    fn test_semicolon_at_end() {
        let r = parse_command("cmd1;");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
        assert_eq!(r.commands[0].operator, Some(ChainOperator::Sequence));
    }

    #[test]
    fn test_complex_pipeline_with_redirect() {
        let r = parse_command("find . -name '*.log' | xargs grep ERROR > results.txt");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 2);
        assert_eq!(r.commands[0].operator, Some(ChainOperator::Pipe));
        assert!(r.commands[1].has_redirection_to_file);
    }

    #[test]
    fn test_heredoc_in_pipeline() {
        let r = parse_command("cat <<EOF | sort\nbanana\napple\nEOF");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.has_heredoc));
    }

    #[test]
    fn test_process_sub_in_redirected_statement() {
        let r = parse_command("sort <(cat file) > sorted.txt");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_process_substitution);
        assert!(r.commands[0].has_redirection_to_file);
    }

    #[test]
    fn test_compound_statement_semicolons() {
        let r = parse_command("echo a; echo b; echo c");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 3);
        assert_eq!(r.commands[0].operator, Some(ChainOperator::Sequence));
        assert_eq!(r.commands[1].operator, Some(ChainOperator::Sequence));
    }

    #[test]
    fn test_raw_input_preserved() {
        let input = "echo hello";
        let r = parse_command(input);
        assert_eq!(r.raw_input, input);
    }

    #[test]
    fn test_subshell_nested() {
        let r = parse_command("(echo a; (echo b))");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().all(|c| c.is_subshell));
    }

    #[test]
    fn test_command_with_comment_after() {
        let r = parse_command("ls -la # list all");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.len(), 1);
        assert_eq!(r.commands[0].command_name, "ls");
    }

    #[test]
    fn test_heredoc_redirect_on_redirected_statement() {
        let r = parse_command("grep pattern <<EOF > out.txt\nhello pattern\nEOF");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_heredoc);
        assert!(r.commands[0].has_redirection_to_file);
    }

    #[test]
    fn test_readonly_declaration() {
        let r = parse_command("readonly X=10");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_name, "readonly");
    }

    #[test]
    fn test_typeset_declaration() {
        let r = parse_command("typeset -i N=5");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_name, "typeset");
    }

    #[test]
    fn test_variable_expansion_in_double_quotes() {
        let r = parse_command("echo \"${HOME}/bin\"");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_variable_expansion);
    }

    #[test]
    fn test_arithmetic_in_command() {
        let r = parse_command("echo $((2 * 3 + 1))");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_variable_expansion);
    }

    #[test]
    fn test_background_job() {
        let r = parse_command("sleep 100 &");
        assert!(!r.parse_failed);
        assert_eq!(r.commands[0].command_name, "sleep");
    }

    #[test]
    fn test_redirected_subshell() {
        let r = parse_command("(echo a; echo b) > combined.txt");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.is_subshell && c.has_redirection_to_file));
    }

    #[test]
    fn test_redirected_declaration() {
        // declaration_command with redirection - tests redirected_statement with declaration
        let r = parse_command("export FOO=bar > /dev/null");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_redirection_to_file);
    }

    #[test]
    fn test_for_loop_commands() {
        // for loop - tests the wildcard `_` branch in extract_commands
        let r = parse_command("for i in 1 2 3; do echo $i; done");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.command_name == "echo"));
    }

    #[test]
    fn test_while_loop() {
        let r = parse_command("while true; do echo loop; done");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.command_name == "echo"));
    }

    #[test]
    fn test_if_statement() {
        let r = parse_command("if true; then echo yes; fi");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.command_name == "echo"));
    }

    #[test]
    fn test_case_statement() {
        let r = parse_command("case $x in a) echo a;; esac");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.command_name == "echo"));
    }

    #[test]
    fn test_function_definition() {
        let r = parse_command("foo() { echo hello; }");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.command_name == "echo"));
    }

    #[test]
    fn test_command_group_braces() {
        // { cmd; } - compound_statement / group command
        let r = parse_command("{ echo a; echo b; }");
        assert!(!r.parse_failed);
        assert_eq!(r.commands.iter().filter(|c| c.command_name == "echo").count(), 2);
    }

    #[test]
    fn test_redirected_list() {
        // A list inside a redirected_statement
        let r = parse_command("{ cmd1 && cmd2; } > out.txt");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.has_redirection_to_file));
    }

    #[test]
    fn test_heredoc_with_command_substitution() {
        let r = parse_command("cat <<EOF\n$(whoami)\nEOF");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_heredoc);
    }

    #[test]
    fn test_pipe_to_redirected() {
        let r = parse_command("echo hi | tee file.txt > /dev/null");
        assert!(!r.parse_failed);
        assert!(r.commands.iter().any(|c| c.has_redirection_to_file));
    }

    #[test]
    fn test_ansi_c_string_in_args() {
        let r = parse_command("printf $'hello\\tworld\\n'");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_ansi_c_string);
    }

    #[test]
    fn test_command_substitution_in_variable() {
        let r = parse_command("DIR=$(pwd) ls");
        assert!(!r.parse_failed);
        assert!(r.commands[0].has_command_substitution);
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
