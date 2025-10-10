//! Unused code

#[derive(Debug, Clone)]
pub struct ConfigHandle {
    /// Sender for sending requests to the tool manager task
    sender: RequestSender<AgentConfigRequest, AgentConfigResponse, AgentConfigError>,
}

impl ConfigHandle {
    pub async fn get_config(&self, agent_name: &str) -> Result<AgentConfig, AgentConfigError> {
        match self
            .sender
            .send_recv(AgentConfigRequest::GetConfig {
                agent_name: agent_name.to_string(),
            })
            .await
            .unwrap_or(Err(AgentConfigError::Channel))?
        {
            AgentConfigResponse::Config(agent_config) => Ok(agent_config),
            other => {
                error!(?other, "received unexpected response");
                Err(AgentConfigError::Custom("received unexpected response".to_string()))
            },
        }
    }
}

#[derive(Debug)]
pub struct AgentConfigManager {
    configs: Vec<AgentConfig>,

    request_tx: RequestSender<AgentConfigRequest, AgentConfigResponse, AgentConfigError>,
    request_rx: RequestReceiver<AgentConfigRequest, AgentConfigResponse, AgentConfigError>,
}

impl AgentConfigManager {
    pub fn new() -> Self {
        let (request_tx, request_rx) = new_request_channel();
        Self {
            configs: Vec::new(),
            request_tx,
            request_rx,
        }
    }

    pub async fn spawn(mut self) -> Result<(ConfigHandle, Vec<AgentConfigError>)> {
        let request_tx_clone = self.request_tx.clone();

        // TODO - return errors back.
        let (configs, errors) = load_agents().await?;
        self.configs = configs;

        tokio::spawn(async move {
            self.run().await;
        });

        Ok((
            ConfigHandle {
                sender: request_tx_clone,
            },
            errors,
        ))
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                req = self.request_rx.recv() => {
                    let Some(req) = req else {
                        warn!("Agent config request channel has closed, exiting");
                        break;
                    };
                    let res = self.handle_agent_config_request(req.payload).await;
                    respond!(req, res);
                }
            }
        }
    }

    async fn handle_agent_config_request(
        &mut self,
        req: AgentConfigRequest,
    ) -> Result<AgentConfigResponse, AgentConfigError> {
        match req {
            AgentConfigRequest::GetConfig { agent_name } => {
                let agent_config = self
                    .configs
                    .iter()
                    .find_map(|a| {
                        if a.config.name() == agent_name {
                            Some(a.clone())
                        } else {
                            None
                        }
                    })
                    .ok_or(AgentConfigError::AgentNotFound { name: agent_name })?;
                Ok(AgentConfigResponse::Config(agent_config))
            },
            AgentConfigRequest::GetAllConfigs => {
                todo!()
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentConfigRequest {
    GetConfig { agent_name: String },
    GetAllConfigs,
}

#[derive(Debug, Clone)]
pub enum AgentConfigResponse {
    Config(AgentConfig),
    AllConfigs {
        configs: Vec<AgentConfig>,
        invalid_configs: Vec<()>,
    },
}
