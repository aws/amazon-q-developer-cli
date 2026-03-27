use std::collections::BTreeSet;

use super::super::mcp::types::PromptArgument;
use super::PROMPT_NAME_REGEX;

/// Maximum number of positional arguments supported (${1} through ${10}).
const MAX_ARGUMENT_POSITION: u8 = 10;

/// Parsed argument placeholder metadata from a prompt template.
///
/// - `${1}` through `${10}` — positional arguments
/// - `$ARGUMENTS` / `${@}` — all arguments joined by spaces
pub struct PromptTemplateArgs {
    positional: BTreeSet<u8>,
    has_all_args: bool,
}

impl PromptTemplateArgs {
    /// Parse a prompt template and extract placeholder metadata.
    pub fn parse(content: &str) -> Self {
        let mut positional = BTreeSet::new();
        let has_all_args = content.contains("$ARGUMENTS") || content.contains("${@}");

        for (i, _) in content.match_indices("${") {
            let rest = &content[i + 2..];
            let Some(end) = rest.find('}') else { continue };
            let inner = &rest[..end];

            if let Ok(n) = inner.parse::<u8>()
                && (1..=MAX_ARGUMENT_POSITION).contains(&n)
                && !inner.starts_with('0')
            {
                positional.insert(n);
            }
        }

        Self {
            positional,
            has_all_args,
        }
    }

    /// The set of positional placeholder positions.
    pub fn positional(&self) -> &BTreeSet<u8> {
        &self.positional
    }

    /// Whether the template contains `$ARGUMENTS` or `${@}`.
    pub fn has_all_args(&self) -> bool {
        self.has_all_args
    }

    /// Expand all placeholders in `content` with the given arguments.
    pub fn expand(&self, content: &str, arguments: &[String]) -> String {
        let mut result = if self.has_all_args {
            let args_joined = arguments.join(" ");
            content
                .replace("$ARGUMENTS", &args_joined)
                .replace("${@}", &args_joined)
        } else {
            content.to_string()
        };

        for &pos in &self.positional {
            let placeholder = format!("${{{pos}}}");
            let replacement = arguments.get((pos - 1) as usize).map_or("", String::as_str);
            result = result.replace(&placeholder, replacement);
        }

        result
    }

    /// Build `PromptArgument` metadata from parsed template for use in prompt discovery.
    pub fn to_prompt_arguments(&self) -> Option<Vec<PromptArgument>> {
        if self.positional.is_empty() && !self.has_all_args {
            return None;
        }

        let mut args: Vec<PromptArgument> = self
            .positional
            .iter()
            .map(|&pos| PromptArgument {
                name: format!("arg{pos}"),
                description: None,
                required: Some(true),
            })
            .collect();

        if self.has_all_args && self.positional.is_empty() {
            args.push(PromptArgument {
                name: "args".to_string(),
                description: Some("All arguments".to_string()),
                required: Some(false),
            });
        }

        Some(args)
    }
}

