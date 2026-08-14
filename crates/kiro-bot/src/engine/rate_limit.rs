//! Fleet-wide per-user prompt admission.

use std::time::Duration;

use anyhow::ensure;

use crate::config::RateLimitConfig;
use crate::engine::coordinator::{
    Coordinator,
    RateLimitOutcome,
};

pub(crate) const RECORD_TTL_GRACE_SECS: i64 = 24 * 60 * 60;
pub(crate) const MAX_WINDOW_SECS: u64 = (i64::MAX - RECORD_TTL_GRACE_SECS) as u64;

pub fn validate(config: &RateLimitConfig) -> anyhow::Result<()> {
    ensure!(
        config.max_prompts > 0,
        "rate_limit.max_prompts must be greater than zero"
    );
    ensure!(
        config.window_secs > 0,
        "rate_limit.window_secs must be greater than zero"
    );
    ensure!(
        config.window_secs <= MAX_WINDOW_SECS,
        "rate_limit.window_secs must be at most {MAX_WINDOW_SECS}"
    );
    Ok(())
}

pub async fn check(
    coordinator: &dyn Coordinator,
    config: &RateLimitConfig,
    user_id: &str,
) -> anyhow::Result<RateLimitOutcome> {
    validate(config)?;
    if user_id.is_empty() {
        return Ok(RateLimitOutcome::Allowed);
    }
    coordinator
        .admit_prompt(user_id, config.max_prompts, Duration::from_secs(config.window_secs))
        .await
}

pub fn notice(wait: Duration) -> String {
    let seconds = (wait.as_secs() + u64::from(wait.subsec_nanos() > 0)).max(1);
    format!(
        "You're sending requests too quickly. Please try again in about {} second{}.",
        seconds,
        if seconds == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_limits() {
        assert!(
            validate(&RateLimitConfig {
                max_prompts: 0,
                window_secs: 60,
            })
            .is_err()
        );
        assert!(
            validate(&RateLimitConfig {
                max_prompts: 1,
                window_secs: 0,
            })
            .is_err()
        );
    }

    #[test]
    fn accepts_largest_i64_safe_window() {
        validate(&RateLimitConfig {
            max_prompts: u32::MAX,
            window_secs: MAX_WINDOW_SECS,
        })
        .unwrap();
    }

    #[test]
    fn rejects_window_that_would_overflow_record_ttl() {
        let error = validate(&RateLimitConfig {
            max_prompts: 1,
            window_secs: MAX_WINDOW_SECS + 1,
        })
        .unwrap_err();
        assert!(error.to_string().contains("must be at most"));
    }

    #[test]
    fn notices_round_wait_up_for_users() {
        assert_eq!(
            notice(Duration::from_millis(1_001)),
            "You're sending requests too quickly. Please try again in about 2 seconds."
        );
        assert_eq!(
            notice(Duration::ZERO),
            "You're sending requests too quickly. Please try again in about 1 second."
        );
    }
}
