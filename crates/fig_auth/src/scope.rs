use crate::consts::SCOPES;

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
pub(crate) fn is_scopes<S: AsRef<str>>(scopes: &[S]) -> bool {
    scopes_match(SCOPES, scopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scopes_match() {
        assert!(scopes_match(&["a", "b", "c"], &["a", "b", "c"]));
        assert!(scopes_match(&["a", "b", "c"], &["a", "c", "b"]));
        assert!(!scopes_match(&["a", "b", "c"], &["a", "b"]));
        assert!(!scopes_match(&["a", "b"], &["a", "b", "c"]));

        assert!(is_scopes(SCOPES));
    }
}
