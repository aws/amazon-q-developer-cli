//! Monty Python character name generator for knight naming.

use std::collections::HashSet;

/// Knight names from Monty Python and the Holy Grail.
const KNIGHT_NAMES: &[&str] = &[
    "Lancelot", "Galahad", "Robin", "Bedevere", "Patsy", "Tim", "Zoot", "Herbert", "Bors", "Gawain", "Ector",
];

/// Simple first names for squires.
const SQUIRE_NAMES: &[&str] = &[
    "Bob", "Alice", "Charlie", "Dave", "Eve", "Frank", "Grace", "Hank", "Iris", "Jack", "Kate", "Leo", "Mia", "Nate",
    "Olive", "Pete", "Quinn", "Rosa", "Sam", "Tess",
];

/// Pick the series name (always Holy Grail for knights).
pub fn pick_series() -> &'static str {
    "Monty Python and the Holy Grail"
}

/// Get the next available knight name.
///
/// Assigns names in order. If all names are taken, appends a number suffix.
pub fn next_name(_series_name: &str, used_names: &HashSet<String>) -> String {
    // Assign in order (not random) for predictability
    for name in KNIGHT_NAMES {
        if !used_names.contains(*name) {
            return name.to_string();
        }
    }

    // All names taken — append number
    for i in 2.. {
        for name in KNIGHT_NAMES {
            let numbered = format!("{}-{}", name, i);
            if !used_names.contains(&numbered) {
                return numbered;
            }
        }
    }

    unreachable!()
}

/// Get the next available squire name.
///
/// Assigns names in order. If all names are taken, appends a number suffix.
pub fn next_squire_name(used_names: &HashSet<String>) -> String {
    // Assign in order (not random) for predictability
    for name in SQUIRE_NAMES {
        if !used_names.contains(*name) {
            return name.to_string();
        }
    }

    // All names taken — append number
    for i in 2.. {
        for name in SQUIRE_NAMES {
            let numbered = format!("{}-{}", name, i);
            if !used_names.contains(&numbered) {
                return numbered;
            }
        }
    }

    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_knight_is_lancelot() {
        let name = next_name("", &HashSet::new());
        assert_eq!(name, "Lancelot");
    }

    #[test]
    fn test_names_assigned_in_order() {
        let mut used = HashSet::new();
        let n1 = next_name("", &used);
        used.insert(n1.clone());
        let n2 = next_name("", &used);
        used.insert(n2.clone());
        let n3 = next_name("", &used);

        assert_eq!(n1, "Lancelot");
        assert_eq!(n2, "Galahad");
        assert_eq!(n3, "Robin");
    }

    #[test]
    fn test_overflow_appends_number() {
        let used: HashSet<String> = KNIGHT_NAMES.iter().map(|s| s.to_string()).collect();
        let name = next_name("", &used);
        assert_eq!(name, "Lancelot-2");
    }

    #[test]
    fn test_avoids_used_names() {
        let mut used = HashSet::new();
        used.insert("Lancelot".to_string());
        let name = next_name("", &used);
        assert_eq!(name, "Galahad");
    }

    #[test]
    fn test_first_squire_is_bob() {
        let name = next_squire_name(&HashSet::new());
        assert_eq!(name, "Bob");
    }

    #[test]
    fn test_squire_names_assigned_in_order() {
        let mut used = HashSet::new();
        let n1 = next_squire_name(&used);
        used.insert(n1.clone());
        let n2 = next_squire_name(&used);
        used.insert(n2.clone());
        let n3 = next_squire_name(&used);

        assert_eq!(n1, "Bob");
        assert_eq!(n2, "Alice");
        assert_eq!(n3, "Charlie");
    }

    #[test]
    fn test_squire_overflow_appends_number() {
        let used: HashSet<String> = SQUIRE_NAMES.iter().map(|s| s.to_string()).collect();
        let name = next_squire_name(&used);
        assert_eq!(name, "Bob-2");
    }
}
