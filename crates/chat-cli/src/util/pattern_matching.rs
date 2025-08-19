use std::collections::HashSet;
use globset::Glob;

/// Check if a string matches any pattern in a set of patterns
pub fn matches_any_pattern(patterns: &HashSet<String>, text: &str) -> bool {
    patterns.iter().any(|pattern| {
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
