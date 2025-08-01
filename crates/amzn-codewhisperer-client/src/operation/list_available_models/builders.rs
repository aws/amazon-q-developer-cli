// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub use crate::operation::list_available_models::_list_available_models_input::ListAvailableModelsInputBuilder;
pub use crate::operation::list_available_models::_list_available_models_output::ListAvailableModelsOutputBuilder;

impl crate::operation::list_available_models::builders::ListAvailableModelsInputBuilder {
    /// Sends a request with this input using the given client.
    pub async fn send_with(
        self,
        client: &crate::Client,
    ) -> ::std::result::Result<
        crate::operation::list_available_models::ListAvailableModelsOutput,
        ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::list_available_models::ListAvailableModelsError,
            ::aws_smithy_runtime_api::client::orchestrator::HttpResponse,
        >,
    > {
        let mut fluent_builder = client.list_available_models();
        fluent_builder.inner = self;
        fluent_builder.send().await
    }
}
/// Fluent builder constructing a request to `ListAvailableModels`.
#[derive(::std::clone::Clone, ::std::fmt::Debug)]
pub struct ListAvailableModelsFluentBuilder {
    handle: ::std::sync::Arc<crate::client::Handle>,
    inner: crate::operation::list_available_models::builders::ListAvailableModelsInputBuilder,
    config_override: ::std::option::Option<crate::config::Builder>,
}
impl
    crate::client::customize::internal::CustomizableSend<
        crate::operation::list_available_models::ListAvailableModelsOutput,
        crate::operation::list_available_models::ListAvailableModelsError,
    > for ListAvailableModelsFluentBuilder
{
    fn send(
        self,
        config_override: crate::config::Builder,
    ) -> crate::client::customize::internal::BoxFuture<
        crate::client::customize::internal::SendResult<
            crate::operation::list_available_models::ListAvailableModelsOutput,
            crate::operation::list_available_models::ListAvailableModelsError,
        >,
    > {
        ::std::boxed::Box::pin(async move { self.config_override(config_override).send().await })
    }
}
impl ListAvailableModelsFluentBuilder {
    /// Creates a new `ListAvailableModelsFluentBuilder`.
    pub(crate) fn new(handle: ::std::sync::Arc<crate::client::Handle>) -> Self {
        Self {
            handle,
            inner: ::std::default::Default::default(),
            config_override: ::std::option::Option::None,
        }
    }

    /// Access the ListAvailableModels as a reference.
    pub fn as_input(&self) -> &crate::operation::list_available_models::builders::ListAvailableModelsInputBuilder {
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
        crate::operation::list_available_models::ListAvailableModelsOutput,
        ::aws_smithy_runtime_api::client::result::SdkError<
            crate::operation::list_available_models::ListAvailableModelsError,
            ::aws_smithy_runtime_api::client::orchestrator::HttpResponse,
        >,
    > {
        let input = self
            .inner
            .build()
            .map_err(::aws_smithy_runtime_api::client::result::SdkError::construction_failure)?;
        let runtime_plugins = crate::operation::list_available_models::ListAvailableModels::operation_runtime_plugins(
            self.handle.runtime_plugins.clone(),
            &self.handle.conf,
            self.config_override,
        );
        crate::operation::list_available_models::ListAvailableModels::orchestrate(&runtime_plugins, input).await
    }

    /// Consumes this builder, creating a customizable operation that can be modified before being
    /// sent.
    pub fn customize(
        self,
    ) -> crate::client::customize::CustomizableOperation<
        crate::operation::list_available_models::ListAvailableModelsOutput,
        crate::operation::list_available_models::ListAvailableModelsError,
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

    /// Create a paginator for this request
    ///
    /// Paginators are used by calling
    /// [`send().await`](crate::operation::list_available_models::paginator::ListAvailableModelsPaginator::send)
    /// which returns a
    /// [`PaginationStream`](aws_smithy_async::future::pagination_stream::PaginationStream).
    pub fn into_paginator(self) -> crate::operation::list_available_models::paginator::ListAvailableModelsPaginator {
        crate::operation::list_available_models::paginator::ListAvailableModelsPaginator::new(self.handle, self.inner)
    }

    /// The origin context for which to list available models
    pub fn origin(mut self, input: crate::types::Origin) -> Self {
        self.inner = self.inner.origin(input);
        self
    }

    /// The origin context for which to list available models
    pub fn set_origin(mut self, input: ::std::option::Option<crate::types::Origin>) -> Self {
        self.inner = self.inner.set_origin(input);
        self
    }

    /// The origin context for which to list available models
    pub fn get_origin(&self) -> &::std::option::Option<crate::types::Origin> {
        self.inner.get_origin()
    }

    /// Maximum number of models to return in a single response
    pub fn max_results(mut self, input: i32) -> Self {
        self.inner = self.inner.max_results(input);
        self
    }

    /// Maximum number of models to return in a single response
    pub fn set_max_results(mut self, input: ::std::option::Option<i32>) -> Self {
        self.inner = self.inner.set_max_results(input);
        self
    }

    /// Maximum number of models to return in a single response
    pub fn get_max_results(&self) -> &::std::option::Option<i32> {
        self.inner.get_max_results()
    }

    /// Token for retrieving the next page of results
    pub fn next_token(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.inner = self.inner.next_token(input.into());
        self
    }

    /// Token for retrieving the next page of results
    pub fn set_next_token(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.inner = self.inner.set_next_token(input);
        self
    }

    /// Token for retrieving the next page of results
    pub fn get_next_token(&self) -> &::std::option::Option<::std::string::String> {
        self.inner.get_next_token()
    }

    /// ARN of the profile to use for model filtering
    pub fn profile_arn(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.inner = self.inner.profile_arn(input.into());
        self
    }

    /// ARN of the profile to use for model filtering
    pub fn set_profile_arn(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.inner = self.inner.set_profile_arn(input);
        self
    }

    /// ARN of the profile to use for model filtering
    pub fn get_profile_arn(&self) -> &::std::option::Option<::std::string::String> {
        self.inner.get_profile_arn()
    }

    /// Provider of AI models
    pub fn model_provider(mut self, input: crate::types::ModelProvider) -> Self {
        self.inner = self.inner.model_provider(input);
        self
    }

    /// Provider of AI models
    pub fn set_model_provider(mut self, input: ::std::option::Option<crate::types::ModelProvider>) -> Self {
        self.inner = self.inner.set_model_provider(input);
        self
    }

    /// Provider of AI models
    pub fn get_model_provider(&self) -> &::std::option::Option<crate::types::ModelProvider> {
        self.inner.get_model_provider()
    }
}
