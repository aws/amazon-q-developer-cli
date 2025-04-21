use amzn_toolkit_telemetry::config::BehaviorVersion;
use aws_credential_types::provider::error::CredentialsError;
use aws_credential_types::{
    Credentials,
    provider,
};
use aws_sdk_cognitoidentity::primitives::{
    DateTime,
    DateTimeFormat,
};
use fig_aws_common::app_name;

use crate::TelemetryStage;

const CREDENTIALS_KEY: &str = "telemetry-cognito-credentials";

const DATE_TIME_FORMAT: DateTimeFormat = DateTimeFormat::DateTime;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct CredentialsJson {
    pub access_key_id: Option<String>,
    pub secret_key: Option<String>,
    pub session_token: Option<String>,
    pub expiration: Option<String>,
}

pub(crate) async fn get_cognito_credentials_send(
    telemetry_stage: &TelemetryStage,
) -> Result<Credentials, CredentialsError> {
    let conf = aws_sdk_cognitoidentity::Config::builder()
        .behavior_version(BehaviorVersion::v2025_01_17())
        .region(telemetry_stage.region.clone())
        .app_name(app_name())
        .build();
    let client = aws_sdk_cognitoidentity::Client::from_conf(conf);

    let identity_id = client
        .get_id()
        .identity_pool_id(telemetry_stage.cognito_pool_id)
        .send()
        .await
        .map_err(CredentialsError::provider_error)?
        .identity_id
        .ok_or(CredentialsError::provider_error("no identity_id from get_id"))?;

    let credentials = client
        .get_credentials_for_identity()
        .identity_id(identity_id)
        .send()
        .await
        .map_err(CredentialsError::provider_error)?
        .credentials
        .ok_or(CredentialsError::provider_error(
            "no credentials from get_credentials_for_identity",
        ))?;

    if let Ok(json) = serde_json::to_value(CredentialsJson {
        access_key_id: credentials.access_key_id.clone(),
        secret_key: credentials.secret_key.clone(),
        session_token: credentials.session_token.clone(),
        expiration: credentials.expiration.and_then(|t| t.fmt(DATE_TIME_FORMAT).ok()),
    }) {
        fig_settings::state::set_value(CREDENTIALS_KEY, json).ok();
    }

    let Some(access_key_id) = credentials.access_key_id else {
        return Err(CredentialsError::provider_error("access key id not found"));
    };

    let Some(secret_key) = credentials.secret_key else {
        return Err(CredentialsError::provider_error("secret access key not found"));
    };

    Ok(Credentials::new(
        access_key_id,
        secret_key,
        credentials.session_token,
        credentials.expiration.and_then(|dt| dt.try_into().ok()),
        "",
    ))
}

pub(crate) async fn get_cognito_credentials(telemetry_stage: &TelemetryStage) -> Result<Credentials, CredentialsError> {
    match fig_settings::state::get_string(CREDENTIALS_KEY).ok().flatten() {
        Some(creds) => {
            let CredentialsJson {
                access_key_id,
                secret_key,
                session_token,
                expiration,
            }: CredentialsJson = serde_json::from_str(&creds).map_err(CredentialsError::provider_error)?;

            let Some(access_key_id) = access_key_id else {
                return get_cognito_credentials_send(telemetry_stage).await;
            };

            let Some(secret_key) = secret_key else {
                return get_cognito_credentials_send(telemetry_stage).await;
            };

            Ok(Credentials::new(
                access_key_id,
                secret_key,
                session_token,
                expiration
                    .and_then(|s| DateTime::from_str(&s, DATE_TIME_FORMAT).ok())
                    .and_then(|dt| dt.try_into().ok()),
                "",
            ))
        },
        None => get_cognito_credentials_send(telemetry_stage).await,
    }
}

#[derive(Debug)]
pub(crate) struct CognitoProvider {
    telemetry_stage: TelemetryStage,
}

impl CognitoProvider {
    pub(crate) fn new(telemetry_stage: TelemetryStage) -> CognitoProvider {
        CognitoProvider { telemetry_stage }
    }
}

impl provider::ProvideCredentials for CognitoProvider {
    fn provide_credentials<'a>(&'a self) -> provider::future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        provider::future::ProvideCredentials::new(get_cognito_credentials(&self.telemetry_stage))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn pools() {
        for telemetry_stage in [TelemetryStage::BETA, TelemetryStage::EXTERNAL_PROD] {
            get_cognito_credentials_send(&telemetry_stage).await.unwrap();
        }
    }
}
