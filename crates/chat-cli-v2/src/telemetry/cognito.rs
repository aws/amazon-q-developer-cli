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
    info,
    trace,
    warn,
};

use crate::aws_common::app_name;
use crate::database::{
    CredentialsJson,
    Database,
};
use crate::telemetry::TelemetryStage;

fn create_cognito_client(telemetry_stage: &TelemetryStage) -> aws_sdk_cognitoidentity::Client {
    let conf = aws_sdk_cognitoidentity::Config::builder()
        .behavior_version(BehaviorVersion::v2026_01_12())
        .region(telemetry_stage.region.clone())
        .app_name(app_name())
        .build();
    aws_sdk_cognitoidentity::Client::from_conf(conf)
}

async fn send_cognito_request(
    client: &aws_sdk_cognitoidentity::Client,
    database: &mut Database,
    telemetry_stage: &TelemetryStage,
) -> Result<Credentials, CredentialsError> {
    let identity_id = match database.get_cognito_identity_id().ok().flatten() {
        Some(id) => {
            trace!("Using cached cognito identity_id");
            id
        },
        None => {
            info!("Cognito GetId called, no cached identity_id");
            let id = client
                .get_id()
                .identity_pool_id(telemetry_stage.cognito_pool_id)
                .send()
                .await
                .map_err(CredentialsError::provider_error)?
                .identity_id
                .ok_or(CredentialsError::provider_error("no identity_id from get_id"))?;
            database.set_cognito_identity_id(&id).ok();
            id
        },
    };

    let credentials = match client
        .get_credentials_for_identity()
        .identity_id(&identity_id)
        .send()
        .await
    {
        Ok(resp) => resp.credentials.ok_or(CredentialsError::provider_error(
            "no credentials from get_credentials_for_identity",
        ))?,
        Err(err) => {
            // Identity may be invalid — clear cache and retry with a fresh GetId
            warn!(?err, "GetCredentialsForIdentity failed, clearing cached identity_id");
            database.clear_cognito_identity_id().ok();
            let new_id = client
                .get_id()
                .identity_pool_id(telemetry_stage.cognito_pool_id)
                .send()
                .await
                .map_err(CredentialsError::provider_error)?
                .identity_id
                .ok_or(CredentialsError::provider_error("no identity_id from get_id"))?;
            database.set_cognito_identity_id(&new_id).ok();
            client
                .get_credentials_for_identity()
                .identity_id(new_id)
                .send()
                .await
                .map_err(CredentialsError::provider_error)?
                .credentials
                .ok_or(CredentialsError::provider_error(
                    "no credentials from get_credentials_for_identity after retry",
                ))?
        },
    };

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

pub async fn get_cognito_credentials_send(
    database: &mut Database,
    telemetry_stage: &TelemetryStage,
) -> Result<Credentials, CredentialsError> {
    trace!("Creating new cognito credentials");
    let client = create_cognito_client(telemetry_stage);
    send_cognito_request(&client, database, telemetry_stage).await
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
    use aws_sdk_cognitoidentity::operation::get_credentials_for_identity::{
        GetCredentialsForIdentityError,
        GetCredentialsForIdentityOutput,
    };
    use aws_sdk_cognitoidentity::operation::get_id::GetIdOutput;
    use aws_sdk_cognitoidentity::types::Credentials as CognitoCredentials;
    use aws_smithy_mocks::{
        RuleMode,
        mock,
        mock_client,
    };

    use super::*;

    fn mock_creds() -> CognitoCredentials {
        CognitoCredentials::builder()
            .access_key_id("test_access_key")
            .secret_key("test_secret_key")
            .session_token("test_session_token")
            .build()
    }

    #[tokio::test]
    async fn pools() {
        let get_id_rule = mock!(aws_sdk_cognitoidentity::Client::get_id)
            .then_output(|| GetIdOutput::builder().identity_id("us-east-1:test-identity-id").build());

        let get_creds_rule = mock!(aws_sdk_cognitoidentity::Client::get_credentials_for_identity).then_output(|| {
            GetCredentialsForIdentityOutput::builder()
                .credentials(mock_creds())
                .build()
        });

        let client = mock_client!(aws_sdk_cognitoidentity, RuleMode::MatchAny, [
            &get_id_rule,
            &get_creds_rule
        ]);

        for telemetry_stage in [TelemetryStage::BETA, TelemetryStage::EXTERNAL_PROD] {
            let creds = send_cognito_request(&client, &mut Database::new().await.unwrap(), &telemetry_stage)
                .await
                .expect("mock should intercept cognito calls");
            assert_eq!(
                creds.access_key_id(),
                "test_access_key",
                "credentials should come from mock"
            );
        }
    }

    #[tokio::test]
    async fn caches_identity_id_skips_get_id() {
        let mut db = Database::new().await.unwrap();
        db.set_cognito_identity_id("us-east-1:cached-id").unwrap();

        let get_id_rule = mock!(aws_sdk_cognitoidentity::Client::get_id).then_output(|| {
            GetIdOutput::builder()
                .identity_id("us-east-1:should-not-be-called")
                .build()
        });

        let get_creds_rule = mock!(aws_sdk_cognitoidentity::Client::get_credentials_for_identity).then_output(|| {
            GetCredentialsForIdentityOutput::builder()
                .credentials(mock_creds())
                .build()
        });

        let client = mock_client!(aws_sdk_cognitoidentity, RuleMode::MatchAny, [
            &get_id_rule,
            &get_creds_rule
        ]);

        let creds = send_cognito_request(&client, &mut db, &TelemetryStage::BETA)
            .await
            .unwrap();

        assert_eq!(creds.access_key_id(), "test_access_key");
        assert_eq!(
            get_id_rule.num_calls(),
            0,
            "GetId should not be called when identity_id is cached"
        );
        assert_eq!(get_creds_rule.num_calls(), 1);
    }

    #[tokio::test]
    async fn no_cached_identity_calls_get_id() {
        let mut db = Database::new().await.unwrap();

        let get_id_rule = mock!(aws_sdk_cognitoidentity::Client::get_id)
            .then_output(|| GetIdOutput::builder().identity_id("us-east-1:new-id").build());

        let get_creds_rule = mock!(aws_sdk_cognitoidentity::Client::get_credentials_for_identity).then_output(|| {
            GetCredentialsForIdentityOutput::builder()
                .credentials(mock_creds())
                .build()
        });

        let client = mock_client!(aws_sdk_cognitoidentity, RuleMode::MatchAny, [
            &get_id_rule,
            &get_creds_rule
        ]);

        send_cognito_request(&client, &mut db, &TelemetryStage::BETA)
            .await
            .unwrap();

        assert_eq!(
            get_id_rule.num_calls(),
            1,
            "GetId should be called when no cached identity_id"
        );
        assert_eq!(get_creds_rule.num_calls(), 1);
        assert_eq!(
            db.get_cognito_identity_id().unwrap(),
            Some("us-east-1:new-id".to_string())
        );
    }

    #[tokio::test]
    async fn invalid_cached_identity_falls_back_to_get_id() {
        let mut db = Database::new().await.unwrap();
        db.set_cognito_identity_id("us-east-1:invalid-id").unwrap();

        let get_creds_rule = mock!(aws_sdk_cognitoidentity::Client::get_credentials_for_identity)
            .sequence()
            .error(|| GetCredentialsForIdentityError::unhandled("invalid identity"))
            .output(|| {
                GetCredentialsForIdentityOutput::builder()
                    .credentials(mock_creds())
                    .build()
            })
            .build();

        let get_id_rule = mock!(aws_sdk_cognitoidentity::Client::get_id)
            .then_output(|| GetIdOutput::builder().identity_id("us-east-1:fresh-id").build());

        let client = mock_client!(aws_sdk_cognitoidentity, RuleMode::MatchAny, [
            &get_id_rule,
            &get_creds_rule
        ]);

        let creds = send_cognito_request(&client, &mut db, &TelemetryStage::BETA)
            .await
            .unwrap();

        assert_eq!(creds.access_key_id(), "test_access_key");
        assert_eq!(
            get_id_rule.num_calls(),
            1,
            "GetId should be called after invalid identity"
        );
        assert_eq!(
            get_creds_rule.num_calls(),
            2,
            "GetCredentialsForIdentity should be called twice"
        );
        assert_eq!(
            db.get_cognito_identity_id().unwrap(),
            Some("us-east-1:fresh-id".to_string())
        );
    }
}
