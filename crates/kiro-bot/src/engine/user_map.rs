//! Slack user ID → human-readable name resolution.
//!
//! The `[users]` table in `config.toml` maps Slack user IDs to aliases used in
//! Cedar policies and conversation context. Unknown IDs fall back to the raw ID.

use std::collections::HashMap;

pub struct UserMap(HashMap<String, String>);

impl UserMap {
    pub fn empty() -> Self {
        Self(HashMap::new())
    }

    pub fn from_map(map: HashMap<String, String>) -> Self {
        Self(map)
    }

    /// Resolve a Slack user ID to a human name, falling back to the raw ID.
    pub fn resolve<'a>(&'a self, slack_id: &'a str) -> &'a str {
        self.0.get(slack_id).map(|s| s.as_str()).unwrap_or(slack_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_known_user() {
        let map = UserMap(HashMap::from([("U123".into(), "alice".into())]));
        assert_eq!(map.resolve("U123"), "alice");
    }

    #[test]
    fn resolve_unknown_falls_back_to_slack_id() {
        let map = UserMap::empty();
        assert_eq!(map.resolve("U999"), "U999");
    }
}
