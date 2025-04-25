use std::sync::{
    Arc,
    Mutex,
};

use amzn_codewhisperer_client::Client as CodewhispererClient;
use amzn_codewhisperer_client::operation::generate_completions::GenerateCompletionsError;
use amzn_codewhisperer_client::types::error::AccessDeniedError;
use amzn_codewhisperer_client::types::{
    AccessDeniedExceptionReason,
    OptOutPreference,
    TelemetryEvent,
    UserContext,
};
use amzn_consolas_client::Client as ConsolasClient;
use amzn_consolas_client::error::DisplayErrorContext;
use amzn_consolas_client::operation::generate_recommendations::GenerateRecommendationsError;
use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_types::request_id::RequestId;
use fig_auth::builder_id::BearerResolver;
use fig_aws_common::{
    UserAgentOverrideInterceptor,
    app_name,
};
use fig_settings::State;
use tracing::error;

use super::shared::{
    bearer_sdk_config,
    sigv4_sdk_config,
};
use crate::interceptor::opt_out::OptOutInterceptor;
use crate::interceptor::session_id::SessionIdInterceptor;
use crate::model::{
    Recommendation,
    RecommendationsInput,
    RecommendationsOutput,
};
use crate::profile::Profile;
use crate::{
    Customization,
    Endpoint,
    Error,
};

// Limits for FileContext
pub const FILE_CONTEXT_LEFT_FILE_CONTENT_MAX_LEN: usize = 10240;
pub const FILE_CONTEXT_RIGHT_FILE_CONTENT_MAX_LEN: usize = 10240;
pub const FILE_CONTEXT_FILE_NAME_MAX_LEN: usize = 1024;

mod inner {
    use amzn_codewhisperer_client::Client as CodewhispererClient;
    use amzn_consolas_client::Client as ConsolasClient;

    #[derive(Clone, Debug)]
    pub enum Inner {
        Codewhisperer(CodewhispererClient),
        Consolas(ConsolasClient),
        Mock,
    }
}

#[derive(Clone, Debug)]
pub struct Client {
    inner: inner::Inner,
    profile_arn: Option<String>,
}

impl Client {
    pub async fn new() -> Result<Client, Error> {
        let endpoint = Endpoint::load_codewhisperer();
        let client = if fig_util::system_info::in_cloudshell() {
            Self::new_consolas_client(&endpoint).await?
        } else {
            Self::new_codewhisperer_client(&endpoint).await
        };
        Ok(client)
    }

    pub fn mock() -> Self {
        Self {
            inner: inner::Inner::Mock,
            profile_arn: None,
        }
    }

