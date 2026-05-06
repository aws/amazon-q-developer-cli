use std::sync::Arc;
use std::sync::atomic::{
    AtomicU32,
    Ordering,
};
use std::time::Instant;

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::BeforeTransmitInterceptorContextRef;
use aws_smithy_runtime_api::client::retries::RequestAttempts;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::{
    ConfigBag,
    Storable,
    StoreReplace,
};
use parking_lot::Mutex;
use tracing::debug;

use crate::api_client::MAX_RETRY_DELAY_DURATION;

/// A warning message produced by the delay tracking interceptor when a retry is detected.
#[derive(Debug, Clone)]
pub struct RetryWarning {
    pub attempt: u32,
    pub max_attempts: u32,
    pub delay_secs: f64,
    pub message: String,
}

/// Shared buffer for retry warnings produced by the interceptor.
/// The interceptor pushes warnings here; callers drain after each `send_message`.
pub type RetryWarningBuffer = Arc<Mutex<Vec<RetryWarning>>>;

/// Shared counter tracking the max attempt number observed by the interceptor.
///
/// The SDK increments `RequestAttempts` before each attempt (starting at 1). The interceptor
/// records the max value seen, which equals the total number of attempts made for the request
/// once it completes. Always-on, regardless of whether the delay crossed the warning threshold.
pub type RequestAttemptsTracker = Arc<AtomicU32>;

#[derive(Debug, Clone)]
pub struct DelayTrackingInterceptor {
    warnings: RetryWarningBuffer,
    attempts: RequestAttemptsTracker,
    /// Max attempts configured on the SDK retry policy. Surfaced to the UI so the message
    /// can say "attempt N/M" accurately rather than hardcoding a number.
    max_attempts: u32,
}

impl DelayTrackingInterceptor {
    pub fn new(warnings: RetryWarningBuffer, attempts: RequestAttemptsTracker, max_attempts: u32) -> Self {
        Self {
            warnings,
            attempts,
            max_attempts,
        }
    }
}

