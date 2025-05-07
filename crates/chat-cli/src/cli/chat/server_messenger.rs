use tokio::sync::mpsc::{
    Receiver,
    Sender,
    channel,
};

use crate::mcp_client::{
    Messenger,
    MessengerError,
    PromptsListResult,
    ResourceTemplatesListResult,
    ResourcesListResult,
    ToolsListResult,
};

#[derive(Clone, Debug)]
pub enum UpdateEventMessage {
    ToolsListResult {
        server_name: String,
        result: ToolsListResult,
    },
    PromptsListResult {
        server_name: String,
        result: PromptsListResult,
    },
    ResourcesListResult {
        server_name: String,
        result: ResourcesListResult,
    },
    ResourceTemplatesListResult {
        server_name: String,
        result: ResourceTemplatesListResult,
    },
    DisplayTaskEnded,
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
    async fn send_tools_list_result(&self, result: ToolsListResult) -> Result<(), MessengerError> {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::ToolsListResult {
                server_name: self.server_name.clone(),
                result,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    async fn send_prompts_list_result(&self, result: PromptsListResult) -> Result<(), MessengerError> {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::PromptsListResult {
                server_name: self.server_name.clone(),
                result,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    async fn send_resources_list_result(&self, result: ResourcesListResult) -> Result<(), MessengerError> {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::ResourcesListResult {
                server_name: self.server_name.clone(),
                result,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    async fn send_resource_templates_list_result(
        &self,
        result: ResourceTemplatesListResult,
    ) -> Result<(), MessengerError> {
        Ok(self
            .update_event_sender
            .send(UpdateEventMessage::ResourceTemplatesListResult {
                server_name: self.server_name.clone(),
                result,
            })
            .await
            .map_err(|e| MessengerError::Custom(e.to_string()))?)
    }

    fn duplicate(&self) -> Box<dyn Messenger> {
        Box::new(self.clone())
    }
}
