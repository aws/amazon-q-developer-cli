use crossterm::style::Color;
use crossterm::{
    execute,
    style,
};
use rmcp::model::{
    CreateElicitationRequestParam,
    CreateElicitationResult,
    ElicitationAction,
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

    async fn handle_elicitation_request(
        &self,
        request: CreateElicitationRequestParam,
    ) -> core::result::Result<CreateElicitationResult, MessengerError> {
        use std::io::{
            self,
            Write,
        };

        let _ = execute!(
            std::io::stdout(),
            style::Print("\nMCP server "),
            style::SetForegroundColor(Color::Magenta),
            style::Print(&self.server_name),
            style::SetForegroundColor(Color::Reset),
            style::Print(" is requesting information\n")
        );
        println!("{}", request.message);
        println!();

        let mut content = std::collections::HashMap::new();

        if let Some(properties) = request.requested_schema.get("properties").and_then(|p| p.as_object()) {
            for (key, _property) in properties {
                print!("  {}: ", key);
                if let Err(e) = io::stdout().flush() {
                    return Err(MessengerError::Custom(e.to_string()));
                }

                let mut input = String::new();
                if let Err(e) = io::stdin().read_line(&mut input) {
                    return Err(MessengerError::Custom(e.to_string()));
                }
                let input = input.trim();

                if !input.is_empty() {
                    content.insert(key.clone(), serde_json::Value::String(input.to_string()));
                }
            }
        }

        let _ = execute!(
            io::stdout(),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("\nSubmit this information? ["),
            style::SetForegroundColor(Color::Green),
            style::Print("y"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("/"),
            style::SetForegroundColor(Color::Green),
            style::Print("n"),
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("]: "),
            style::SetForegroundColor(Color::Reset),
        );

        if let Err(e) = io::stdout().flush() {
            return Err(MessengerError::Custom(e.to_string()));
        }

        let mut confirmation = String::new();
        if let Err(e) = io::stdin().read_line(&mut confirmation) {
            return Err(MessengerError::Custom(e.to_string()));
        }

        let action = match confirmation.trim().to_lowercase().as_str() {
            "y" | "yes" => ElicitationAction::Accept,
            "n" | "no" => ElicitationAction::Decline,
            "" => ElicitationAction::Cancel,
            _ => ElicitationAction::Cancel,
        };

        let content_value = if matches!(action, ElicitationAction::Accept) {
            Some(serde_json::to_value(content).unwrap_or_default())
        } else {
            None
        };

        Ok(CreateElicitationResult {
            action,
            content: content_value,
        })
    }

    fn duplicate(&self) -> Box<dyn Messenger> {
        Box::new(self.clone())
    }
}
