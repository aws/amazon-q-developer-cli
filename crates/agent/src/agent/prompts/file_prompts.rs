use std::collections::HashMap;
use std::path::Path;

use super::super::mcp::types::Prompt;
use super::template_args::PromptTemplateArgs;

/// Discover file-based prompts from .kiro/prompts/*.md directories.
/// Returns a HashMap<String, Vec<Prompt>> keyed by source ("local" or "global").
pub fn discover(cwd: &Path) -> HashMap<String, Vec<Prompt>> {
    let mut result = HashMap::new();
    let mut seen = std::collections::HashSet::new();

    // Local prompts (cwd/.kiro/prompts/*.md)
    let local_dir = cwd.join(".kiro").join("prompts");
    if let Ok(entries) = std::fs::read_dir(&local_dir) {
        let mut local_prompts = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            {
                seen.insert(name.to_string());
                let arguments = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|content| PromptTemplateArgs::parse(&content).to_prompt_arguments());
                local_prompts.push(Prompt {
                    name: name.to_string(),
                    description: None,
                    arguments,
                });
            }
        }
        if !local_prompts.is_empty() {
            result.insert("local".to_string(), local_prompts);
        }
    }

    // Global prompts (~/.kiro/prompts/*.md, or $KIRO_HOME/prompts/*.md) - skip if already in local
    if let Ok(kiro_home) = crate::agent::util::directories::kiro_home_dir()
        && let Ok(entries) = std::fs::read_dir(kiro_home.join("prompts"))
    {
        let mut global_prompts = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
                && !seen.contains(name)
            {
                let arguments = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|content| PromptTemplateArgs::parse(&content).to_prompt_arguments());
                global_prompts.push(Prompt {
                    name: name.to_string(),
                    description: None,
                    arguments,
                });
            }
        }
        if !global_prompts.is_empty() {
            result.insert("global".to_string(), global_prompts);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_no_prompts_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result = discover(tmp.path());
        // Local dir doesn't exist; no local prompts
        assert!(result.get("local").is_none());
    }

    #[test]
    fn test_discover_local_prompts() {
        let tmp = tempfile::tempdir().unwrap();
        let prompts_dir = tmp.path().join(".kiro").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(prompts_dir.join("hello.md"), "Hello").unwrap();
        std::fs::write(prompts_dir.join("greet.md"), "Greet {{name}}").unwrap();
        // Non-md file should be ignored
        std::fs::write(prompts_dir.join("README.txt"), "ignored").unwrap();

        let result = discover(tmp.path());
        let locals = result.get("local").expect("should have local prompts");
        assert_eq!(locals.len(), 2);
        let names: Vec<_> = locals.iter().map(|p| p.name.clone()).collect();
        assert!(names.contains(&"hello".to_string()));
        assert!(names.contains(&"greet".to_string()));
    }

    #[test]
    fn test_discover_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let prompts_dir = tmp.path().join(".kiro").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        let result = discover(tmp.path());
        // Empty dir, no entries inserted
        assert!(result.get("local").is_none());
    }

    #[test]
    fn test_discover_only_non_md() {
        let tmp = tempfile::tempdir().unwrap();
        let prompts_dir = tmp.path().join(".kiro").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(prompts_dir.join("file.txt"), "ignored").unwrap();
        std::fs::write(prompts_dir.join("config.json"), "{}").unwrap();
        let result = discover(tmp.path());
        // No .md files; no prompts
        assert!(result.get("local").is_none());
    }

    #[test]
    fn test_discover_md_with_arguments() {
        let tmp = tempfile::tempdir().unwrap();
        let prompts_dir = tmp.path().join(".kiro").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        // Use template syntax that PromptTemplateArgs would parse
        std::fs::write(
            prompts_dir.join("p1.md"),
            "---\nargs:\n  - name: x\n    required: true\n---\nUse {{x}}",
        )
        .unwrap();

        let result = discover(tmp.path());
        let locals = result.get("local").expect("should have local prompts");
        assert_eq!(locals.len(), 1);
        assert_eq!(locals[0].name, "p1");
    }
}
