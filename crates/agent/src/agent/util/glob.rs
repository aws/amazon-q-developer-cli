use globset::Glob;

/// Runs a glob match given by `pattern` for all items in `items`, returning the items that
/// matched.
pub fn find_matches<T, U>(pattern: U, items: T) -> Vec<String>
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
{
    let mut matches = Vec::new();
    let Ok(glob) = globset::Glob::new(pattern.as_ref()) else {
        return matches;
    };

    let matcher = glob.compile_matcher();
    for item in items {
        let item = item.as_ref();
        if matcher.is_match(item) {
            matches.push(item.to_string());
        }
    }

    matches
}

/// Check if a string matches any pattern in a set of patterns
pub fn matches_any_pattern<T, U, V>(patterns: T, text: V) -> bool
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
    V: AsRef<str>,
{
    let text = text.as_ref();

    patterns.into_iter().any(|pattern| {
        let pattern = pattern.as_ref();

        // Exact match first
        if pattern == text {
            return true;
        }

        // Glob pattern match if contains wildcards
        if pattern.contains('*') || pattern.contains('?') {
            if let Ok(glob) = Glob::new(pattern) {
                return glob.compile_matcher().is_match(text);
            }
        }

        false
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_exact_match() {
        let mut patterns = HashSet::new();
        patterns.insert("fs_read".to_string());

        assert!(matches_any_pattern(&patterns, "fs_read"));
        assert!(!matches_any_pattern(&patterns, "fs_write"));
    }

    #[test]
    fn test_wildcard_patterns() {
        let mut patterns = HashSet::new();
        patterns.insert("fs_*".to_string());

        assert!(matches_any_pattern(&patterns, "fs_read"));
        assert!(matches_any_pattern(&patterns, "fs_write"));
        assert!(!matches_any_pattern(&patterns, "execute_bash"));
    }

    #[test]
    fn test_mcp_patterns() {
        let mut patterns = HashSet::new();
        patterns.insert("@mcp-server/*".to_string());

        assert!(matches_any_pattern(&patterns, "@mcp-server/tool1"));
        assert!(matches_any_pattern(&patterns, "@mcp-server/tool2"));
        assert!(!matches_any_pattern(&patterns, "@other-server/tool"));
    }

    #[test]
    fn test_question_mark_wildcard() {
        let mut patterns = HashSet::new();
        patterns.insert("fs_?ead".to_string());

        assert!(matches_any_pattern(&patterns, "fs_read"));
        assert!(!matches_any_pattern(&patterns, "fs_write"));
    }
}
