pub struct TokenCounter;

impl TokenCounter {
    /// Estimates the number of tokens in the input content.
    /// Currently uses a simple heuristic: content length / 3
    ///
    /// Rounds up to the nearest multiple of 10 to avoid giving users a false sense of precision.
    pub fn count_tokens(content: &str) -> usize {
        (content.len() / 3 + 5) / 10 * 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_count() {
        let text = "This is a test sentence.";
        let count = TokenCounter::count_tokens(text);
        assert_eq!(count, (text.len() / 3 + 5) / 10 * 10);
    }
}
