use radix_trie::{
    Trie,
    TrieCommon,
};

#[derive(Debug, Clone, Default)]
pub struct CompletionCache {
    trie: Trie<String, f64>,
}

impl CompletionCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.trie = Trie::new();
    }

    pub fn get_insert_text(&self, buffer: &str) -> Option<&str> {
        self.trie.get_raw_descendant(buffer).and_then(|descendant| {
            descendant
                .iter()
                .min_by(|(_, a), (_, b)| a.total_cmp(b))
                .map(|(k, _)| k.as_str())
        })
    }

    /// Lower values are higher priority.
    pub fn insert(&mut self, recommendation: impl Into<String>, value: f64) {
        self.trie.insert(recommendation.into(), value);
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn mock_cache() -> CompletionCache {
        let mut cache = CompletionCache::new();

        cache.insert("echo hello, world", 0.5);
        cache.insert("echo hello there", 1.0);

        cache.insert("ls", 0.5);
        cache.insert("ls -l", 1.0);
        cache.insert("ls -al", 1.0);

        cache.insert("cat -n", 0.5);
        cache.insert("cat", 1.0);

        cache
    }

    #[test]
    fn test_get_insert_text() {
        let cache = mock_cache();

        assert_eq!(cache.get_insert_text("e"), Some("echo hello, world"));
        assert_eq!(cache.get_insert_text("ech"), Some("echo hello, world"));
        assert_eq!(cache.get_insert_text("echo hello "), Some("echo hello there"));

        assert_eq!(cache.get_insert_text("l"), Some("ls"));
        assert_eq!(cache.get_insert_text("ls -l"), Some("ls -l"));
        assert_eq!(cache.get_insert_text("ls -a"), Some("ls -al"));

        assert_eq!(cache.get_insert_text("c"), Some("cat -n"));
        assert_eq!(cache.get_insert_text("cat -"), Some("cat -n"));
        assert_eq!(cache.get_insert_text("cat -n"), Some("cat -n"));

        assert_eq!(cache.get_insert_text("other"), None);
    }
}
