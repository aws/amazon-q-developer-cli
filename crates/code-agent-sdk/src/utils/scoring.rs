//! Fuzzy scoring utilities for symbol matching

/// Calculate the similarity score between a query and a symbol name
///
/// This function implements a multi-layered scoring algorithm with case-sensitive boosting:
/// 1. Exact match gets the highest score (1.0 for exact case, 0.95 for case-insensitive)
/// 2. Prefix match gets high score (0.9 for exact case, 0.85 for case-insensitive)
/// 3. Contains match gets medium-high score (0.8 for exact case, 0.75 for case-insensitive)
/// 4. Fuzzy matching using Jaro-Winkler algorithm with pattern scoring
pub fn calculate_fuzzy_score(
    filter_lower: &str,
    label_lower: &str,
    filter_original: &str,
    label_original: &str,
) -> f64 {
    // Handle empty query - return all symbols with neutral score
    if filter_lower.is_empty() {
        return 0.5;
    }

    // Exact match (case-insensitive)
    if filter_lower == label_lower {
        // Exact case match gets perfect score
        return if filter_original == label_original { 1.0 } else { 0.95 };
    }

    // Prefix match (case-insensitive)
    if label_lower.starts_with(filter_lower) {
        // Case-sensitive prefix gets higher score
        return if label_original.starts_with(filter_original) {
            0.9
        } else {
            0.85
        };
    }

    // Contains match (case-insensitive)
    if label_lower.contains(filter_lower) {
        // Case-sensitive contains gets higher score
        return if label_original.contains(filter_original) {
            0.8
        } else {
            0.75
        };
    }

    // Fuzzy matching using Jaro-Winkler
    let jaro_winkler_score = strsim::jaro_winkler(filter_lower, label_lower);

    // Additional scoring for camelCase and snake_case patterns
    let pattern_score = calculate_pattern_score(filter_lower, label_lower);
    (jaro_winkler_score * 0.7) + (pattern_score * 0.3)
}

/// Calculate additional score based on naming patterns (camelCase, snake_case, etc.)
fn calculate_pattern_score(query: &str, symbol_name: &str) -> f64 {
    // Check if query matches word boundaries in camelCase
    let camel_case_score = calculate_camel_case_score(query, symbol_name);

    // Check if query matches word boundaries in snake_case
    let snake_case_score = calculate_snake_case_score(query, symbol_name);

    // Return the best pattern score
    camel_case_score.max(snake_case_score)
}

/// Calculate score for camelCase pattern matching
fn calculate_camel_case_score(query: &str, symbol_name: &str) -> f64 {
    // Extract capital letters and first letter for camelCase matching
    let mut camel_chars = Vec::new();
    let mut chars = symbol_name.chars();

    // Always include first character
    if let Some(first_char) = chars.next() {
        camel_chars.push(first_char.to_lowercase().next().unwrap_or(first_char));
    }

    // Add capital letters
    for ch in chars {
        if ch.is_uppercase() {
            camel_chars.push(ch.to_lowercase().next().unwrap_or(ch));
        }
    }

    let camel_string: String = camel_chars.iter().collect();

    if camel_string.starts_with(query) {
        return 0.7;
    }

    if camel_string.contains(query) {
        return 0.5;
    }

    // Use Jaro-Winkler for fuzzy matching on camelCase pattern
    strsim::jaro_winkler(query, &camel_string) * 0.6
}

/// Calculate score for snake_case pattern matching
fn calculate_snake_case_score(query: &str, symbol_name: &str) -> f64 {
    // Split on underscores and check if query matches word starts
    let words: Vec<&str> = symbol_name.split('_').collect();

    // Check if query matches the start of any word
    for word in &words {
        if word.starts_with(query) {
            return 0.6;
        }
    }

    // Check if query matches concatenated first letters of words
    let first_letters: String = words
        .iter()
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_lowercase();

    if first_letters.starts_with(query) {
        return 0.5;
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert_eq!(calculate_fuzzy_score("foo", "foo", "foo", "foo"), 1.0);
        assert_eq!(calculate_fuzzy_score("foo", "foo", "Foo", "foo"), 0.95);
    }

    #[test]
    fn test_prefix_match() {
        assert!((calculate_fuzzy_score("user", "userservice", "User", "UserService") - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_contains_match() {
        assert!((calculate_fuzzy_score("service", "userservice", "Service", "UserService") - 0.8).abs() < 0.01);
    }
}
