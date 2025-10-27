use std::path::Path;

use glob::Pattern;

/// Pattern-based file filtering for semantic search indexing
#[derive(Debug, Clone)]
pub struct PatternFilter {
    include_patterns: Vec<Pattern>,
    exclude_patterns: Vec<Pattern>,
}

impl PatternFilter {
    /// Create a new pattern filter
    pub fn new(include_patterns: &[String], exclude_patterns: &[String]) -> Result<Self, String> {
        let include_patterns = include_patterns
            .iter()
            .map(|p| Pattern::new(p).map_err(|e| format!("Invalid include pattern '{p}': {e}")))
            .collect::<Result<Vec<_>, _>>()?;

        let exclude_patterns = exclude_patterns
            .iter()
            .map(|p| Pattern::new(p).map_err(|e| format!("Invalid exclude pattern '{p}': {e}")))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            include_patterns,
            exclude_patterns,
        })
    }

    /// Check if a file should be included based on patterns
    /// Handles both absolute and relative paths automatically
    pub fn should_include(&self, file_path: &Path) -> bool {
        // Check include patterns (if any)
        if !self.include_patterns.is_empty() {
            let matches_include = self
                .include_patterns
                .iter()
                .any(|pattern| Self::matches_pattern(pattern, file_path));
            if !matches_include {
                return false;
            }
        }

        // Check exclude patterns (if any)
        if !self.exclude_patterns.is_empty() {
            let matches_exclude = self
                .exclude_patterns
                .iter()
                .any(|pattern| Self::matches_pattern(pattern, file_path));
            if matches_exclude {
                return false;
            }
        }

        true
    }

    /// Match a pattern against a path, handling both absolute and relative paths
    fn matches_pattern(pattern: &Pattern, file_path: &Path) -> bool {
        let path_str = file_path.to_string_lossy();

        // Try direct match first (for relative paths)
        if pattern.matches(&path_str) {
            return true;
        }

        // For absolute paths, try matching against path components
        // This handles cases where pattern is "node_modules/**" but path is
        // "/full/path/to/node_modules/file"
        let components: Vec<_> = file_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect();

        // Try to find a suffix of the path that matches the pattern
        for i in 0..components.len() {
            let suffix_path = components[i..].join("/");
            if pattern.matches(&suffix_path) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_pattern_filter_creation() {
        let filter = PatternFilter::new(&["*.rs".to_string()], &["target/**".to_string()]);
        assert!(filter.is_ok());

        let invalid_filter = PatternFilter::new(&["[".to_string()], &[]);
        assert!(invalid_filter.is_err());
        assert!(invalid_filter.unwrap_err().contains("Invalid include pattern"));
    }

    #[test]
    fn test_include_patterns_work() {
        // Test that include patterns work correctly
        let include_patterns = vec!["*.rs".to_string()];
        let exclude_patterns = vec![];

        let filter = PatternFilter::new(&include_patterns, &exclude_patterns).unwrap();

        // Should include .rs files
        assert!(filter.should_include(&PathBuf::from("main.rs")));
        assert!(filter.should_include(&PathBuf::from("lib.rs")));

        // Should not include other files
        assert!(!filter.should_include(&PathBuf::from("main.py")));
        assert!(!filter.should_include(&PathBuf::from("README.md")));
    }

    #[test]
    fn test_exclude_patterns_work() {
        // Test that exclude patterns work correctly
        let include_patterns = vec![];
        let exclude_patterns = vec!["node_modules/**".to_string()];

        let filter = PatternFilter::new(&include_patterns, &exclude_patterns).unwrap();

        // Should exclude files in node_modules
        assert!(!filter.should_include(&PathBuf::from("node_modules/package/index.js")));
        assert!(!filter.should_include(&PathBuf::from("node_modules/lib.rs")));

        // Should include other files
        assert!(filter.should_include(&PathBuf::from("src/main.rs")));
        assert!(filter.should_include(&PathBuf::from("README.md")));
    }

    #[test]
    fn test_recursive_patterns() {
        // Test that recursive patterns (**) work correctly
        let include_patterns = vec!["**/*.rs".to_string()];
        let exclude_patterns = vec![];

        let filter = PatternFilter::new(&include_patterns, &exclude_patterns).unwrap();

        // Should include .rs files at any depth
        assert!(filter.should_include(&PathBuf::from("main.rs")));
        assert!(filter.should_include(&PathBuf::from("src/main.rs")));
        assert!(filter.should_include(&PathBuf::from("src/lib/mod.rs")));
        assert!(filter.should_include(&PathBuf::from("deep/nested/path/file.rs")));

        // Should not include non-.rs files
        assert!(!filter.should_include(&PathBuf::from("src/main.py")));
        assert!(!filter.should_include(&PathBuf::from("deep/nested/README.md")));
    }

    #[test]
    fn test_combined_include_exclude() {
        // Test that include and exclude patterns work together
        let include_patterns = vec!["**/*.rs".to_string()];
        let exclude_patterns = vec!["target/**".to_string()];

        let filter = PatternFilter::new(&include_patterns, &exclude_patterns).unwrap();

        // Should include .rs files not in target
        assert!(filter.should_include(&PathBuf::from("src/main.rs")));
        assert!(filter.should_include(&PathBuf::from("tests/test.rs")));

        // Should exclude .rs files in target
        assert!(!filter.should_include(&PathBuf::from("target/debug/main.rs")));
        assert!(!filter.should_include(&PathBuf::from("target/release/lib.rs")));

        // Should exclude non-.rs files everywhere
        assert!(!filter.should_include(&PathBuf::from("src/main.py")));
        assert!(!filter.should_include(&PathBuf::from("README.md")));
    }

    #[test]
    fn test_node_modules_exclusion_issue() {
        // Test the specific issue mentioned in PR: node_modules exclusion not working
        let include_patterns = vec![];
        let exclude_patterns = vec!["node_modules/**".to_string()];

        let filter = PatternFilter::new(&include_patterns, &exclude_patterns).unwrap();

        // These should be excluded (the reported bug)
        assert!(!filter.should_include(&PathBuf::from("node_modules/package.json")));
        assert!(!filter.should_include(&PathBuf::from("node_modules/lib/index.js")));
        assert!(!filter.should_include(&PathBuf::from("node_modules/deep/nested/file.txt")));

        // These should be included
        assert!(filter.should_include(&PathBuf::from("src/index.js")));
        assert!(filter.should_include(&PathBuf::from("package.json")));
    }

    #[test]
    fn test_node_modules_exclusion_with_temp_dir() {
        use std::fs;

        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create the directory structure
        fs::create_dir_all(temp_path.join("node_modules/some-package")).unwrap();
        fs::create_dir_all(temp_path.join("src")).unwrap();

        // Create files
        fs::write(temp_path.join("node_modules/package.json"), "{}").unwrap();
        fs::write(temp_path.join("node_modules/some-package/index.js"), "// code").unwrap();
        fs::write(temp_path.join("src/main.js"), "// main code").unwrap();
        fs::write(temp_path.join("package.json"), "{}").unwrap();

        // Test the filter
        let include_patterns = vec![];
        let exclude_patterns = vec!["node_modules/**".to_string()];
        let filter = PatternFilter::new(&include_patterns, &exclude_patterns).unwrap();

        // Test relative to temp directory
        let node_modules_file = PathBuf::from("node_modules/package.json");
        let node_modules_nested = PathBuf::from("node_modules/some-package/index.js");
        let src_file = PathBuf::from("src/main.js");
        let root_file = PathBuf::from("package.json");

        // These should be excluded (the reported bug)
        assert!(
            !filter.should_include(&node_modules_file),
            "node_modules/package.json should be excluded"
        );
        assert!(
            !filter.should_include(&node_modules_nested),
            "node_modules/some-package/index.js should be excluded"
        );

        // These should be included
        assert!(filter.should_include(&src_file), "src/main.js should be included");
        assert!(filter.should_include(&root_file), "package.json should be included");
    }

    #[test]
    fn test_pattern_documentation_accuracy() {
        let filter = PatternFilter::new(&["*.rs".to_string()], &[]).unwrap();

        assert!(filter.should_include(&PathBuf::from("main.rs"))); // Current dir - should match

        // These should NOT match with *.rs (only * not **)
        // If they do match, then the documentation is wrong
        let _nested_matches = filter.should_include(&PathBuf::from("src/main.rs"));
        assert!(_nested_matches, "*.rs should match nested files recursively");
    }

    #[test]
    fn test_empty_patterns() {
        // Test behavior with no patterns (should include everything)
        let include_patterns = vec![];
        let exclude_patterns = vec![];

        let filter = PatternFilter::new(&include_patterns, &exclude_patterns).unwrap();

        // Should include everything when no patterns are specified
        assert!(filter.should_include(&PathBuf::from("main.rs")));
        assert!(filter.should_include(&PathBuf::from("src/main.rs")));
        assert!(filter.should_include(&PathBuf::from("node_modules/package.json")));
        assert!(filter.should_include(&PathBuf::from("target/debug/main")));
    }

    #[test]
    fn test_absolute_vs_relative_path_handling() {
        use std::fs;

        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create directory structure
        fs::create_dir_all(temp_path.join("node_modules")).unwrap();
        fs::create_dir_all(temp_path.join("src")).unwrap();

        let filter = PatternFilter::new(&[], &["node_modules/**".to_string()]).unwrap();

        // Test relative paths (should work)
        let relative_excluded = PathBuf::from("node_modules/package.json");
        let relative_included = PathBuf::from("src/main.js");

        assert!(
            !filter.should_include(&relative_excluded),
            "Relative node_modules path should be excluded"
        );
        assert!(
            filter.should_include(&relative_included),
            "Relative src path should be included"
        );

        // Test absolute paths (the fix - should also work now)
        let absolute_excluded = temp_path.join("node_modules/package.json");
        let absolute_included = temp_path.join("src/main.js");

        assert!(
            !filter.should_include(&absolute_excluded),
            "Absolute node_modules path should be excluded"
        );
        assert!(
            filter.should_include(&absolute_included),
            "Absolute src path should be included"
        );
    }
}
