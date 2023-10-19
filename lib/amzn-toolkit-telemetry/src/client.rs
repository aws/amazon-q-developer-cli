// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[derive(Debug)]
pub(crate) struct Handle {
    pub(crate) conf: crate::Config,
    pub(crate) runtime_plugins: ::aws_smithy_runtime_api::client::runtime_plugin::RuntimePlugins,
}

/// Client for the service
///
/// Client for invoking operations on the service. Each operation on the service is a method on this
/// this struct. `.send()` MUST be invoked on the generated operations to dispatch the request to the service.
/// # Using the `Client`
///
/// A client has a function for every operation that can be performed by the service.
/// For example, the [`PostErrorReport`](crate::operation::post_error_report) operation has
/// a [`Client::post_error_report`], function which returns a builder for that operation.
/// The fluent builder ultimately has a `send()` function that returns an async future that
/// returns a result, as illustrated below:
///
/// ```rust,ignore
/// let result = client.post_error_report()
///     .aws_product("example")
///     .send()
///     .await;
/// ```
///
/// The underlying HTTP requests that get made by this can be modified with the `customize_operation`
/// function on the fluent builder. See the [`customize`](crate::client::customize) module for more
/// information.
#[derive(::std::clone::Clone, ::std::fmt::Debug)]
pub struct Client {
    handle: ::std::sync::Arc<Handle>,
}

impl Client {
    /// Creates a new client from the service [`Config`](crate::Config).
    ///
    /// # Panics
    ///
    /// This method will panic if the `conf` has retry or timeouts enabled without a `sleep_impl`.
    /// If you experience this panic, it can be fixed by setting the `sleep_impl`, or by disabling
    /// retries and timeouts.
    pub fn from_conf(conf: crate::Config) -> Self {
        let retry_config = conf
            .retry_config()
            .cloned()
            .unwrap_or_else(::aws_smithy_types::retry::RetryConfig::disabled);
        let timeout_config = conf
            .timeout_config()
            .cloned()
            .unwrap_or_else(::aws_smithy_types::timeout::TimeoutConfig::disabled);
        let sleep_impl = conf.sleep_impl();
        if (retry_config.has_retry() || timeout_config.has_timeouts()) && sleep_impl.is_none() {
            panic!(
                "An async sleep implementation is required for retries or timeouts to work. \
                                        Set the `sleep_impl` on the Config passed into this function to fix this panic."
            );
        }

        Self {
            handle: ::std::sync::Arc::new(Handle {
                conf: conf.clone(),
                runtime_plugins: crate::config::base_client_runtime_plugins(conf),
            }),
        }
    }

    /// Returns the client's configuration.
    pub fn config(&self) -> &crate::Config {
        &self.handle.conf
    }

    #[doc(hidden)]
    // TODO(enableNewSmithyRuntimeCleanup): Delete this function when cleaning up middleware
    // This is currently kept around so the tests still compile in both modes
    /// Creates a client with the given service configuration.
    pub fn with_config<C, M, R>(_client: ::aws_smithy_client::Client<C, M, R>, conf: crate::Config) -> Self {
        Self::from_conf(conf)
    }

    #[doc(hidden)]
    // TODO(enableNewSmithyRuntimeCleanup): Delete this function when cleaning up middleware
    // This is currently kept around so the tests still compile in both modes
    /// Returns the client's configuration.
    pub fn conf(&self) -> &crate::Config {
        &self.handle.conf
    }
}

impl Client {
    /// Creates a new client from an [SDK Config](::aws_types::sdk_config::SdkConfig).
    ///
    /// # Panics
    ///
    /// - This method will panic if the `sdk_config` is missing an async sleep implementation. If you experience this panic, set
    ///     the `sleep_impl` on the Config passed into this function to fix it.
    /// - This method will panic if the `sdk_config` is missing an HTTP connector. If you experience this panic, set the
    ///     `http_connector` on the Config passed into this function to fix it.
    pub fn new(sdk_config: &::aws_types::sdk_config::SdkConfig) -> Self {
        Self::from_conf(sdk_config.into())
    }
}

/// Operation customization and supporting types.
///
/// The underlying HTTP requests made during an operation can be customized
/// by calling the `customize()` method on the builder returned from a client
/// operation call. For example, this can be used to add an additional HTTP header:
///
/// ```ignore
/// # async fn wrapper() -> ::std::result::Result<(), amzn_toolkit_telemetry::Error> {
/// # let client: amzn_toolkit_telemetry::Client = unimplemented!();
/// use ::http::header::{HeaderName, HeaderValue};
///
/// let result = client.post_error_report()
///     .customize()
///     .await?
///     .mutate_request(|req| {
///         // Add `x-example-header` with value
///         req.headers_mut()
///             .insert(
///                 HeaderName::from_static("x-example-header"),
///                 HeaderValue::from_static("1"),
///             );
///     })
///     .send()
///     .await;
/// # }
/// ```
pub mod customize;

mod post_error_report;

mod post_feedback;

mod post_metrics;