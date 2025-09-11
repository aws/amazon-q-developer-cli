use rmcp::model::{
    ListPromptsResult,
    ListResourceTemplatesResult,
    ListResourcesResult,
    ListToolsResult,
};
use rmcp::{
    Peer,
    RoleClient,
};
use tokio::sync::mpsc::{
    Receiver,
    Sender,
    channel,
};

use crate::mcp_client::messenger::{
    Messenger,
    MessengerError,
    MessengerResult,
    Result,
};

#[allow(dead_code)]
#[derive(Debug)]
pub enum UpdateEventMessage {
    ListToolsResult {
        server_name: String,
        result: Result<ListToolsResult>,
        peer: Option<Peer<RoleClient>>,
    },
    ListPromptsResult {
        server_name: String,
        result: Result<ListPromptsResult>,
        peer: Option<Peer<RoleClient>>,
    },
    ListResourcesResult {
        server_name: String,
        result: Result<ListResourcesResult>,
        peer: Option<Peer<RoleClient>>,
    },
    ResourceTemplatesListResult {
        server_name: String,
        result: Result<ListResourceTemplatesResult>,
        peer: Option<Peer<RoleClient>>,
    },
    OauthLink {
        server_name: String,
        link: String,
    },
    InitStart {
        server_name: String,
    },
    Deinit {
        server_name: String,
    },
}

#[derive(Clone, Debug)]
pub struct ServerMessengerBuilder {
    pub update_event_sender: Sender<UpdateEventMessage>,
}

impl ServerMessengerBuilder {
    pub fn new(capacity: usize) -> (Receiver<UpdateEventMessage>, Self) {
        let (tx, rx) = channel::<UpdateEventMessage>(capacity);
        let this = Self {
            update_event_sender: tx,
        };
        (rx, this)
    }

    pub fn build_with_name(&self, server_name: String) -> ServerMessenger {
        ServerMessenger {
            server_name,
            update_event_sender: self.update_event_sender.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ServerMessenger {
    pub server_name: String,
    pub update_event_sender: Sender<UpdateEventMessage>,
}

#[async_trait::async_trait]
impl Messenger for ServerMessenger {
    async fn send_tools_list_result(
        &self,
        result: Result<ListToolsResult>,
        peer: Option<Peer<RoleClient>>,
    ) -> MessengerResult {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::ListToolsResult {
                server_name: self.server_name.clone(),
                result,
                peer,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    async fn send_prompts_list_result(
        &self,
        result: Result<ListPromptsResult>,
        peer: Option<Peer<RoleClient>>,
    ) -> MessengerResult {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::ListPromptsResult {
                server_name: self.server_name.clone(),
                result,
                peer,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    async fn send_resources_list_result(
        &self,
        result: Result<ListResourcesResult>,
        peer: Option<Peer<RoleClient>>,
    ) -> MessengerResult {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::ListResourcesResult {
                server_name: self.server_name.clone(),
                result,
                peer,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    async fn send_resource_templates_list_result(
        &self,
        result: Result<ListResourceTemplatesResult>,
        peer: Option<Peer<RoleClient>>,
    ) -> MessengerResult {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::ResourceTemplatesListResult {
                server_name: self.server_name.clone(),
                result,
                peer,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    async fn send_oauth_link(&self, link: String) -> MessengerResult {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::OauthLink {
                server_name: self.server_name.clone(),
                link,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    async fn send_init_msg(&self) -> MessengerResult {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::InitStart {
                server_name: self.server_name.clone(),
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    fn send_deinit_msg(&self) {
        let sender = self.update_event_sender.clone();
        let server_name = self.server_name.clone();
        tokio::spawn(async move {
            let _ = sender.send(UpdateEventMessage::Deinit { server_name }).await;
        });
    }

    fn duplicate(&self) -> Box<dyn Messenger> {
        Box::new(self.clone())
    }
}
