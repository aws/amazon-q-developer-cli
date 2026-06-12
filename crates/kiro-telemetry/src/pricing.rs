pub const PRICING_TABLE_VERSION: f64 = 20260606.0;

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

pub fn estimate_cost_usd(model_class: &str, usage: TokenUsage) -> Option<f64> {
    if usage.is_empty() {
        return None;
    }

    let prices = TokenPrices::for_model_class(model_class)?;
    let input_cost = usage.uncached_input_tokens as f64 * prices.input_per_million;
    let cache_read_cost = usage.cache_read_input_tokens as f64 * prices.cache_read_per_million;
    let cache_write_surcharge = usage.cache_write_input_tokens as f64 * prices.cache_write_surcharge_per_million;
    let output_cost = usage.output_tokens as f64 * prices.output_per_million;

    Some((input_cost + cache_read_cost + cache_write_surcharge + output_cost) / 1_000_000.0)
}

#[derive(Clone, Copy, Debug)]
struct TokenPrices {
    input_per_million: f64,
    cache_read_per_million: f64,
    cache_write_surcharge_per_million: f64,
    output_per_million: f64,
}

impl TokenPrices {
    fn for_model_class(model_class: &str) -> Option<Self> {
        match model_class {
            "anthropic_opus" => Some(Self {
                input_per_million: 15.0,
                cache_read_per_million: 1.5,
                cache_write_surcharge_per_million: 3.75,
                output_per_million: 75.0,
            }),
            "anthropic_sonnet" => Some(Self {
                input_per_million: 3.0,
                cache_read_per_million: 0.3,
                cache_write_surcharge_per_million: 0.75,
                output_per_million: 15.0,
            }),
            "anthropic_haiku" => Some(Self {
                input_per_million: 0.8,
                cache_read_per_million: 0.08,
                cache_write_surcharge_per_million: 0.2,
                output_per_million: 4.0,
            }),
            "openai_gpt5" => Some(Self {
                input_per_million: 1.25,
                cache_read_per_million: 0.125,
                cache_write_surcharge_per_million: 0.0,
                output_per_million: 10.0,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_sonnet_cost_from_token_usage() {
        let cost = estimate_cost_usd("anthropic_sonnet", TokenUsage {
            uncached_input_tokens: 1_000_000,
            cache_read_input_tokens: 1_000_000,
            cache_write_input_tokens: 1_000_000,
            output_tokens: 1_000_000,
        })
        .expect("known model class should estimate cost");

        assert!((cost - 19.05).abs() < 0.000000001);
    }

    #[test]
    fn does_not_estimate_unknown_or_empty_usage() {
        assert_eq!(
            estimate_cost_usd("other", TokenUsage {
                output_tokens: 1,
                ..Default::default()
            },),
            None
        );
        assert_eq!(estimate_cost_usd("anthropic_sonnet", TokenUsage::default()), None);
    }

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
