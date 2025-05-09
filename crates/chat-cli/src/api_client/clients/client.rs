use amzn_codewhisperer_client::Client as CodewhispererClient;
use amzn_codewhisperer_client::types::{
    OptOutPreference,
    TelemetryEvent,
    UserContext,
};
use tracing::error;

use super::shared::bearer_sdk_config;
use crate::api_client::interceptor::opt_out::OptOutInterceptor;
use crate::api_client::{
    ApiClientError,
    Endpoint,
};
use crate::auth::builder_id::BearerResolver;
use crate::aws_common::{
    UserAgentOverrideInterceptor,
    app_name,
};
use crate::database::Database;
use crate::database::state::{
    AuthProfile,
    StateDatabase,
};

mod inner {
    use amzn_codewhisperer_client::Client as CodewhispererClient;

    #[derive(Clone, Debug)]
    pub enum Inner {
        Codewhisperer(CodewhispererClient),
        Mock,
    }
}

#[derive(Clone, Debug)]
pub struct Client {
    inner: inner::Inner,
    profile: Option<AuthProfile>,
}

impl Client {
    pub async fn new(database: &mut Database, endpoint: Option<Endpoint>) -> Client {
        if cfg!(test) {
            return Self {
                inner: inner::Inner::Mock,
                profile: None,
            };
        }

        let endpoint = endpoint.unwrap_or(Endpoint::load_codewhisperer(database));
        let conf_builder: amzn_codewhisperer_client::config::Builder =
            (&bearer_sdk_config(database, &endpoint).await).into();
        let conf = conf_builder
            .http_client(crate::aws_common::http_client::client(database))
            .interceptor(OptOutInterceptor::new(database))
            .interceptor(UserAgentOverrideInterceptor::new())
            .bearer_token_resolver(BearerResolver)
            .app_name(app_name())
            .endpoint_url(endpoint.url())
            .build();

        let inner = inner::Inner::Codewhisperer(CodewhispererClient::from_conf(conf));

        let profile = match database.get_auth_profile() {
            Ok(profile) => profile,
            Err(err) => {
                error!("Failed to get auth profile: {err}");
                None
            },
        };

        Self { inner, profile }
    }

    pub async fn send_telemetry_event(
        &self,
        telemetry_event: TelemetryEvent,
        user_context: UserContext,
        telemetry_enabled: bool,
    ) -> Result<(), ApiClientError> {
        match &self.inner {
            inner::Inner::Codewhisperer(client) => {
                let _ = client
                    .send_telemetry_event()
                    .telemetry_event(telemetry_event)
                    .user_context(user_context)
                    .opt_out_preference(match telemetry_enabled {
                        true => OptOutPreference::OptIn,
                        false => OptOutPreference::OptOut,
                    })
                    .set_profile_arn(self.profile.as_ref().map(|p| p.arn.clone()))
                    .send()
                    .await;
                Ok(())
            },
            inner::Inner::Mock => Ok(()),
        }
    }

    pub async fn list_available_profiles(&self) -> Result<Vec<AuthProfile>, ApiClientError> {
        match &self.inner {
            inner::Inner::Codewhisperer(client) => {
                let mut profiles = vec![];
                let mut client = client.list_available_profiles().into_paginator().send();
                while let Some(profiles_output) = client.next().await {
                    profiles.extend(profiles_output?.profiles().iter().cloned().map(AuthProfile::from));
                }

                Ok(profiles)
            },
            inner::Inner::Mock => Ok(vec![
                AuthProfile {
                    arn: "my:arn:1".to_owned(),
                    profile_name: "MyProfile".to_owned(),
                },
                AuthProfile {
                    arn: "my:arn:2".to_owned(),
                    profile_name: "MyOtherProfile".to_owned(),
                },
            ]),
        }
    }
}

#[cfg(test)]
mod tests {
    use amzn_codewhisperer_client::types::{
        ChatAddMessageEvent,
        IdeCategory,
        OperatingSystem,
    };

    use super::*;

    #[tokio::test]
    async fn create_clients() {
        let mut database = crate::database::Database::new().await.unwrap();
        let _ = Client::new(&mut database, None).await;
    }

    #[tokio::test]
    async fn test_mock() {
        let mut database = crate::database::Database::new().await.unwrap();
        let client = Client::new(&mut database, None).await;
        client
            .send_telemetry_event(
                TelemetryEvent::ChatAddMessageEvent(
                    ChatAddMessageEvent::builder()
                        .conversation_id("<conversation-id>")
                        .message_id("<message-id>")
                        .build()
                        .unwrap(),
                ),
                UserContext::builder()
                    .ide_category(IdeCategory::Cli)
                    .operating_system(OperatingSystem::Linux)
                    .product("<product>")
                    .build()
                    .unwrap(),
                false,
            )
            .await
            .unwrap();
    }
}
