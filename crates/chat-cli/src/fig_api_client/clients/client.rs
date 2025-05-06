use amzn_codewhisperer_client::Client as CodewhispererClient;
use amzn_codewhisperer_client::types::{
    OptOutPreference,
    TelemetryEvent,
    UserContext,
};
use tracing::error;

use super::shared::bearer_sdk_config;
use crate::fig_api_client::interceptor::opt_out::OptOutInterceptor;
use crate::fig_api_client::profile::Profile;
use crate::fig_api_client::{
    ApiClientError,
    Endpoint,
};
use crate::fig_auth::builder_id::BearerResolver;
use crate::fig_aws_common::{
    UserAgentOverrideInterceptor,
    app_name,
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
    profile_arn: Option<String>,
}

impl Client {
    pub async fn new() -> Result<Client, ApiClientError> {
        if cfg!(test) {
            return Ok(Self {
                inner: inner::Inner::Mock,
                profile_arn: None,
            });
        }

        let endpoint = Endpoint::load_codewhisperer();
        Ok(Self::new_codewhisperer_client(&endpoint).await)
    }

    pub async fn new_codewhisperer_client(endpoint: &Endpoint) -> Self {
        let conf_builder: amzn_codewhisperer_client::config::Builder = (&bearer_sdk_config(endpoint).await).into();
        let conf = conf_builder
            .http_client(crate::fig_aws_common::http_client::client())
            .interceptor(OptOutInterceptor::new())
            .interceptor(UserAgentOverrideInterceptor::new())
            .bearer_token_resolver(BearerResolver)
            .app_name(app_name())
            .endpoint_url(endpoint.url())
            .build();

        let inner = inner::Inner::Codewhisperer(CodewhispererClient::from_conf(conf));

        let profile_arn = match crate::fig_settings::state::get_value("api.codewhisperer.profile") {
            Ok(Some(profile)) => match profile.get("arn") {
                Some(arn) => match arn.as_str() {
                    Some(arn) => Some(arn.to_string()),
                    None => {
                        error!("Stored arn is not a string. Instead it was: {arn}");
                        None
                    },
                },
                None => {
                    error!("Stored profile does not contain an arn. Instead it was: {profile}");
                    None
                },
            },
            Ok(None) => None,
            Err(err) => {
                error!("Failed to retrieve profile: {}", err);
                None
            },
        };

        Self { inner, profile_arn }
    }

    // .telemetry_event(TelemetryEvent::UserTriggerDecisionEvent(user_trigger_decision_event))
    // .user_context(user_context)
    // .opt_out_preference(opt_out_preference)
    pub async fn send_telemetry_event(
        &self,
        telemetry_event: TelemetryEvent,
        user_context: UserContext,
        opt_out: OptOutPreference,
    ) -> Result<(), ApiClientError> {
        match &self.inner {
            inner::Inner::Codewhisperer(client) => {
                let _ = client
                    .send_telemetry_event()
                    .telemetry_event(telemetry_event)
                    .user_context(user_context)
                    .opt_out_preference(opt_out)
                    .set_profile_arn(self.profile_arn.clone())
                    .send()
                    .await;
                Ok(())
            },
            inner::Inner::Mock => Ok(()),
        }
    }

    pub async fn list_available_profiles(&self) -> Result<Vec<Profile>, ApiClientError> {
        match &self.inner {
            inner::Inner::Codewhisperer(client) => {
                let mut profiles = vec![];
                let mut client = client.list_available_profiles().into_paginator().send();
                while let Some(profiles_output) = client.next().await {
                    profiles.extend(profiles_output?.profiles().iter().cloned().map(Profile::from));
                }

                Ok(profiles)
            },
            inner::Inner::Mock => Ok(vec![
                Profile {
                    arn: "my:arn:1".to_owned(),
                    profile_name: "MyProfile".to_owned(),
                },
                Profile {
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
        let endpoint = Endpoint::load_codewhisperer();

        let _ = Client::new().await;
        let _ = Client::new_codewhisperer_client(&endpoint).await;
    }

    #[tokio::test]
    async fn test_mock() {
        let client = Client::new().await.unwrap();
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
                OptOutPreference::OptIn,
            )
            .await
            .unwrap();
    }
}
