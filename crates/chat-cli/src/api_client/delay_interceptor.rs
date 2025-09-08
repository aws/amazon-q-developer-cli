use std::time::{
    Duration,
    Instant,
};

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
use crossterm::style::Color;
use crossterm::{
    execute,
    style,
};

use crate::api_client::MAX_RETRY_DELAY_DURATION;

#[derive(Debug, Clone)]
pub struct DelayTrackingInterceptor {
    minor_delay_threshold: Duration,
    major_delay_threshold: Duration,
}

impl DelayTrackingInterceptor {
    pub fn new() -> Self {
        Self {
            minor_delay_threshold: Duration::from_secs(2),
            major_delay_threshold: Duration::from_secs(5),
        }
    }

    fn print_warning(message: String) {
        let mut stderr = std::io::stderr();
        let _ = execute!(
            stderr,
            style::SetForegroundColor(Color::Yellow),
            style::Print("\nWARNING: "),
            style::SetForegroundColor(Color::Reset),
            style::Print(message),
            style::Print("\n")
        );
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

        let now = Instant::now();

        if let Some(last_attempt_time) = cfg.load::<LastAttemptTime>() {
            let delay = now.duration_since(last_attempt_time.0).min(MAX_RETRY_DELAY_DURATION);

            if delay >= self.major_delay_threshold {
                Self::print_warning(format!(
                    "Retry #{}, retrying within {:.1}s..",
                    attempt_number,
                    MAX_RETRY_DELAY_DURATION.as_secs_f64()
                ));
            } else if delay >= self.minor_delay_threshold {
                Self::print_warning(format!("Retry #{}, retrying within 5s..", attempt_number,));
            }
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