impl Intercept for DelayTrackingInterceptor {
    fn name(&self) -> &'static str {
        "DelayTrackingInterceptor"
    }

    fn read_before_transmit(
        &self,
        _: &BeforeTransmitInterceptorContextRef<'_>,
        _: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let attempt_number = cfg.load::<RequestAttempts>().map_or(1, |attempts| attempts.attempts());

        // Always record the attempt count, regardless of whether this is a retry.
        // Use fetch_max so concurrent requests on the same client don't clobber each other.
        self.attempts.fetch_max(attempt_number, Ordering::Relaxed);

        // Targeted tracing for the streaming client retry plumbing. `target` set so it
        // can be isolated with `KIRO_LOG_LEVEL=kiro::retry=trace`.
        debug!(
            target: "kiro::retry",
            attempt = attempt_number,
            max_attempts = self.max_attempts,
            "DelayTrackingInterceptor.read_before_transmit"
        );

        let now = Instant::now();

        // Emit a warning for *every* retry (attempt ≥ 2), not just slow ones. The inline
        // TUI surface is cheap to update and users want to know as soon as the SDK is
        // doing work on their behalf. The reported delay is the measured gap since the
        // last attempt, clamped to the configured max backoff.
        if attempt_number >= 2 {
            let delay = cfg.load::<LastAttemptTime>().map_or(MAX_RETRY_DELAY_DURATION, |t| {
                now.duration_since(t.0).min(MAX_RETRY_DELAY_DURATION)
            });

            // Round up to the nearest whole second so the user sees "Retrying in 1s"
            // rather than "Retrying in 0.0s" for sub-second backoffs (including the
            // first retry under adaptive retry, which often has ~0ms delay).
            let display_secs = delay.as_secs_f64().ceil().max(1.0);
            let msg = format!(
                "Retrying in {}s (attempt {}/{})",
                display_secs as u64, attempt_number, self.max_attempts,
            );
            debug!(
                target: "kiro::retry",
                attempt = attempt_number,
                delay_ms = delay.as_millis() as u64,
                display_secs = display_secs,
                "pushing retry warning"
            );
            self.warnings.lock().push(RetryWarning {
                attempt: attempt_number,
                max_attempts: self.max_attempts,
                delay_secs: display_secs,
                message: msg,
            });
        }

        cfg.interceptor_state().store_put(LastAttemptTime(Instant::now()));
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct LastAttemptTime(Instant);

impl Storable for LastAttemptTime {
    type Storer = StoreReplace<Self>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{
        Duration,
        Instant,
    };

    use aws_smithy_runtime_api::client::interceptors::context::{
        Input,
        InterceptorContext,
    };
    use aws_smithy_runtime_api::client::runtime_components::RuntimeComponentsBuilder;
    use aws_smithy_types::config_bag::ConfigBag;
    use parking_lot::Mutex;

    use super::*;

    fn make_interceptor() -> (DelayTrackingInterceptor, RetryWarningBuffer, RequestAttemptsTracker) {
        let buf: RetryWarningBuffer = Arc::new(Mutex::new(Vec::new()));
        let attempts: RequestAttemptsTracker = Arc::new(AtomicU32::new(0));
        // Tests use the streaming client's max (6) for assertions — the exact value doesn't
        // matter, we're just verifying it flows through.
        let interceptor = DelayTrackingInterceptor::new(buf.clone(), attempts.clone(), 6);
        (interceptor, buf, attempts)
    }

    fn call_interceptor(interceptor: &DelayTrackingInterceptor, cfg: &mut ConfigBag) {
        let rc = RuntimeComponentsBuilder::for_tests().build().unwrap();
        let mut context = InterceptorContext::new(Input::erase(()));
        context.set_request(aws_smithy_runtime_api::http::Request::empty());
        let ctx =
            aws_smithy_runtime_api::client::interceptors::context::BeforeTransmitInterceptorContextRef::from(&context);
        interceptor.read_before_transmit(&ctx, &rc, cfg).unwrap();
    }

    /// Seed the config bag with a `RequestAttempts` value so the interceptor treats the
    /// next call as attempt N.
    fn set_attempt(cfg: &mut ConfigBag, n: u32) {
        cfg.interceptor_state().store_put(RequestAttempts::new(n));
    }

    #[test]
    fn first_attempt_does_not_emit_warning() {
        // Attempt 1 is not a retry — no warning should ever be emitted. But the attempt
        // counter must still tick to 1 so telemetry can distinguish "never ran" from
        // "ran once and failed".
        let (interceptor, buf, attempts) = make_interceptor();
        let mut cfg = ConfigBag::base();

        call_interceptor(&interceptor, &mut cfg);

        assert!(buf.lock().is_empty(), "no warning on first attempt");
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn every_retry_emits_a_warning_even_with_zero_delay() {
        // Key behaviour: under adaptive retry, the first retry often has ~0ms backoff.
        // We still want the user to see it so they know something is happening.
        let (interceptor, buf, _) = make_interceptor();
        let mut cfg = ConfigBag::base();

        // Simulate attempt #2 with a previous attempt that happened "just now" (~0 delay).
        set_attempt(&mut cfg, 2);
        cfg.interceptor_state().store_put(LastAttemptTime(Instant::now()));

        call_interceptor(&interceptor, &mut cfg);

        let warnings = buf.lock();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].attempt, 2);
        assert_eq!(warnings[0].max_attempts, 6);
        // Sub-second delays are floored at 1s to avoid confusing "Retrying in 0s".
        assert_eq!(warnings[0].delay_secs, 1.0);
        assert!(warnings[0].message.contains("attempt 2/6"));
        assert!(warnings[0].message.contains("Retrying in 1s"));
    }

    #[test]
    fn measured_delay_is_reflected_in_message() {
        // A retry with a visible backoff should show that backoff in the message.
        let (interceptor, buf, _) = make_interceptor();
        let mut cfg = ConfigBag::base();

        set_attempt(&mut cfg, 3);
        cfg.interceptor_state()
            .store_put(LastAttemptTime(Instant::now() - Duration::from_millis(2_400)));

        call_interceptor(&interceptor, &mut cfg);

        let warnings = buf.lock();
        assert_eq!(warnings.len(), 1);
        // 2.4s rounds up to 3s for display (avoids truncation down to "2s").
        assert_eq!(warnings[0].delay_secs, 3.0);
        assert!(warnings[0].message.contains("Retrying in 3s"));
        assert!(warnings[0].message.contains("attempt 3/6"));
    }

    #[test]
    fn delay_is_clamped_to_max_backoff() {
        // If the SDK were to wait longer than MAX_RETRY_DELAY_DURATION for some reason,
        // we still display the configured max rather than the real gap so the user
        // doesn't see a nonsensical number from clock skew / suspend / etc.
        let (interceptor, buf, _) = make_interceptor();
        let mut cfg = ConfigBag::base();

        set_attempt(&mut cfg, 2);
        cfg.interceptor_state()
            .store_put(LastAttemptTime(Instant::now() - Duration::from_secs(30)));

        call_interceptor(&interceptor, &mut cfg);

        let warnings = buf.lock();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].delay_secs, MAX_RETRY_DELAY_DURATION.as_secs_f64());
    }

    #[test]
    fn attempts_use_fetch_max_not_overwrite() {
        let (interceptor, _, attempts) = make_interceptor();
        // Seed a higher value — a subsequent call with attempt=1 must not lower it.
        attempts.store(3, Ordering::Relaxed);

        let mut cfg = ConfigBag::base();
        call_interceptor(&interceptor, &mut cfg);

        assert_eq!(attempts.load(Ordering::Relaxed), 3);
    }
}
