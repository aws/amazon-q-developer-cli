use std::collections::HashMap;
use std::path::Path;

use super::super::mcp::types::Prompt;
use super::super::util::steering::extract_yaml_frontmatter;
use super::PROMPT_NAME_REGEX;
use super::template_args::PromptTemplateArgs;

/// Strip YAML frontmatter from markdown content, returning the body after the closing `---`.
fn strip_frontmatter(content: &str) -> String {
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return content.to_string();
    }
    let lines: Vec<&str> = content.lines().collect();
    if let Some((end, _)) = lines.iter().enumerate().skip(1).find(|(_, l)| l.trim() == "---") {
        // Skip the closing --- line and any leading whitespace after it
        lines[end + 1..].join("\n").trim_start_matches('\n').to_string()
    } else {
        content.to_string()
    }
}

/// Parse `name` and `description` from YAML frontmatter.
fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>) {
    let Some(yaml) = extract_yaml_frontmatter(content) else {
        return (None, None);
    };
    let mut name = None;
    let mut description = None;
    for line in yaml.lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim().to_string());
        }
    }
    (name, description)
}

/// Discover skills from `.kiro/skills/*/SKILL.md` directories.
/// Returns a `HashMap<String, Vec<Prompt>>` keyed by source (`"skill:local"` / `"skill:global"`).
pub fn discover(cwd: &Path) -> HashMap<String, Vec<Prompt>> {
    let mut result = HashMap::new();
    let mut seen = std::collections::HashSet::new();

    // Local skills (cwd/.kiro/skills/*/SKILL.md)
    let local_dir = cwd.join(".kiro").join("skills");
    if let Ok(entries) = std::fs::read_dir(&local_dir) {
        let mut local_skills = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_file = path.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }
            let dir_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let Ok(content) = std::fs::read_to_string(&skill_file) else {
                continue;
            };
            let (fm_name, description) = parse_frontmatter(&content);
            let name = fm_name.unwrap_or_else(|| dir_name.clone());
            seen.insert(name.clone());

            let arguments = PromptTemplateArgs::parse(&strip_frontmatter(&content)).to_prompt_arguments();
            local_skills.push(Prompt {
                name,
                description,
                arguments,
            });
        }
        if !local_skills.is_empty() {
            result.insert("skill:local".to_string(), local_skills);
        }
    }

    // Global skills (~/.kiro/skills/*/SKILL.md) — skip if already in local
    if let Some(home) = dirs::home_dir() {
        let global_dir = home.join(".kiro").join("skills");
        if let Ok(entries) = std::fs::read_dir(&global_dir) {
            let mut global_skills = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let skill_file = path.join("SKILL.md");
                if !skill_file.exists() {
                    continue;
                }
                let dir_name = match path.file_name().and_then(|s| s.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                let Ok(content) = std::fs::read_to_string(&skill_file) else {
                    continue;
                };
                let (fm_name, description) = parse_frontmatter(&content);
                let name = fm_name.unwrap_or_else(|| dir_name.clone());
                if seen.contains(&name) {
                    continue;
                }

                let arguments = PromptTemplateArgs::parse(&strip_frontmatter(&content)).to_prompt_arguments();
                global_skills.push(Prompt {
                    name,
                    description,
                    arguments,
                });
            }
            if !global_skills.is_empty() {
                result.insert("skill:global".to_string(), global_skills);
            }
        }
    }

    result
}

/// Resolve a skill by name. Checks local then global `.kiro/skills/{name}/SKILL.md`.
/// Returns the file content with YAML frontmatter stripped.
///
/// Rejects names that don't match the prompt naming rules to prevent path traversal.
pub fn resolve_skill(cwd: &Path, name: &str) -> Option<String> {
    if !PROMPT_NAME_REGEX.is_match(name) {
        return None;
    }

    let local_path = cwd.join(".kiro").join("skills").join(name).join("SKILL.md");
    if let Ok(content) = std::fs::read_to_string(&local_path) {
        return Some(strip_frontmatter(&content));
    }

    let global_path = dirs::home_dir()?
        .join(".kiro")
        .join("skills")
        .join(name)
        .join("SKILL.md");
    std::fs::read_to_string(global_path).ok().map(|c| strip_frontmatter(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_frontmatter() {
        let content = "---\nname: test\ndescription: A test\n---\n# Hello\nBody here";
        assert_eq!(strip_frontmatter(content), "# Hello\nBody here");
    }

    #[test]
    fn test_strip_frontmatter_no_frontmatter() {
        let content = "# Hello\nBody here";
        assert_eq!(strip_frontmatter(content), "# Hello\nBody here");
    }

    #[test]
    fn test_strip_frontmatter_empty_body() {
        let content = "---\nname: test\n---\n";
        assert_eq!(strip_frontmatter(content), "");
    }

    #[test]
    fn test_parse_frontmatter_both() {
        let content = "---\nname: my-skill\ndescription: Does stuff\n---\nBody";
        let (name, desc) = parse_frontmatter(content);
        assert_eq!(name.unwrap(), "my-skill");
        assert_eq!(desc.unwrap(), "Does stuff");
    }

    #[test]
    fn test_parse_frontmatter_missing() {
        let content = "No frontmatter here";
        let (name, desc) = parse_frontmatter(content);
        assert!(name.is_none());
        assert!(desc.is_none());
    }

    #[test]
    fn test_parse_frontmatter_partial() {
        let content = "---\ndescription: Only desc\n---\nBody";
        let (name, desc) = parse_frontmatter(content);
        assert!(name.is_none());
        assert_eq!(desc.unwrap(), "Only desc");
    }

    #[test]
    fn test_resolve_rejects_path_traversal() {
        let cwd = Path::new("/tmp/fake");
        assert!(resolve_skill(cwd, "../../etc/passwd").is_none());
        assert!(resolve_skill(cwd, "path/name").is_none());
        assert!(resolve_skill(cwd, "has space").is_none());
        assert!(resolve_skill(cwd, "").is_none());
    }

    #[test]
    fn test_resolve_accepts_valid_names() {
        assert!(PROMPT_NAME_REGEX.is_match("valid-skill"));
        assert!(PROMPT_NAME_REGEX.is_match("my_skill_v2"));
        assert!(PROMPT_NAME_REGEX.is_match("CamelCase"));
        // resolve returns None because files don't exist, but passes validation
        let cwd = Path::new("/tmp/fake");
        assert!(resolve_skill(cwd, "valid-skill").is_none());
    }

    #[test]
    fn test_discover_nonexistent_local_dir() {
        let cwd = Path::new("/tmp/nonexistent-kiro-test-dir");
        let result = discover(cwd);
        // Local skills should be empty since the cwd doesn't exist.
        // Global skills may or may not exist depending on the test environment.
        assert!(!result.contains_key("skill:local"));
    }
}