/// Resolve a file-based prompt by name, checking local then global directories.
/// Returns the file content if found.
///
/// Rejects names that don't match the prompt naming rules to prevent path traversal.
pub fn resolve_file_prompt(cwd: &std::path::Path, name: &str) -> Option<String> {
    if !PROMPT_NAME_REGEX.is_match(name) {
        return None;
    }

    let local_path = cwd.join(".kiro").join("prompts").join(format!("{name}.md"));
    if let Ok(content) = std::fs::read_to_string(&local_path) {
        return Some(content);
    }

    let global_path = dirs::home_dir()?
        .join(".kiro")
        .join("prompts")
        .join(format!("{name}.md"));
    std::fs::read_to_string(global_path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_positional() {
        let t = PromptTemplateArgs::parse("Hello ${1}");
        assert_eq!(*t.positional(), BTreeSet::from([1u8]));

        let t = PromptTemplateArgs::parse("${3} ${1} ${2}");
        assert_eq!(*t.positional(), BTreeSet::from([1, 2, 3]));

        let t = PromptTemplateArgs::parse("${1} ${1} ${2}");
        assert_eq!(*t.positional(), BTreeSet::from([1, 2]));

        let t = PromptTemplateArgs::parse("${10}");
        assert_eq!(*t.positional(), BTreeSet::from([10]));

        let t = PromptTemplateArgs::parse("No placeholders");
        assert!(t.positional().is_empty());
    }

    #[test]
    fn test_parse_all_args() {
        let t = PromptTemplateArgs::parse("Hello $ARGUMENTS");
        assert!(t.has_all_args());
        assert!(t.positional().is_empty());

        let t = PromptTemplateArgs::parse("Hello ${@}");
        assert!(t.has_all_args());

        let t = PromptTemplateArgs::parse("${1} and $ARGUMENTS");
        assert!(t.has_all_args());
        assert_eq!(*t.positional(), BTreeSet::from([1]));
    }

    #[test]
    fn test_parse_ignores_invalid() {
        let t = PromptTemplateArgs::parse("${0}");
        assert!(t.positional().is_empty());

        let t = PromptTemplateArgs::parse("${11}");
        assert!(t.positional().is_empty());

        let t = PromptTemplateArgs::parse("${01}");
        assert!(t.positional().is_empty());

        let t = PromptTemplateArgs::parse("${ 1 }");
        assert!(t.positional().is_empty());

        let t = PromptTemplateArgs::parse("${a}");
        assert!(t.positional().is_empty());
    }

    #[test]
    fn test_expand_positional() {
        let content = "Hello ${1}";
        let t = PromptTemplateArgs::parse(content);
        assert_eq!(t.expand(content, &["World".into()]), "Hello World");

        let content = "${3} ${1} ${2}";
        let t = PromptTemplateArgs::parse(content);
        assert_eq!(t.expand(content, &["A".into(), "B".into(), "C".into()]), "C A B");

        let content = "${1} and ${1}";
        let t = PromptTemplateArgs::parse(content);
        assert_eq!(t.expand(content, &["test".into()]), "test and test");

        let content = "${1} ${2} ${3}";
        let t = PromptTemplateArgs::parse(content);
        assert_eq!(t.expand(content, &["only".into()]), "only  ");
    }

    #[test]
    fn test_expand_all_args() {
        let content = "Hello $ARGUMENTS";
        let t = PromptTemplateArgs::parse(content);
        assert_eq!(t.expand(content, &["world".into(), "test".into()]), "Hello world test");

        let content = "Hello ${@}";
        let t = PromptTemplateArgs::parse(content);
        assert_eq!(t.expand(content, &["world".into(), "test".into()]), "Hello world test");

        let content = "${1}: $ARGUMENTS";
        let t = PromptTemplateArgs::parse(content);
        assert_eq!(
            t.expand(content, &["Command".into(), "arg1".into(), "arg2".into()]),
            "Command: Command arg1 arg2"
        );

        let content = "Command: $ARGUMENTS";
        let t = PromptTemplateArgs::parse(content);
        assert_eq!(t.expand(content, &[]), "Command: ");
    }

    #[test]
    fn test_to_prompt_arguments() {
        let t = PromptTemplateArgs::parse("No placeholders");
        assert!(t.to_prompt_arguments().is_none());

        let t = PromptTemplateArgs::parse("${1} ${2}");
        let args = t.to_prompt_arguments().unwrap();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].name, "arg1");
        assert_eq!(args[1].name, "arg2");

        let t = PromptTemplateArgs::parse("$ARGUMENTS");
        let args = t.to_prompt_arguments().unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].name, "args");
        assert_eq!(args[0].required, Some(false));
    }

    #[test]
    fn test_resolve_rejects_path_traversal() {
        let cwd = std::path::Path::new("/tmp/fake");
        assert!(resolve_file_prompt(cwd, "../../etc/passwd").is_none());
        assert!(resolve_file_prompt(cwd, "path/name").is_none());
        assert!(resolve_file_prompt(cwd, "path\\name").is_none());
        assert!(resolve_file_prompt(cwd, "has space").is_none());
        assert!(resolve_file_prompt(cwd, "special!char").is_none());
        assert!(resolve_file_prompt(cwd, "").is_none());
    }

    #[test]
    fn test_resolve_accepts_valid_names() {
        let cwd = std::path::Path::new("/tmp/fake");
        // These won't find files but should not be rejected by validation
        assert!(super::PROMPT_NAME_REGEX.is_match("valid-name"));
        assert!(super::PROMPT_NAME_REGEX.is_match("valid_name_v2"));
        assert!(super::PROMPT_NAME_REGEX.is_match("CamelCase"));
        assert!(super::PROMPT_NAME_REGEX.is_match("123"));
        // resolve returns None because files don't exist, but it passes validation
        assert!(resolve_file_prompt(cwd, "valid-name").is_none());
    }
}
