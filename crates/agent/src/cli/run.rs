use std::io::Write as _;
use std::process::ExitCode;

use agent::agent::Agent;
use agent::agent::agent_config::load_agents;
use agent::agent::agent_loop::protocol::{
    AgentLoopEventKind,
    UserTurnMetadata,
};
use agent::agent::protocol::{
    AgentEvent,
    ApprovalResult,
    InputItem,
    SendApprovalResultArgs,
    SendPromptArgs,
};
use clap::Args;
use eyre::{
    Result,
    bail,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::warn;

#[derive(Debug, Clone, Default, Args)]
pub struct RunArgs {
    /// The name of the agent to run the session with.
    #[arg(long)]
    agent: Option<String>,
    /// The id of the model to use.
    #[arg(long)]
    model: Option<String>,
    /// Resumes the session given by the provided ID
    #[arg(short, long)]
    resume: Option<String>,
    /// The output format
    #[arg(long)]
    output_format: Option<OutputFormat>,
    /// Trust all tools
    #[arg(long)]
    dangerously_trust_all_tools: bool,
    /// The initial prompt.
    prompt: Vec<String>,
}

impl RunArgs {
    pub async fn execute(self) -> Result<ExitCode> {
        let initial_prompt = self.prompt.join(" ");

        let (configs, _) = load_agents().await?;
        let mut agent = match &self.agent {
            Some(name) => {
                if let Some(cfg) = configs.iter().find(|c| c.name() == name.as_str()) {
                    Agent::from_config(cfg.config().clone()).await?.spawn()
                } else {
                    warn!(?name, "unable to find agent with name");
                    Agent::new_default().await?.spawn()
                }
            },
            _ => Agent::new_default().await?.spawn(),
        };

        while let Ok(evt) = agent.recv().await {
            if matches!(evt, AgentEvent::Initialized) {
                break;
            }
        }

        agent
            .send_prompt(SendPromptArgs {
                content: vec![InputItem::Text(initial_prompt)],
            })
            .await?;

        loop {
            let Ok(evt) = agent.recv().await else {
                bail!("channel closed");
            };

            // First, print output
            self.handle_output_format_printing(&evt).await?;

            // Check for exit conditions
            match &evt {
                AgentEvent::AgentLoop(evt) => {
                    if let AgentLoopEventKind::UserTurnEnd(_) = &evt.kind {
                        break;
                    }
                },
                AgentEvent::RequestError(loop_error) => bail!("agent encountered an error: {:?}", loop_error),
                AgentEvent::ApprovalRequest { id, tool_use, .. } => {
                    if !self.dangerously_trust_all_tools {
                        bail!("Tool approval is required: {:?}", tool_use);
                    } else {
                        warn!(?tool_use, "trust all is enabled, ignoring approval request");
                        agent
                            .send_tool_use_approval_result(SendApprovalResultArgs {
                                id: id.clone(),
                                result: ApprovalResult::Approve,
                            })
                            .await?;
                    }
                },
                _ => (),
            }
        }

        Ok(ExitCode::SUCCESS)
    }

    fn output_format(&self) -> OutputFormat {
        self.output_format.unwrap_or(OutputFormat::Text)
    }

    async fn handle_output_format_printing(&self, evt: &AgentEvent) -> Result<()> {
        match self.output_format() {
            OutputFormat::Text => {
                if let AgentEvent::AgentLoop(evt) = &evt {
                    match &evt.kind {
                        AgentLoopEventKind::AssistantText(text) => {
                            print!("{}", text);
                            let _ = std::io::stdout().flush();
                        },
                        AgentLoopEventKind::ToolUse(tool_use) => {
                            print!("\n{}\n", serde_json::to_string_pretty(tool_use).expect("does not fail"));
                        },
                        _ => (),
                    }
                }
                Ok(())
            },
            OutputFormat::Json => Ok(()), // output will be dealt with after exiting the main loop
            OutputFormat::JsonStreaming => {
                if let AgentEvent::AgentLoop(evt) = &evt {
                    match &evt.kind {
                        AgentLoopEventKind::StreamEvent(stream_event) => {
                            println!("{}", serde_json::to_string(stream_event)?);
                        },
                        AgentLoopEventKind::StreamError(stream_error) => {
                            println!("{}", serde_json::to_string(stream_error)?);
                        },
                        _ => (),
                    }
                }
                Ok(())
            },
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
enum OutputFormat {
    Text,
    Json,
    JsonStreaming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonOutput {
    result: String,
    metadata: UserTurnMetadata,
}
