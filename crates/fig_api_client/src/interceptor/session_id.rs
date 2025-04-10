use std::sync::{
    Arc,
    Mutex,
};

use aws_smithy_runtime_api::client::interceptors::Intercept;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_types::config_bag::ConfigBag;

use crate::consts::X_AMZN_SESSIONID_HEADER;

#[derive(Debug, Clone)]
pub struct SessionIdInterceptor {
    inner: Arc<Mutex<Option<String>>>,
}

impl SessionIdInterceptor {
    pub const fn new(inner: Arc<Mutex<Option<String>>>) -> Self {
        Self { inner }
    }
}

impl Intercept for SessionIdInterceptor {
    fn name(&self) -> &'static str {
        "SessionIdInterceptor"
    }

    fn read_after_deserialization(
        &self,
        context: &amzn_codewhisperer_client::config::interceptors::AfterDeserializationInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        _cfg: &mut ConfigBag,
    ) -> Result<(), amzn_codewhisperer_client::error::BoxError> {
        *self
            .inner
            .lock()
            .expect("Failed to write to SessionIdInterceptor mutex") = context
            .response()
            .headers()
            .get(X_AMZN_SESSIONID_HEADER)
            .map(Into::into);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use amzn_consolas_client::config::RuntimeComponentsBuilder;
    use amzn_consolas_client::config::interceptors::{
        AfterDeserializationInterceptorContextRef,
        InterceptorContext,
    };
    use aws_smithy_runtime_api::client::interceptors::context::Input;
    use aws_smithy_runtime_api::http::StatusCode;
    use aws_smithy_types::body::SdkBody;

    use super::*;

    #[test]
    fn test_opt_out_interceptor() {
        let rc = RuntimeComponentsBuilder::for_tests().build().unwrap();
        let mut cfg = ConfigBag::base();

        let mut context = InterceptorContext::new(Input::erase(()));
        let mut response =
            aws_smithy_runtime_api::http::Response::new(StatusCode::try_from(200).unwrap(), SdkBody::empty());
        response
            .headers_mut()
            .insert(X_AMZN_SESSIONID_HEADER, "test-session-id");
        context.set_response(response);
        let context = AfterDeserializationInterceptorContextRef::from(&context);

        let session_id_lock = Arc::new(Mutex::new(None));
        let interceptor = SessionIdInterceptor::new(session_id_lock.clone());
        println!("Interceptor: {}", interceptor.name());

        interceptor
            .read_after_deserialization(&context, &rc, &mut cfg)
            .expect("success");
        assert_eq!(*session_id_lock.lock().unwrap(), Some("test-session-id".to_string()));
    }
}
