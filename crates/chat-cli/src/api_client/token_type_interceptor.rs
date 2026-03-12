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