    pub async fn new_codewhisperer_client(endpoint: &Endpoint) -> Self {
        let conf_builder: amzn_codewhisperer_client::config::Builder = (&bearer_sdk_config(endpoint).await).into();
        let conf = conf_builder
            .http_client(fig_aws_common::http_client::client())
            .interceptor(OptOutInterceptor::new())
            .interceptor(UserAgentOverrideInterceptor::new())
            .bearer_token_resolver(BearerResolver)
            .app_name(app_name())
            .endpoint_url(endpoint.url())
            .build();

        let inner = inner::Inner::Codewhisperer(CodewhispererClient::from_conf(conf));

        let profile_arn = match fig_settings::state::get_value("api.codewhisperer.profile") {
            Ok(Some(profile)) => match profile.get("arn") {
                Some(arn) => Some(arn.to_string()),
                None => {
                    error!("Stored profile does not contain an arn");
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

    pub async fn new_consolas_client(endpoint: &Endpoint) -> Result<Self, Error> {
        let conf_builder: amzn_consolas_client::config::Builder = (&sigv4_sdk_config(endpoint).await?).into();
        let conf = conf_builder
            .http_client(fig_aws_common::http_client::client())
            .interceptor(OptOutInterceptor::new())
            .interceptor(UserAgentOverrideInterceptor::new())
            .app_name(app_name())
            .endpoint_url(endpoint.url())
            .build();
        Ok(Self {
            inner: inner::Inner::Consolas(ConsolasClient::from_conf(conf)),
            profile_arn: None,
        })
    }

    pub async fn generate_recommendations(
        &self,
        mut input: RecommendationsInput,
    ) -> Result<RecommendationsOutput, Error> {
        let truncate_left = |s: String, max_len: usize| {
            if s.len() > max_len {
                s[(s.len() - max_len)..].into()
            } else {
                s
            }
        };

        let truncate_right = |s: String, max_len: usize| {
            if s.len() > max_len { s[..max_len].into() } else { s }
        };

        let filename = truncate_right(input.file_context.filename, FILE_CONTEXT_FILE_NAME_MAX_LEN);
        let left_content = truncate_left(
            input.file_context.left_file_content,
            FILE_CONTEXT_LEFT_FILE_CONTENT_MAX_LEN,
        );
        let right_content = truncate_right(
            input.file_context.right_file_content,
            FILE_CONTEXT_RIGHT_FILE_CONTENT_MAX_LEN,
        );

        input.file_context.filename = filename;
        input.file_context.left_file_content = left_content;
        input.file_context.right_file_content = right_content;

        match &self.inner {
            inner::Inner::Codewhisperer(client) => {
                Ok(codewhisperer_generate_recommendation(client, input, self.profile_arn.clone()).await?)
            },
            inner::Inner::Consolas(client) => Ok(consolas_generate_recommendation(client, input).await?),
            inner::Inner::Mock => Ok(RecommendationsOutput {
                recommendations: vec![Recommendation {
                    content: "Hello, world!".to_string(),
                }],
                next_token: None,
                session_id: Some("xxx".to_string()),
                request_id: Some("yyy".to_string()),
            }),
        }
    }

    /// List the customizations the user has access to
    pub async fn list_customizations(&self) -> Result<Vec<Customization>, Error> {
        let mut customizations = Vec::new();

        match &self.inner {
            inner::Inner::Codewhisperer(client) => {
                let mut paginator = client
                    .list_available_customizations()
                    .set_profile_arn(self.profile_arn.clone())
                    .into_paginator()
                    .send();
                while let Some(res) = paginator.next().await {
                    let output = res?;
                    customizations.extend(output.customizations.into_iter().map(Into::into));
                }
            },
            inner::Inner::Consolas(client) => {
                let mut pag = client.list_customizations().into_paginator().send();
                while let Some(res) = pag.next().await {
                    let output = res?;
                    customizations.extend(output.customizations.into_iter().map(Into::into));
                }
            },
            inner::Inner::Mock => customizations.extend([
                Customization {
                    arn: "arn:aws:codewhisperer:us-east-1:000000000000:customization/ABCDEF123456".into(),
                    name: Some("my-customization".into()),
                    description: Some("My customization".into()),
                },
                Customization {
                    arn: "arn:aws:codewhisperer:us-east-1:000000000000:customization/GHIJKL987654".into(),
                    name: Some("my-other-customization".into()),
                    description: Some("My other customization".into()),
                },
            ]),
        }

        Ok(customizations)
    }

    // .telemetry_event(TelemetryEvent::UserTriggerDecisionEvent(user_trigger_decision_event))
    // .user_context(user_context)
    // .opt_out_preference(opt_out_preference)
    pub async fn send_telemetry_event(
        &self,
        telemetry_event: TelemetryEvent,
        user_context: UserContext,
        opt_out: OptOutPreference,
    ) -> Result<(), Error> {
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
            inner::Inner::Consolas(_) => Err(Error::UnsupportedConsolas("send_telemetry_event")),
            inner::Inner::Mock => Ok(()),
        }
    }

    pub async fn list_available_profiles(&self) -> Result<Vec<Profile>, Error> {
        match &self.inner {
            inner::Inner::Codewhisperer(client) => {
                let mut profiles = vec![];
                let mut client = client.list_available_profiles().into_paginator().send();
                while let Some(profiles_output) = client.next().await {
                    println!("profiles_output: {:?}", profiles_output);
                    profiles.extend(profiles_output?.profiles().iter().cloned().map(Profile::from));
                }

                Ok(profiles)
            },
            inner::Inner::Consolas(_) => Err(Error::UnsupportedConsolas("list_available_profiles")),
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

async fn codewhisperer_generate_recommendation_inner(
    client: &CodewhispererClient,
    input: RecommendationsInput,
    profile_arn: Option<String>,
) -> Result<RecommendationsOutput, SdkError<GenerateCompletionsError, HttpResponse>> {
    let session_id_lock = Arc::new(Mutex::new(None));

    let customization_arn = match Customization::load_selected(&State::new()) {
        Ok(opt) => opt.map(|Customization { arn, .. }| arn),
        Err(err) => {
            error!(%err, "Failed to load selected customization");
            None
        },
    };

    let output = client
        .generate_completions()
        .file_context(
            amzn_codewhisperer_client::types::FileContext::builder()
                .left_file_content(input.file_context.left_file_content)
                .right_file_content(input.file_context.right_file_content)
                .filename(input.file_context.filename)
                .programming_language(
                    amzn_codewhisperer_client::types::ProgrammingLanguage::builder()
                        .language_name(input.file_context.programming_language.language_name.as_ref())
                        .build()?,
                )
                .build()?,
        )
        .max_results(input.max_results)
        .set_next_token(input.next_token)
        .set_customization_arn(customization_arn)
        .set_profile_arn(profile_arn.clone())
        .customize()
        .interceptor(SessionIdInterceptor::new(session_id_lock.clone()))
        .send()
        .await?;

    let session_id = {
        let mut session_id_lock = session_id_lock.lock().expect("Failed to lock session ID");
        session_id_lock.take()
    };

    Ok(RecommendationsOutput {
        request_id: output.request_id().map(Into::into),
        recommendations: output
            .completions
            .unwrap_or_default()
            .into_iter()
            .map(|c| Recommendation { content: c.content })
            .collect(),
        next_token: output.next_token,
        session_id,
    })
}

/// Make a request to the CodeWhisperer API with error handling for invalid customizations with the
/// specified client
async fn codewhisperer_generate_recommendation(
    client: &CodewhispererClient,
    input: RecommendationsInput,
    profile_arn: Option<String>,
) -> Result<RecommendationsOutput, SdkError<GenerateCompletionsError, HttpResponse>> {
    let res = codewhisperer_generate_recommendation_inner(client, input.clone(), profile_arn.clone()).await;

    let output = match res {
        Ok(output) => output,
        Err(ref err @ SdkError::ServiceError(ref service_err))
            if matches!(
                service_err.err(),
                GenerateCompletionsError::AccessDeniedError(AccessDeniedError {
                    reason: Some(AccessDeniedExceptionReason::UnauthorizedCustomizationResourceAccess),
                    ..
                })
            ) =>
        {
            error!(err =% DisplayErrorContext(err), "Access denied for selected customization, clearing selection and trying again");
            if let Err(err) = Customization::delete_selected(&State::new()) {
                error!(%err, "Failed to delete selected customization");
            }
            codewhisperer_generate_recommendation_inner(client, input, profile_arn.clone()).await?
        },
        Err(err) => return Err(err),
    };
    Ok(output)
}

pub async fn consolas_generate_recommendation(
    client: &ConsolasClient,
    input: RecommendationsInput,
) -> Result<RecommendationsOutput, SdkError<GenerateRecommendationsError, HttpResponse>> {
    let session_id_lock = Arc::new(Mutex::new(None));

    let output = client
        .generate_recommendations()
        .file_context(
            amzn_consolas_client::types::FileContext::builder()
                .left_file_content(input.file_context.left_file_content)
                .right_file_content(input.file_context.right_file_content)
                .filename(input.file_context.filename)
                .programming_language(
                    amzn_consolas_client::types::ProgrammingLanguage::builder()
                        .language_name(input.file_context.programming_language.language_name.as_ref())
                        .build()?,
                )
                .build()?,
        )
        .max_results(input.max_results)
        .set_next_token(input.next_token)
        .customize()
        .interceptor(SessionIdInterceptor::new(session_id_lock.clone()))
        .send()
        .await?;

    let session_id = {
        let mut session_id_lock = session_id_lock.lock().expect("Failed to lock session ID");
        session_id_lock.take()
    };

    Ok(RecommendationsOutput {
        request_id: output.request_id().map(Into::into),
        recommendations: output
            .recommendations
            .unwrap_or_default()
            .into_iter()
            .map(|c| Recommendation { content: c.content })
            .collect(),
        next_token: output.next_token,
        session_id,
    })
}

#[cfg(test)]
mod tests {
    use amzn_codewhisperer_client::types::{
        ChatAddMessageEvent,
        IdeCategory,
        OperatingSystem,
    };

    use super::*;
    use crate::model::{
        FileContext,
        LanguageName,
        ProgrammingLanguage,
    };

    #[tokio::test]
    async fn create_clients() {
        let endpoint = Endpoint::load_codewhisperer();

        let _ = Client::new().await;
        let _ = Client::new_codewhisperer_client(&endpoint).await;
        let _ = Client::new_consolas_client(&endpoint).await;
    }

    #[tokio::test]
    async fn test_mock() {
        let client = Client::mock();
        client
            .generate_recommendations(RecommendationsInput {
                file_context: FileContext {
                    left_file_content: "left".into(),
                    right_file_content: "right".into(),
                    filename: "filename".into(),
                    programming_language: ProgrammingLanguage {
                        language_name: LanguageName::Rust,
                    },
                },
                max_results: 0,
                next_token: None,
            })
            .await
            .unwrap();
        client.list_customizations().await.unwrap();
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
