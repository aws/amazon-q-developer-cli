pub struct TokenCounter;

impl TokenCounter {
    pub const TOKEN_TO_CHAR_RATIO: usize = 3;

    /// Estimates the number of tokens in the input content.
    /// Currently uses a simple heuristic: content length / TOKEN_TO_CHAR_RATIO
    ///
    /// Rounds up to the nearest multiple of 10 to avoid giving users a false sense of precision.
    pub fn count_tokens(content: &str) -> usize {
        (content.len() / Self::TOKEN_TO_CHAR_RATIO + 5) / 10 * 10
    }

    pub const fn token_to_chars(token: usize) -> usize {
        token * Self::TOKEN_TO_CHAR_RATIO
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
