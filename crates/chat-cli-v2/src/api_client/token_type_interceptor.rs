//! Interceptor to add TokenType header for External IdP and API key authentication

use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::interceptors::context::BeforeTransmitInterceptorContextMut;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::ConfigBag;

const TOKEN_TYPE_HEADER: &str = "TokenType";
const EXTERNAL_IDP_VALUE: &str = "EXTERNAL_IDP";
const API_KEY_VALUE: &str = "API_KEY";

/// Authentication mode for the TokenType header.
#[derive(Debug, Clone)]
pub enum AuthMode {
    /// No TokenType header needed (Builder ID / Social)
    Normal,
    /// External IdP authentication
    ExternalIdp,
    /// API key authentication via KIRO_API_KEY env var
    ApiKey,
}

#[derive(Debug, Clone)]
pub struct TokenTypeInterceptor {
    auth_mode: AuthMode,
}

impl TokenTypeInterceptor {
    pub fn new(auth_mode: AuthMode) -> Self {
        Self { auth_mode }
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
        match &self.auth_mode {
            AuthMode::ExternalIdp => {
                context
                    .request_mut()
                    .headers_mut()
                    .insert(TOKEN_TYPE_HEADER, EXTERNAL_IDP_VALUE);
            },
            AuthMode::ApiKey => {
                context
                    .request_mut()
                    .headers_mut()
                    .insert(TOKEN_TYPE_HEADER, API_KEY_VALUE);
            },
            AuthMode::Normal => {},
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aws_smithy_runtime_api::client::interceptors::context::{
        BeforeTransmitInterceptorContextMut,
        Input,
        InterceptorContext,
    };
    use aws_smithy_runtime_api::client::runtime_components::RuntimeComponentsBuilder;
    use aws_smithy_types::config_bag::ConfigBag;

    use super::*;

    fn make_context_and_call(auth_mode: AuthMode) -> String {
        let interceptor = TokenTypeInterceptor::new(auth_mode);
        let rc = RuntimeComponentsBuilder::for_tests().build().unwrap();
        let mut cfg = ConfigBag::base();
        let mut context = InterceptorContext::new(Input::erase(()));
        context.set_request(aws_smithy_runtime_api::http::Request::empty());
        let mut ctx = BeforeTransmitInterceptorContextMut::from(&mut context);
        interceptor.modify_before_signing(&mut ctx, &rc, &mut cfg).unwrap();
        ctx.request().headers().get(TOKEN_TYPE_HEADER).unwrap_or("").to_string()
    }

    #[test]
    fn test_external_idp_sets_header() {
        assert_eq!(make_context_and_call(AuthMode::ExternalIdp), EXTERNAL_IDP_VALUE);
    }

    #[test]
    fn test_api_key_sets_header() {
        assert_eq!(make_context_and_call(AuthMode::ApiKey), API_KEY_VALUE);
    }

    #[test]
    fn test_normal_does_not_set_header() {
        assert_eq!(make_context_and_call(AuthMode::Normal), "");
    }
}
