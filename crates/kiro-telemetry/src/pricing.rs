/// Token counts for a single model response, feeding `kiro_cli_tokens_consumed`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TokenUsage {
    pub uncached_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub output_tokens: u64,
}

impl TokenUsage {
    pub fn from_signed_counts(
        uncached_input_tokens: Option<i64>,
        cache_read_input_tokens: Option<i64>,
        cache_write_input_tokens: Option<i64>,
        output_tokens: Option<i64>,
    ) -> Self {
        Self {
            uncached_input_tokens: positive_i64_to_u64(uncached_input_tokens),
            cache_read_input_tokens: positive_i64_to_u64(cache_read_input_tokens),
            cache_write_input_tokens: positive_i64_to_u64(cache_write_input_tokens),
            output_tokens: positive_i64_to_u64(output_tokens),
        }
    }

    pub fn is_empty(self) -> bool {
        self.uncached_input_tokens == 0
            && self.cache_read_input_tokens == 0
            && self.cache_write_input_tokens == 0
            && self.output_tokens == 0
    }
}

fn positive_i64_to_u64(value: Option<i64>) -> u64 {
    value.filter(|value| *value > 0).unwrap_or_default() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_usage_from_positive_signed_counts() {
        assert_eq!(
            TokenUsage::from_signed_counts(Some(10), Some(-1), None, Some(5)),
            TokenUsage {
                uncached_input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            }
        );
    }
}
