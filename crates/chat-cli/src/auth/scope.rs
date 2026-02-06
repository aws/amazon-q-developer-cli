use crate::auth::consts::{
    DEFAULT_SCOPE_PREFIX,
    SCOPE_SUFFIXES,
};
use crate::database::Database;
use crate::database::settings::Setting;

/// Get the configured scope prefix, or use default
pub(crate) fn get_scope_prefix(database: &Database) -> String {
    database
        .settings
        .get_string(Setting::ApiOidcScopePrefix)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_SCOPE_PREFIX.to_string())
}

/// Build scopes with the configured prefix
pub(crate) fn get_scopes(database: &Database) -> Vec<String> {
    let prefix = get_scope_prefix(database);
    SCOPE_SUFFIXES.iter().map(|s| format!("{}{}", prefix, s)).collect()
}

pub fn scopes_match<A: AsRef<str>, B: AsRef<str>>(a: &[A], b: &[B]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut a = a.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
    let mut b = b.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
    a.sort();
    b.sort();
    a == b
}

/// Checks if the given scopes match the predefined scopes.
pub(crate) fn is_scopes<S: AsRef<str>>(scopes: &[S], database: &Database) -> bool {
    let expected = get_scopes(database);
    scopes_match(&expected, scopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build scopes with a given prefix (for tests without a database).
    fn build_scopes_with_prefix(prefix: &str) -> Vec<String> {
        SCOPE_SUFFIXES.iter().map(|s| format!("{}{}", prefix, s)).collect()
    }

    #[test]
    fn test_scopes_match() {
        assert!(scopes_match(&["a", "b", "c"], &["a", "b", "c"]));
        assert!(scopes_match(&["a", "b", "c"], &["a", "c", "b"]));
        assert!(!scopes_match(&["a", "b", "c"], &["a", "b"]));
        assert!(!scopes_match(&["a", "b"], &["a", "b", "c"]));
    }

    #[test]
    fn test_default_scopes_content() {
        let scopes = build_scopes_with_prefix(DEFAULT_SCOPE_PREFIX);
        assert_eq!(scopes, vec![
            "codewhisperer:completions",
            "codewhisperer:analysis",
            "codewhisperer:conversations",
        ]);
    }
}
