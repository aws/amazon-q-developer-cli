//! Interceptor that stamps `x-kiro-attempt: N;max=M` on each request attempt so the
//! backend can distinguish absorbed retries from final-attempt (customer-facing) faults.

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::BeforeTransmitInterceptorContextMut;
use aws_smithy_runtime_api::client::retries::RequestAttempts;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::ConfigBag;

pub const X_KIRO_ATTEMPT_HEADER: &str = "x-kiro-attempt";

#[derive(Debug, Clone)]
pub struct AttemptHeaderInterceptor {
    max_attempts: u32,
}

impl AttemptHeaderInterceptor {
    pub fn new(max_attempts: u32) -> Self {
        Self { max_attempts }
    }
}

impl Intercept for AttemptHeaderInterceptor {
    fn name(&self) -> &'static str {
        "AttemptHeaderInterceptor"
    }

    fn modify_before_signing(
        &self,
        context: &mut BeforeTransmitInterceptorContextMut<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        // The orchestrator stores the 1-based attempt counter before attempt hooks run;
        // absent (e.g. in unit tests) means first attempt.
        let attempt = cfg.load::<RequestAttempts>().map_or(1, |a| a.attempts());
        context
            .request_mut()
            .headers_mut()
            .insert(X_KIRO_ATTEMPT_HEADER, format!("{};max={}", attempt, self.max_attempts));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aws_smithy_runtime_api::client::interceptors::context::{
        Input,
        InterceptorContext,
    };
    use aws_smithy_runtime_api::client::runtime_components::RuntimeComponentsBuilder;

    use super::*;

    fn header_for(attempt: Option<u32>, max: u32) -> String {
        let interceptor = AttemptHeaderInterceptor::new(max);
        let rc = RuntimeComponentsBuilder::for_tests().build().unwrap();
        let mut cfg = ConfigBag::base();
        if let Some(n) = attempt {
            cfg.interceptor_state().store_put(RequestAttempts::new(n));
        }
        let mut context = InterceptorContext::new(Input::erase(()));
        context.set_request(aws_smithy_runtime_api::http::Request::empty());
        let mut ctx = BeforeTransmitInterceptorContextMut::from(&mut context);
        interceptor.modify_before_signing(&mut ctx, &rc, &mut cfg).unwrap();
        ctx.request().headers().get(X_KIRO_ATTEMPT_HEADER).unwrap().to_string()
    }

    #[test]
    fn first_attempt_defaults_to_one_when_attempts_missing() {
        assert_eq!(header_for(None, 3), "1;max=3");
    }

    #[test]
    fn attempt_number_flows_from_config_bag() {
        assert_eq!(header_for(Some(1), 3), "1;max=3");
        assert_eq!(header_for(Some(2), 3), "2;max=3");
        assert_eq!(header_for(Some(3), 3), "3;max=3");
    }

    #[test]
    fn restamping_replaces_previous_value() {
        // Simulates the retry loop: the same request must carry only the latest value.
        let interceptor = AttemptHeaderInterceptor::new(3);
        let rc = RuntimeComponentsBuilder::for_tests().build().unwrap();
        let mut context = InterceptorContext::new(Input::erase(()));
        context.set_request(aws_smithy_runtime_api::http::Request::empty());

        for n in 1..=3u32 {
            let mut cfg = ConfigBag::base();
            cfg.interceptor_state().store_put(RequestAttempts::new(n));
            let mut ctx = BeforeTransmitInterceptorContextMut::from(&mut context);
            interceptor.modify_before_signing(&mut ctx, &rc, &mut cfg).unwrap();
        }
        let ctx = BeforeTransmitInterceptorContextMut::from(&mut context);
        assert_eq!(ctx.request().headers().get(X_KIRO_ATTEMPT_HEADER), Some("3;max=3"));
    }
}
