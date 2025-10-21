use crate::model::entities::SymbolInfo;

/// Calculate the similarity score between a query and a symbol name using strsim algorithms
pub fn calculate_symbol_score(query: &str, symbol_name: &str, symbol_type: Option<&str>) -> f64 {
    if query.is_empty() {
        return 0.0;
    }

    // Exact match gets highest score
    if query == symbol_name {
        return 1.0;
    }

    // Prefix match gets high score
    if symbol_name.starts_with(query) {
        return 0.9;
    }

    // Contains match gets medium-high score
    if symbol_name.contains(query) {
        return 0.8;
    }

    // Use strsim algorithms for fuzzy matching
    let jaro_winkler_score = strsim::jaro_winkler(query, symbol_name);
    let normalized_levenshtein_score = strsim::normalized_levenshtein(query, symbol_name);
    let sorensen_dice_score = strsim::sorensen_dice(query, symbol_name);

    // Combine scores with weights
    let fuzzy_score = (jaro_winkler_score * 0.4)
        + (normalized_levenshtein_score * 0.4)
        + (sorensen_dice_score * 0.2);

    // Additional scoring for camelCase and snake_case patterns
    let pattern_score = calculate_pattern_score(query, symbol_name);
    let combined_score = (fuzzy_score * 0.7) + (pattern_score * 0.3);

    // Boost score for important symbol types
    let kind_boost = match symbol_type {
        Some("Function") | Some("Method") => 1.1,
        Some("Class") | Some("Interface") => 1.05,
        Some("Constant") => 1.02,
        _ => 1.0,
    };

    (combined_score * kind_boost).min(1.0)
}

/// Calculate additional score based on naming patterns (camelCase, snake_case, etc.)
fn calculate_pattern_score(query: &str, symbol_name: &str) -> f64 {
    let camel_case_score = calculate_camel_case_score(query, symbol_name);
    let snake_case_score = calculate_snake_case_score(query, symbol_name);
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

/// Search symbols using fuzzy matching
pub fn search_symbols_fuzzy(
    symbols: Vec<SymbolInfo>,
    query: &str,
    limit: usize,
    min_score: f64,
) -> Vec<SymbolInfo> {
    if symbols.is_empty() || query.is_empty() {
        return symbols.into_iter().take(limit).collect();
    }

    let query_lower = query.to_lowercase();
    let mut scored_symbols: Vec<(f64, SymbolInfo)> = Vec::new();

    for symbol in symbols {
        let score = calculate_symbol_score(
            &query_lower,
            &symbol.name.to_lowercase(),
            symbol.symbol_type.as_deref(),
        );

        if score >= min_score {
            scored_symbols.push((score, symbol));
        }
    }

    scored_symbols.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored_symbols.truncate(limit);

    scored_symbols
        .into_iter()
        .map(|(_, symbol)| symbol)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_symbol(name: &str, symbol_type: &str) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            symbol_type: Some(symbol_type.to_string()),
            file_path: "test.ts".to_string(),
            fully_qualified_name: format!("test.ts::{}", name),
            start_row: 1,
            start_column: 1,
            end_row: 1,
            end_column: 10,
            container_name: None,
            detail: None,
            source_line: None,
        }
    }

    #[test]
    fn test_exact_match() {
        let score = calculate_symbol_score("greet", "greet", Some("Function"));
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_prefix_match() {
        let score = calculate_symbol_score("calc", "calculate", Some("Function"));
        assert_eq!(score, 0.9);
    }

    #[test]
    fn test_contains_match() {
        let score = calculate_symbol_score("user", "getuser", Some("Function"));
        assert_eq!(score, 0.8);
    }

    #[test]
    fn test_strsim_algorithms() {
        let jw_score = strsim::jaro_winkler("gret", "greet");
        let lev_score = strsim::normalized_levenshtein("gret", "greet");
        let dice_score = strsim::sorensen_dice("gret", "greet");

        println!("Jaro-Winkler: {}", jw_score);
        println!("Normalized Levenshtein: {}", lev_score);
        println!("Sorensen-Dice: {}", dice_score);

        assert!(jw_score > 0.0);
        assert!(lev_score > 0.0);
        assert!(dice_score >= 0.0);
    }

    #[test]
    fn test_typo_tolerance() {
        let score = calculate_symbol_score("gret", "greet", Some("Function"));
        println!("Typo score for 'gret' vs 'greet': {}", score);
        assert!(score > 0.0);
    }

    #[test]
    fn test_no_crazy_matches() {
        // Test that unrelated strings don't match
        let score = calculate_symbol_score("greet_user", "result", Some("Variable"));
        println!("Score for 'greet_user' vs 'result': {}", score);
        assert!(
            score < 0.4,
            "Unrelated strings should not match with score >= 0.4"
        );

        let score2 = calculate_symbol_score("function", "variable", Some("Variable"));
        println!("Score for 'function' vs 'variable': {}", score2);
        assert!(
            score2 < 0.4,
            "Unrelated strings should not match with score >= 0.4"
        );

        // But related strings should still match
        let score3 = calculate_symbol_score("greet", "greeting", Some("Variable"));
        println!("Score for 'greet' vs 'greeting': {}", score3);
        assert!(
            score3 >= 0.4,
            "Related strings should match with score >= 0.4"
        );
    }

    #[test]
    fn test_multiple_keywords() {
        // Test multiple keywords separated by space
        let score1 = calculate_symbol_score("auth login", "AuthenticationImpl", Some("Class"));
        println!("Score for 'auth login' vs 'AuthenticationImpl': {}", score1);

        let score2 = calculate_symbol_score("auth login logout", "LoginService", Some("Class"));
        println!(
            "Score for 'auth login logout' vs 'LoginService': {}",
            score2
        );

        let score3 = calculate_symbol_score("user auth", "UserAuthenticator", Some("Class"));
        println!("Score for 'user auth' vs 'UserAuthenticator': {}", score3);

        // Current implementation probably treats "auth login" as one string
        // This test will show us what happens
    }

    #[test]
    fn test_search_with_different_thresholds() {
        let symbols = vec![
            create_symbol("greet", "Function"),
            create_symbol("calculate", "Function"),
        ];

        let strict_results = search_symbols_fuzzy(symbols.clone(), "gre", 10, 0.8);
        let loose_results = search_symbols_fuzzy(symbols, "gre", 10, 0.1);

        println!("Strict results: {}", strict_results.len());
        println!("Loose results: {}", loose_results.len());

        assert!(loose_results.len() >= strict_results.len());
    }
}
