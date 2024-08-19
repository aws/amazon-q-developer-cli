// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub use crate::operation::start_conversation::_start_conversation_input::StartConversationInputBuilder;
pub use crate::operation::start_conversation::_start_conversation_output::StartConversationOutputBuilder;

impl crate::operation::start_conversation::builders::StartConversationInputBuilder {
    /// Sends a request with this input using the given client.
    pub async fn send_with(
        self,
        client: &crate::Client,
    ) -> ::std::result::Result<
        crate::operation::start_conversation::StartConversationOutput,
        ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::start_conversation::StartConversationError,
            ::aws_smithy_runtime_api::client::orchestrator::HttpResponse,
        >,
    > {
        let mut fluent_builder = client.start_conversation();
        fluent_builder.inner = self;
        fluent_builder.send().await
    }
}
/// Fluent builder constructing a request to `StartConversation`.
#[derive(::std::clone::Clone, ::std::fmt::Debug)]
pub struct StartConversationFluentBuilder {
    handle: ::std::sync::Arc<crate::client::Handle>,
    inner: crate::operation::start_conversation::builders::StartConversationInputBuilder,
    config_override: ::std::option::Option<crate::config::Builder>,
}
impl
    crate::client::customize::internal::CustomizableSend<
        crate::operation::start_conversation::StartConversationOutput,
        crate::operation::start_conversation::StartConversationError,
    > for StartConversationFluentBuilder
{
    fn send(
        self,
        config_override: crate::config::Builder,
    ) -> crate::client::customize::internal::BoxFuture<
        crate::client::customize::internal::SendResult<
            crate::operation::start_conversation::StartConversationOutput,
            crate::operation::start_conversation::StartConversationError,
        >,
    > {
        ::std::boxed::Box::pin(async move { self.config_override(config_override).send().await })
    }
}
impl StartConversationFluentBuilder {
    /// Creates a new `StartConversationFluentBuilder`.
    pub(crate) fn new(handle: ::std::sync::Arc<crate::client::Handle>) -> Self {
        Self {
            handle,
            inner: ::std::default::Default::default(),
            config_override: ::std::option::Option::None,
        }
    }

    /// Access the StartConversation as a reference.
    pub fn as_input(&self) -> &crate::operation::start_conversation::builders::StartConversationInputBuilder {
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
        crate::operation::start_conversation::StartConversationOutput,
        ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::start_conversation::StartConversationError,
            ::aws_smithy_runtime_api::client::orchestrator::HttpResponse,
        >,
    > {
        let input = self
            .inner
            .build()
            .map_err(::aws_smithy_runtime_api::client::result::SdkError::construction_failure)?;
        let runtime_plugins = crate::operation::start_conversation::StartConversation::operation_runtime_plugins(
            self.handle.runtime_plugins.clone(),
            &self.handle.conf,
            self.config_override,
        );
        crate::operation::start_conversation::StartConversation::orchestrate(&runtime_plugins, input).await
    }

    /// Consumes this builder, creating a customizable operation that can be modified before being
    /// sent.
    pub fn customize(
        self,
    ) -> crate::client::customize::CustomizableOperation<
        crate::operation::start_conversation::StartConversationOutput,
        crate::operation::start_conversation::StartConversationError,
        Self,
    > {
        crate::client::customize::CustomizableOperation::new(self)
    }

    pub(crate) fn config_override(
        mut self,
        config_override: impl ::std::convert::Into<crate::config::Builder>,
    ) -> Self {
        self.set_config_override(::std::option::Option::Some(config_override.into()));
        self
    }

    pub(crate) fn set_config_override(
        &mut self,
        config_override: ::std::option::Option<crate::config::Builder>,
    ) -> &mut Self {
        self.config_override = config_override;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn origin(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.inner = self.inner.origin(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_origin(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.inner = self.inner.set_origin(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_origin(&self) -> &::std::option::Option<::std::string::String> {
        self.inner.get_origin()
    }

    /// Enum to represent the origin application conversing with Sidekick.
    pub fn source(mut self, input: crate::types::Origin) -> Self {
        self.inner = self.inner.source(input);
        self
    }

    /// Enum to represent the origin application conversing with Sidekick.
    pub fn set_source(mut self, input: ::std::option::Option<crate::types::Origin>) -> Self {
        self.inner = self.inner.set_source(input);
        self
    }

    /// Enum to represent the origin application conversing with Sidekick.
    pub fn get_source(&self) -> &::std::option::Option<crate::types::Origin> {
        self.inner.get_source()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn dry_run(mut self, input: bool) -> Self {
        self.inner = self.inner.dry_run(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_dry_run(mut self, input: ::std::option::Option<bool>) -> Self {
        self.inner = self.inner.set_dry_run(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_dry_run(&self) -> &::std::option::Option<bool> {
        self.inner.get_dry_run()
    }
}