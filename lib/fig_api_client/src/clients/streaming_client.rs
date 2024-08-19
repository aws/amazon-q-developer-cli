use std::time::Duration;

use amzn_codewhisperer_streaming_client::Client as CodewhispererStreamingClient;
use amzn_qdeveloper_streaming_client::Client as QDeveloperStreamingClient;
use aws_config::timeout::TimeoutConfig;
use aws_types::request_id::RequestId;
use aws_types::sdk_config::StalledStreamProtectionConfig;
use fig_auth::builder_id::BearerResolver;
use fig_aws_common::{
    app_name,
    UserAgentOverrideInterceptor,
};

use super::shared::{
    bearer_sdk_config,
    sigv4_sdk_config,
};
use crate::interceptor::opt_out::OptOutInterceptor;
use crate::model::{
    ChatResponseStream,
    ConversationState,
};
use crate::{
    Endpoint,
    Error,
};

fn stalled_stream_protection_config() -> StalledStreamProtectionConfig {
    // @ptrucks recommends using the same configuration as here
    // https://code.amazon.com/packages/MynahChatTests/blobs/b3e23c24e5abce0150f872883f80efc0df50ebbc/--/src/main.rs#L37-L41
    StalledStreamProtectionConfig::enabled()
        .grace_period(Duration::from_secs(20))
        .build()
}

fn timeout_config() -> TimeoutConfig {
    TimeoutConfig::builder()
        .read_timeout(Duration::from_secs(10))
        .operation_timeout(Duration::from_secs(10))
        .operation_attempt_timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(10))
        .build()
}

mod inner {
    use amzn_codewhisperer_streaming_client::Client as CodewhispererStreamingClient;
    use amzn_qdeveloper_streaming_client::Client as QDeveloperStreamingClient;

    use crate::model::ChatResponseStream;

    #[derive(Clone, Debug)]
    pub enum Inner {
        Codewhisperer(CodewhispererStreamingClient),
        QDeveloper(QDeveloperStreamingClient),
        Mock(Vec<ChatResponseStream>),
    }
}

#[derive(Clone, Debug)]
pub struct StreamingClient(inner::Inner);

impl StreamingClient {
    pub async fn new() -> Result<Self, Error> {
        let client = if fig_util::system_info::in_cloudshell() {
            Self::new_qdeveloper_client(&Endpoint::load_q()).await?
        } else {
            Self::new_codewhisperer_client(&Endpoint::load_codewhisperer()).await
        };
        Ok(client)
    }

    pub fn mock(events: Vec<ChatResponseStream>) -> Self {
        Self(inner::Inner::Mock(events))
    }

    pub async fn new_codewhisperer_client(endpoint: &Endpoint) -> Self {
        let conf_builder: amzn_codewhisperer_streaming_client::config::Builder =
            (&bearer_sdk_config(endpoint).await).into();
        let conf = conf_builder
            .interceptor(OptOutInterceptor::new())
            .interceptor(UserAgentOverrideInterceptor::new())
            .bearer_token_resolver(BearerResolver)
            .app_name(app_name())
            .endpoint_url(endpoint.url())
            .stalled_stream_protection(stalled_stream_protection_config())
            .timeout_config(timeout_config())
            .build();
        let client = CodewhispererStreamingClient::from_conf(conf);
        Self(inner::Inner::Codewhisperer(client))
    }

    pub async fn new_qdeveloper_client(endpoint: &Endpoint) -> Result<Self, Error> {
        let conf_builder: amzn_qdeveloper_streaming_client::config::Builder =
            (&sigv4_sdk_config(endpoint).await?).into();
        let conf = conf_builder
            .interceptor(OptOutInterceptor::new())
            .interceptor(UserAgentOverrideInterceptor::new())
            .app_name(app_name())
            .endpoint_url(endpoint.url())
            .stalled_stream_protection(stalled_stream_protection_config())
            .timeout_config(timeout_config())
            .build();
        let client = QDeveloperStreamingClient::from_conf(conf);
        Ok(Self(inner::Inner::QDeveloper(client)))
    }

