//! Interceptor to add TokenType header for External IdP authentication

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::BeforeTransmitInterceptorContextMut;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::ConfigBag;

const TOKEN_TYPE_HEADER: &str = "TokenType";
const EXTERNAL_IDP_VALUE: &str = "EXTERNAL_IDP";

#[derive(Debug, Clone)]
pub struct TokenTypeInterceptor {
    is_external_idp: bool,
}

impl TokenTypeInterceptor {
    pub fn new(is_external_idp: bool) -> Self {
        Self { is_external_idp }
    }
}

impl Intercept for TokenTypeInterceptor {
    fn name(&self) -> &'static str {
        "TokenTypeInterceptor"
    }

    fn modify_before_signing(
        &self,
        context: &mut BeforeTransmitInterceptorContextMut<'_>,
        _runtime_components: &RuntimeComponents,
        _cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        if self.is_external_idp {
            context
                .request_mut()
                .headers_mut()
                .insert(TOKEN_TYPE_HEADER, EXTERNAL_IDP_VALUE);
        }
        Ok(())
    }
}
