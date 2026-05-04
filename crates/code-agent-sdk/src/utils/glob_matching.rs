//! Glob pattern matching utilities for file_patterns and file_extensions.

/// Combine file_patterns and file_extensions into a single list of glob patterns.
pub fn combine_patterns(file_patterns: &[String], file_extensions: &[String]) -> Vec<String> {
    file_patterns
        .iter()
        .cloned()
        .chain(file_extensions.iter().map(|ext| format!("*.{ext}")))
        .collect()
}

/// Match a filename against configs and return language names ranked by specificity.
/// Exact matches first, then most specific glob, then declaration order.
pub fn resolve_language_matches<'a>(
    configs: impl Iterator<Item = (&'a str, Vec<String>)>,
    filename: &str,
) -> Vec<String> {
    let mut ranked: Vec<_> = configs
        .flat_map(|(name, patterns)| {
            patterns
                .into_iter()
                .filter(|pattern| {
                    globset::Glob::new(pattern)
                        .map(|g| g.compile_matcher().is_match(filename))
                        .unwrap_or(false)
                })
                .map(move |pattern| RankedMatch::new(name, &pattern))
        })
        .collect();

    ranked.sort_by_key(|m| std::cmp::Reverse(m.rank()));

    ranked
        .iter()
        .map(|m| m.name.to_string())
        .collect::<indexmap::IndexSet<_>>()
        .into_iter()
        .collect()
}

struct RankedMatch<'a> {
    name: &'a str,
    specificity: usize,
    is_exact: bool,
}

impl<'a> RankedMatch<'a> {
    /// Characters that make a glob pattern non-exact
    const GLOB_SPECIAL_CHARS: &'static [char] = &['*', '?', '[', ']', '{', '}'];

    fn new(name: &'a str, pattern: &str) -> Self {
        let special_count = pattern.chars().filter(|c| Self::GLOB_SPECIAL_CHARS.contains(c)).count();
        Self {
            name,
            specificity: pattern.len() - special_count,
            is_exact: special_count == 0,
        }
    }

    /// Sort key: exact matches first, then by specificity descending
    fn rank(&self) -> (bool, usize) {
        (self.is_exact, self.specificity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combine_patterns() {
        let patterns = combine_patterns(&["Dockerfile".to_string()], &["rs".to_string(), "ts".to_string()]);
        assert_eq!(patterns, vec!["Dockerfile", "*.rs", "*.ts"]);
    }

    #[test]
    fn test_combine_patterns_empty() {
        assert_eq!(combine_patterns(&[], &[]), Vec::<String>::new());
    }

    #[test]
    fn test_resolve_exact_match() {
        let configs = vec![("docker", vec!["Dockerfile".to_string()])];
        let result = resolve_language_matches(configs.into_iter(), "Dockerfile");
        assert_eq!(result, vec!["docker"]);
    }

    #[test]
    fn test_resolve_glob_match() {
        let configs = vec![("docker", vec!["Dockerfile.*".to_string()])];
        let result = resolve_language_matches(configs.into_iter(), "Dockerfile.dev");
        assert_eq!(result, vec!["docker"]);
    }

    #[test]
    fn test_resolve_no_match() {
        let configs = vec![("docker", vec!["Dockerfile".to_string()])];
        let result = resolve_language_matches(configs.into_iter(), "Makefile");
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_only_matching_config() {
        let configs = vec![
            ("docker", vec!["Dockerfile".to_string()]),
            ("yaml", vec!["*.yml".to_string()]),
        ];
        let result = resolve_language_matches(configs.into_iter(), "Dockerfile");
        assert_eq!(result, vec!["docker"]);
    }

    #[test]
    fn test_resolve_exact_wins_over_glob() {
        let configs = vec![
            ("yaml", vec!["*.yml".to_string()]),
            ("compose", vec!["docker-compose.yml".to_string()]),
        ];
        let result = resolve_language_matches(configs.into_iter(), "docker-compose.yml");
        assert_eq!(result[0], "compose");
    }

    #[test]
    fn test_resolve_more_specific_glob_wins() {
        let configs = vec![
            ("yaml", vec!["*.yml".to_string()]),
            ("compose", vec!["docker-compose*.yml".to_string()]),
        ];
        let result = resolve_language_matches(configs.into_iter(), "docker-compose.yml");
        assert_eq!(result[0], "compose");
    }

    #[test]
    fn test_resolve_deduplicates_same_name() {
        let configs = vec![("docker", vec!["Dockerfile".to_string(), "Dockerfile.*".to_string()])];
        let result = resolve_language_matches(configs.into_iter(), "Dockerfile");
        assert_eq!(result, vec!["docker"]);
    }

    #[test]
    fn test_resolve_returns_all_matching() {
        let configs = vec![
            ("broad", vec!["C*g".to_string()]),
            ("specific", vec!["Con*g".to_string()]),
        ];
        let result = resolve_language_matches(configs.into_iter(), "Config");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "specific");
        assert_eq!(result[1], "broad");
    }

    #[test]
    fn test_resolve_empty() {
        let configs: Vec<(&str, Vec<String>)> = vec![];
        let result = resolve_language_matches(configs.into_iter(), "anything");
        assert!(result.is_empty());
    }

    #[test]
    fn test_ranked_match_exact_pattern() {
        let m = RankedMatch::new("test", "Dockerfile");
        assert!(m.is_exact);
        assert_eq!(m.specificity, 10);
    }

    #[test]
    fn test_ranked_match_glob_pattern() {
        let m = RankedMatch::new("test", "*.yml");
        assert!(!m.is_exact);
        assert_eq!(m.specificity, 4);
    }

    #[test]
    fn test_ranked_match_specificity_counts_literals() {
        let broad = RankedMatch::new("a", "C*g");
        let specific = RankedMatch::new("b", "Con*g");
        assert!(specific.specificity > broad.specificity);
    }
}
