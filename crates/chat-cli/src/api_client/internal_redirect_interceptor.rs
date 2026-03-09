//! Interceptor to add redirect-for-internal header for Amazon-internal users.
//!
//! The RTS ALB uses this header to route internal users to KRS instead of the
//! default backend.

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::BeforeTransmitInterceptorContextMut;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::ConfigBag;
use tracing::debug;

const REDIRECT_FOR_INTERNAL_HEADER: &str = "redirect-for-internal";

#[derive(Debug, Clone)]
pub struct InternalRedirectInterceptor {
    is_internal: bool,
}

impl InternalRedirectInterceptor {
    pub fn new(is_internal: bool) -> Self {
        Self { is_internal }
    }
}

impl Intercept for InternalRedirectInterceptor {
    fn name(&self) -> &'static str {
        "InternalRedirectInterceptor"
    }

    fn modify_before_signing(
        &self,
        context: &mut BeforeTransmitInterceptorContextMut<'_>,
        _runtime_components: &RuntimeComponents,
        _cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        if self.is_internal {
            debug!("adding redirect-for-internal header (mwinit detected)");
            context
                .request_mut()
                .headers_mut()
                .insert(REDIRECT_FOR_INTERNAL_HEADER, "true");
        }
        Ok(())
    }
}