    pub async fn send_message(&self, conversation_state: ConversationState) -> Result<SendMessageOutput, Error> {
        let ConversationState {
            conversation_id,
            user_input_message,
        } = conversation_state;

        match &self.0 {
            inner::Inner::Codewhisperer(client) => {
                let conversation_state_builder =
                    amzn_codewhisperer_streaming_client::types::ConversationState::builder()
                        .set_conversation_id(conversation_id)
                        .current_message(
                            amzn_codewhisperer_streaming_client::types::ChatMessage::UserInputMessage(
                                user_input_message.into(),
                            ),
                        )
                        .chat_trigger_type(amzn_codewhisperer_streaming_client::types::ChatTriggerType::Manual);

                Ok(SendMessageOutput::Codewhisperer(
                    client
                        .generate_assistant_response()
                        .conversation_state(conversation_state_builder.build().expect("fix me"))
                        .send()
                        .await?,
                ))
            },
            inner::Inner::QDeveloper(client) => {
                let conversation_state_builder = amzn_qdeveloper_streaming_client::types::ConversationState::builder()
                    .set_conversation_id(conversation_id)
                    .current_message(amzn_qdeveloper_streaming_client::types::ChatMessage::UserInputMessage(
                        user_input_message.into(),
                    ))
                    .chat_trigger_type(amzn_qdeveloper_streaming_client::types::ChatTriggerType::Manual);

                Ok(SendMessageOutput::QDeveloper(
                    client
                        .send_message()
                        .conversation_state(conversation_state_builder.build().expect("fix me"))
                        .send()
                        .await?,
                ))
            },
            inner::Inner::Mock(events) => {
                let mut new_events = events.clone();
                new_events.reverse();
                Ok(SendMessageOutput::Mock(new_events))
            },
        }
    }
}

pub enum SendMessageOutput {
    Codewhisperer(
        amzn_codewhisperer_streaming_client::operation::generate_assistant_response::GenerateAssistantResponseOutput,
    ),
    QDeveloper(amzn_qdeveloper_streaming_client::operation::send_message::SendMessageOutput),
    Mock(Vec<ChatResponseStream>),
}

impl SendMessageOutput {
    pub async fn recv(&mut self) -> Result<Option<ChatResponseStream>, Error> {
        match self {
            SendMessageOutput::Codewhisperer(output) => Ok(output
                .generate_assistant_response_response
                .recv()
                .await?
                .map(|s| s.into())),
            SendMessageOutput::QDeveloper(output) => Ok(output.send_message_response.recv().await?.map(|s| s.into())),
            SendMessageOutput::Mock(vec) => Ok(vec.pop()),
        }
    }
}

impl RequestId for SendMessageOutput {
    fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::QDeveloper(output) => output.request_id(),
            SendMessageOutput::Mock(_) => Some("<mock-request-id>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::UserInputMessage;

    #[tokio::test]
    async fn create_clients() {
        let endpoint = Endpoint::load_codewhisperer();

        let _ = StreamingClient::new().await;
        let _ = StreamingClient::new_codewhisperer_client(&endpoint).await;
        let _ = StreamingClient::new_qdeveloper_client(&endpoint).await;
    }

    #[tokio::test]
    async fn test_mock() {
        let client = StreamingClient::mock(vec![
            ChatResponseStream::assistant_response("Hello!"),
            ChatResponseStream::assistant_response(" How can I"),
            ChatResponseStream::assistant_response(" assist you today?"),
        ]);
        let mut output = client
            .send_message(ConversationState {
                conversation_id: None,
                user_input_message: UserInputMessage {
                    content: "Hello".into(),
                    user_input_message_context: None,
                    user_intent: None,
                },
            })
            .await
            .unwrap();

        let mut output_content = String::new();
        while let Some(ChatResponseStream::AssistantResponseEvent { content }) = output.recv().await.unwrap() {
            output_content.push_str(&content);
        }
        assert_eq!(output_content, "Hello! How can I assist you today?");
    }
}