// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub use crate::operation::send_telemetry_event::_send_telemetry_event_input::SendTelemetryEventInputBuilder;
pub use crate::operation::send_telemetry_event::_send_telemetry_event_output::SendTelemetryEventOutputBuilder;

impl SendTelemetryEventInputBuilder {
    /// Sends a request with this input using the given client.
    pub async fn send_with(
        self,
        client: &crate::Client,
    ) -> ::std::result::Result<
        crate::operation::send_telemetry_event::SendTelemetryEventOutput,
        ::aws_smithy_http::result::SdkError<
            crate::operation::send_telemetry_event::SendTelemetryEventError,
            ::aws_smithy_runtime_api::client::orchestrator::HttpResponse,
        >,
    > {
        let mut fluent_builder = client.send_telemetry_event();
        fluent_builder.inner = self;
        fluent_builder.send().await
    }
}
/// Fluent builder constructing a request to `SendTelemetryEvent`.
///
/// API to record telemetry events.
#[derive(::std::clone::Clone, ::std::fmt::Debug)]
pub struct SendTelemetryEventFluentBuilder {
    handle: ::std::sync::Arc<crate::client::Handle>,
    inner: crate::operation::send_telemetry_event::builders::SendTelemetryEventInputBuilder,
    config_override: ::std::option::Option<crate::config::Builder>,
}
impl
    crate::client::customize::internal::CustomizableSend<
        crate::operation::send_telemetry_event::SendTelemetryEventOutput,
        crate::operation::send_telemetry_event::SendTelemetryEventError,
    > for SendTelemetryEventFluentBuilder
{
    fn send(
        self,
        config_override: crate::config::Builder,
    ) -> crate::client::customize::internal::BoxFuture<
        crate::client::customize::internal::SendResult<
            crate::operation::send_telemetry_event::SendTelemetryEventOutput,
            crate::operation::send_telemetry_event::SendTelemetryEventError,
        >,
    > {
        ::std::boxed::Box::pin(async move { self.config_override(config_override).send().await })
    }
}
impl SendTelemetryEventFluentBuilder {
    /// Creates a new `SendTelemetryEvent`.
    pub(crate) fn new(handle: ::std::sync::Arc<crate::client::Handle>) -> Self {
        Self {
            handle,
            inner: ::std::default::Default::default(),
            config_override: ::std::option::Option::None,
        }
    }

    /// Access the SendTelemetryEvent as a reference.
    pub fn as_input(&self) -> &crate::operation::send_telemetry_event::builders::SendTelemetryEventInputBuilder {
        &self.inner
    }

    /// Sends the request and returns the response.
    ///
    /// If an error occurs, an `SdkError` will be returned with additional details that
    /// can be matched against.
    ///
    /// By default, any retryable failures will be retried twice. Retry behavior
    /// is configurable with the [RetryConfig](aws_smithy_types::retry::RetryConfig), which can be
    /// set when configuring the client.
    pub async fn send(
        self,
    ) -> ::std::result::Result<
        crate::operation::send_telemetry_event::SendTelemetryEventOutput,
        ::aws_smithy_http::result::SdkError<
            crate::operation::send_telemetry_event::SendTelemetryEventError,
            ::aws_smithy_runtime_api::client::orchestrator::HttpResponse,
        >,
    > {
        let input = self
            .inner
            .build()
            .map_err(::aws_smithy_http::result::SdkError::construction_failure)?;
        let runtime_plugins = crate::operation::send_telemetry_event::SendTelemetryEvent::operation_runtime_plugins(
            self.handle.runtime_plugins.clone(),
            &self.handle.conf,
            self.config_override,
        );
        crate::operation::send_telemetry_event::SendTelemetryEvent::orchestrate(&runtime_plugins, input).await
    }

    /// Consumes this builder, creating a customizable operation that can be modified before being
    /// sent.
    // TODO(enableNewSmithyRuntimeCleanup): Remove `async` and `Result` once we switch to orchestrator
    pub async fn customize(
        self,
    ) -> ::std::result::Result<
        crate::client::customize::orchestrator::CustomizableOperation<
            crate::operation::send_telemetry_event::SendTelemetryEventOutput,
            crate::operation::send_telemetry_event::SendTelemetryEventError,
            Self,
        >,
        ::aws_smithy_http::result::SdkError<crate::operation::send_telemetry_event::SendTelemetryEventError>,
    > {
        ::std::result::Result::Ok(crate::client::customize::orchestrator::CustomizableOperation::new(self))
    }

    pub(crate) fn config_override(mut self, config_override: impl Into<crate::config::Builder>) -> Self {
        self.set_config_override(Some(config_override.into()));
        self
    }

    pub(crate) fn set_config_override(&mut self, config_override: Option<crate::config::Builder>) -> &mut Self {
        self.config_override = config_override;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn client_token(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.inner = self.inner.client_token(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_client_token(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.inner = self.inner.set_client_token(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_client_token(&self) -> &::std::option::Option<::std::string::String> {
        self.inner.get_client_token()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn telemetry_event(mut self, input: crate::types::TelemetryEvent) -> Self {
        self.inner = self.inner.telemetry_event(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_telemetry_event(mut self, input: ::std::option::Option<crate::types::TelemetryEvent>) -> Self {
        self.inner = self.inner.set_telemetry_event(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_telemetry_event(&self) -> &::std::option::Option<crate::types::TelemetryEvent> {
        self.inner.get_telemetry_event()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn opt_out_preference(mut self, input: crate::types::OptOutPreference) -> Self {
        self.inner = self.inner.opt_out_preference(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_opt_out_preference(mut self, input: ::std::option::Option<crate::types::OptOutPreference>) -> Self {
        self.inner = self.inner.set_opt_out_preference(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_opt_out_preference(&self) -> &::std::option::Option<crate::types::OptOutPreference> {
        self.inner.get_opt_out_preference()
    }
}