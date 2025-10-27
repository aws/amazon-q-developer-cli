use std::time::SystemTime;

use amzn_toolkit_telemetry_client::config::BehaviorVersion;
use aws_credential_types::provider::error::CredentialsError;
use aws_credential_types::{
    Credentials,
    provider,
};
use aws_sdk_cognitoidentity::primitives::{
    DateTime,
    DateTimeFormat,
};
use tracing::{
    trace,
    warn,
};

use crate::aws_common::app_name;
use crate::database::{
    CredentialsJson,
    Database,
};
use crate::telemetry::TelemetryStage;

pub async fn get_cognito_credentials_send(
    database: &mut Database,
    telemetry_stage: &TelemetryStage,
) -> Result<Credentials, CredentialsError> {
    trace!("Creating new cognito credentials");

    let conf = aws_sdk_cognitoidentity::Config::builder()
        .behavior_version(BehaviorVersion::v2025_08_07())
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

    database.set_credentials_entry(&credentials).ok();

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

pub async fn get_cognito_credentials(
    database: &mut Database,
    telemetry_stage: &TelemetryStage,
) -> Result<Credentials, CredentialsError> {
    match database
        .get_credentials_entry()
        .map_err(CredentialsError::provider_error)?
    {
        Some(CredentialsJson {
            access_key_id,
            secret_key,
            session_token,
            expiration,
        }) => {
            if is_expired(expiration.as_ref()) {
                return get_cognito_credentials_send(database, telemetry_stage).await;
            }

            let Some(access_key_id) = access_key_id else {
                return get_cognito_credentials_send(database, telemetry_stage).await;
            };

            let Some(secret_key) = secret_key else {
                return get_cognito_credentials_send(database, telemetry_stage).await;
            };

            Ok(Credentials::new(
                access_key_id,
                secret_key,
                session_token,
                expiration
                    .and_then(|s| DateTime::from_str(&s, DateTimeFormat::DateTime).ok())
                    .and_then(|dt| dt.try_into().ok()),
                "",
            ))
        },
        None => get_cognito_credentials_send(database, telemetry_stage).await,
    }
}

#[derive(Debug)]
pub struct CognitoProvider {
    telemetry_stage: TelemetryStage,
}

impl CognitoProvider {
    pub fn new(telemetry_stage: TelemetryStage) -> CognitoProvider {
        CognitoProvider { telemetry_stage }
    }
}

impl provider::ProvideCredentials for CognitoProvider {
    fn provide_credentials<'a>(&'a self) -> provider::future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        provider::future::ProvideCredentials::new(async {
            match Database::new().await {
                Ok(mut db) => get_cognito_credentials(&mut db, &self.telemetry_stage).await,
                Err(err) => Err(CredentialsError::provider_error(format!(
                    "failed to get database: {err:?}"
                ))),
            }
        })
    }
}

fn is_expired(expiration: Option<&String>) -> bool {
    let expiration = if let Some(v) = expiration {
        v
    } else {
        warn!("no cognito expiration was saved");
        return true;
    };

    match DateTime::from_str(expiration, DateTimeFormat::DateTime) {
        Ok(expiration) => {
            // Check if the expiration is at least after five minutes after the current time.
            let curr: DateTime = (SystemTime::now() + std::time::Duration::from_secs(60 * 5)).into();
            expiration < curr
        },
        Err(err) => {
            warn!(?err, "invalid cognito expiration was saved");
            true
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn pools() {
        for telemetry_stage in [TelemetryStage::BETA, TelemetryStage::EXTERNAL_PROD] {
            get_cognito_credentials_send(&mut Database::new().await.unwrap(), &telemetry_stage)
                .await
                .unwrap();
        }
    }
}
