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
use crate::aws_common::behavior_version;
use crate::database::Database;
use crate::database::settings::Setting;

// TODO(bskiser): confirm timeout is updated to an appropriate value?
const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(60 * 5);

pub fn timeout_config(database: &Database) -> TimeoutConfig {
    let timeout = database
        .settings
        .get_int(Setting::ApiTimeout)
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

async fn base_sdk_config(
    database: &Database,
    region: Region,
    credentials_provider: impl ProvideCredentials + 'static,
) -> SdkConfig {
    aws_config::defaults(behavior_version())
        .region(region)
        .credentials_provider(credentials_provider)
        .timeout_config(timeout_config(database))
        .retry_config(RetryConfig::adaptive())
        .load()
        .await
}

pub async fn bearer_sdk_config(database: &Database, endpoint: &Endpoint) -> SdkConfig {
    let credentials = Credentials::new("xxx", "xxx", None, None, "xxx");
    base_sdk_config(database, endpoint.region().clone(), credentials).await
}

pub async fn sigv4_sdk_config(database: &Database, endpoint: &Endpoint) -> Result<SdkConfig, ApiClientError> {
    let credentials_chain = CredentialsChain::new().await;

    if let Err(err) = credentials_chain.provide_credentials().await {
        return Err(ApiClientError::Credentials(err));
    };

    Ok(base_sdk_config(database, endpoint.region().clone(), credentials_chain).await)
}
