// use aws_smithy_runtime_api::box_error::BoxError;
// use aws_smithy_runtime_api::client::interceptors::Intercept;
// use aws_smithy_runtime_api::client::interceptors::context::BeforeTransmitInterceptorContextMut;
// use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
// use aws_smithy_types::config_bag::ConfigBag;
//
// use crate::api_client::X_AMZN_CODEWHISPERER_OPT_OUT_HEADER;
// use crate::database::Database;
// use crate::database::settings::Setting;
//
// fn is_codewhisperer_content_optout(database: &Database) -> bool {
//     !database
//         .settings
//         .get_bool(Setting::ShareCodeWhispererContent)
//         .unwrap_or(true)
// }
//
// #[derive(Debug, Clone)]
// pub struct OptOutInterceptor {
//     is_codewhisperer_content_optout: bool,
//     override_value: Option<bool>,
//     _inner: (),
// }
//
// impl OptOutInterceptor {
//     pub fn new(database: &Database) -> Self {
//         Self {
//             is_codewhisperer_content_optout: is_codewhisperer_content_optout(database),
//             override_value: None,
//             _inner: (),
//         }
//     }
// }
//
// impl Intercept for OptOutInterceptor {
//     fn name(&self) -> &'static str {
//         "OptOutInterceptor"
//     }
//
//     fn modify_before_signing(
//         &self,
//         context: &mut BeforeTransmitInterceptorContextMut<'_>,
//         _runtime_components: &RuntimeComponents,
//         _cfg: &mut ConfigBag,
//     ) -> Result<(), BoxError> {
//         let opt_out = self.override_value.unwrap_or(self.is_codewhisperer_content_optout);
//         context
//             .request_mut()
//             .headers_mut()
//             .insert(X_AMZN_CODEWHISPERER_OPT_OUT_HEADER, opt_out.to_string());
//         Ok(())
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use amzn_consolas_client::config::RuntimeComponentsBuilder;
//     use amzn_consolas_client::config::interceptors::InterceptorContext;
//     use aws_smithy_runtime_api::client::interceptors::context::Input;
//
//     use super::*;
//
//     #[tokio::test]
//     async fn test_opt_out_interceptor() {
//         let rc = RuntimeComponentsBuilder::for_tests().build().unwrap();
//         let mut cfg = ConfigBag::base();
//
//         let mut context = InterceptorContext::new(Input::erase(()));
//         context.set_request(aws_smithy_runtime_api::http::Request::empty());
//         let mut context = BeforeTransmitInterceptorContextMut::from(&mut context);
//
//         let database = Database::new().await.unwrap();
//         let mut interceptor = OptOutInterceptor::new(&database);
//         println!("Interceptor: {}", interceptor.name());
//
//         interceptor
//             .modify_before_signing(&mut context, &rc, &mut cfg)
//             .expect("success");
//
//         interceptor.override_value = Some(false);
//         interceptor
//             .modify_before_signing(&mut context, &rc, &mut cfg)
//             .expect("success");
//         let val = context.request().headers().get(X_AMZN_CODEWHISPERER_OPT_OUT_HEADER);
//         assert_eq!(val, Some("false"));
//
//         interceptor.override_value = Some(true);
//         interceptor
//             .modify_before_signing(&mut context, &rc, &mut cfg)
//             .expect("success");
//         let val = context.request().headers().get(X_AMZN_CODEWHISPERER_OPT_OUT_HEADER);
//         assert_eq!(val, Some("true"));
//     }
// }
