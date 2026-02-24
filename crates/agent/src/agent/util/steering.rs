//! Utilities for steering file frontmatter parsing and inclusion filtering.

use serde::Deserialize;

/// Returns `true` if the given path is a steering file that should be
/// subject to frontmatter inclusion filtering.
pub fn is_steering_file(path: &str) -> bool {
    path.contains(".kiro/steering") && path.ends_with(".md")
}

/// Parses the frontmatter of a steering file and returns whether it should
/// be included in the agent context.
pub fn should_include_steering_file(content: &str) -> bool {
    let Some(yaml) = extract_yaml_frontmatter(content) else {
        return true;
    };
    #[derive(Deserialize)]
    struct FrontMatter {
        inclusion: Option<String>,
    }
    match serde_yaml::from_str::<FrontMatter>(&yaml) {
        Ok(fm) => !matches!(fm.inclusion.as_deref(), Some("fileMatch" | "manual")),
        Err(_) => true,
    }
}

/// Extracts the YAML frontmatter block from a markdown file, if present.
pub fn extract_yaml_frontmatter(content: &str) -> Option<String> {
    if !content.starts_with("---\n") {
        return None;
    }
    let lines: Vec<&str> = content.lines().collect();
    let end = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| l.trim() == "---")
        .map(|(i, _)| i)?;
    Some(lines[1..end].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frontmatter_included() {
        assert!(should_include_steering_file("# Title\nContent"));
    }

    #[test]
    fn inclusion_always_included() {
        assert!(should_include_steering_file("---\ninclusion: always\n---\nContent"));
    }

    #[test]
    fn inclusion_filematch_excluded() {
        assert!(!should_include_steering_file("---\ninclusion: fileMatch\n---\nContent"));
    }

    #[test]
    fn inclusion_manual_excluded() {
        assert!(!should_include_steering_file("---\ninclusion: manual\n---\nContent"));
    }

    #[test]
    fn malformed_frontmatter_included() {
        assert!(should_include_steering_file("---\ninvalid: [\n---\nContent"));
    }

    #[test]
    fn is_steering_file_matches_correctly() {
        assert!(is_steering_file("/Users/foo/.kiro/steering/test.md"));
        assert!(!is_steering_file("/Users/foo/.kiro/steering/test.txt"));
        assert!(!is_steering_file("/Users/foo/other/test.md"));
    }
}
