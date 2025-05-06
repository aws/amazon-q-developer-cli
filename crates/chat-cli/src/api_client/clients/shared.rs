use std::time::Duration;

use aws_config::Region;
use aws_config::retry::RetryConfig;
use aws_config::timeout::TimeoutConfig;
use aws_credential_types::Credentials;
use aws_credential_types::provider::ProvideCredentials;
use aws_types::SdkConfig;
use aws_types::sdk_config::StalledStreamProtectionConfig;

use crate::api_client::credentials::CredentialsChain;
use crate::api_client::{
    ApiClientError,
    Endpoint,
};
use crate::fig_aws_common::behavior_version;

// TODO(bskiser): confirm timeout is updated to an appropriate value?
const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(60 * 5);

pub(crate) fn timeout_config() -> TimeoutConfig {
    let timeout = crate::fig_settings::settings::get_int("api.timeout")
        .ok()
        .flatten()
        .and_then(|i| i.try_into().ok())
        .map_or(DEFAULT_TIMEOUT_DURATION, Duration::from_millis);

    TimeoutConfig::builder()
        .read_timeout(timeout)
        .operation_timeout(timeout)
        .operation_attempt_timeout(timeout)
        .connect_timeout(timeout)
        .build()
}

pub(crate) fn stalled_stream_protection_config() -> StalledStreamProtectionConfig {
    StalledStreamProtectionConfig::enabled()
        .grace_period(Duration::from_secs(60 * 5))
        .build()
}

async fn base_sdk_config(region: Region, credentials_provider: impl ProvideCredentials + 'static) -> SdkConfig {
    aws_config::defaults(behavior_version())
        .region(region)
        .credentials_provider(credentials_provider)
        .timeout_config(timeout_config())
        .retry_config(RetryConfig::adaptive())
        .load()
        .await
}

pub(crate) async fn bearer_sdk_config(endpoint: &Endpoint) -> SdkConfig {
    let credentials = Credentials::new("xxx", "xxx", None, None, "xxx");
    base_sdk_config(endpoint.region().clone(), credentials).await
}

pub(crate) async fn sigv4_sdk_config(endpoint: &Endpoint) -> Result<SdkConfig, ApiClientError> {
    let credentials_chain = CredentialsChain::new().await;

    if let Err(err) = credentials_chain.provide_credentials().await {
        return Err(ApiClientError::Credentials(err));
    };

    Ok(base_sdk_config(endpoint.region().clone(), credentials_chain).await)
}
