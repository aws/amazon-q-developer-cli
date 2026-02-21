use std::collections::HashMap;
use std::path::Path;

use super::super::mcp::types::Prompt;

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
                local_prompts.push(Prompt {
                    name: name.to_string(),
                    description: None,
                    arguments: None,
                });
            }
        }
        if !local_prompts.is_empty() {
            result.insert("local".to_string(), local_prompts);
        }
    }

    // Global prompts (~/.kiro/prompts/*.md) - skip if already in local
    if let Some(home) = dirs::home_dir()
        && let Ok(entries) = std::fs::read_dir(home.join(".kiro").join("prompts"))
    {
        let mut global_prompts = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
                && !seen.contains(name)
            {
                global_prompts.push(Prompt {
                    name: name.to_string(),
                    description: None,
                    arguments: None,
                });
            }
        }
        if !global_prompts.is_empty() {
            result.insert("global".to_string(), global_prompts);
        }
    }

    result
}
