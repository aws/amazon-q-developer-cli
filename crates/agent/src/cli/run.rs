use std::io::Write as _;
use std::process::ExitCode;

use clap::Args;
use eyre::{
    Result,
    bail,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::agent::Agent;
use crate::agent::agent_config::load_agents;
use crate::agent::agent_loop::protocol::{
    AgentLoopEventKind,
    UserTurnMetadata,
};
use crate::agent::protocol::{
    AgentEvent,
    ApprovalResult,
    InputItem,
    SendApprovalResultArgs,
    SendPromptArgs,
};

// use crate::chat::{
//     ActiveState,
//     ApprovalResult,
//     InputItem,
//     SendApprovalResultArgs,
//     SendPromptArgs,
//     Session,
//     SessionBuilder,
//     SessionEvent,
//     SessionEventKind,
//     SessionInitWarning,
//     SessionNotification,
// };

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
            self.handle_output(&evt).await?;

            // Check for exit conditions
            match &evt {
                AgentEvent::AgentLoop(evt) => match &evt.kind {
                    AgentLoopEventKind::UserTurnEnd(_) => {
                        break;
                    },
                    _ => (),
                },
                AgentEvent::RequestError(loop_error) => bail!("agent encountered an error: {:?}", loop_error),
                AgentEvent::ApprovalRequest { id, tool_use, context } => {
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

    // pub async fn execute(self) -> Result<ExitCode> {
    //     let initial_prompt = self.prompt.join(" ");
    //
    //     let (session, warnings) = self.init_session().await?;
    //     if !warnings.is_empty() {
    //         warn!(?warnings, "Warnings from initializing the session");
    //     }
    //
    //     let agents = session.agents().cloned().collect::<Vec<_>>();
    //     debug!(?agents, "session spawned with agents");
    //     let agent_id = match self.agent.as_ref() {
    //         Some(name) => agents
    //             .iter()
    //             .find(|id| id.name() == name.as_str())
    //             .ok_or_eyre("session missing agent")?
    //             .clone(),
    //         None => agents.first().expect("session should have an agent").clone(),
    //     };
    //
    //     let mut handle = session.spawn().await;
    //
    //     handle
    //         .send_prompt(SendPromptArgs {
    //             agent_id: agent_id.clone(),
    //             content: vec![InputItem::Text(initial_prompt)],
    //         })
    //         .await?;
    //
    //     loop {
    //         let Ok(res) = handle.recv().await else {
    //             bail!("channel closed");
    //         };
    //
    //         // First, handle output displaying.
    //         self.handle_output(&res).await?;
    //
    //         // Then, check for exit conditions.
    //         match &res.kind {
    //             SessionEventKind::Notification(notif) => match notif {
    //                 SessionNotification::ApprovalRequest { id, tool_use, .. } => {
    //                     if !self.dangerously_trust_all_tools {
    //                         bail!("Tool approval is required: {:?}", tool_use);
    //                     } else {
    //                         warn!(?tool_use, "trust all is enabled, ignoring approval request");
    //                         handle
    //                             .send_tool_use_approval_result(SendApprovalResultArgs {
    //                                 agent_id: agent_id.clone(),
    //                                 id: id.clone(),
    //                                 result: ApprovalResult::Approve,
    //                             })
    //                             .await?;
    //                     }
    //                 },
    //             },
    //             SessionEventKind::AgentRuntime(ev) => {
    //                 if let RuntimeEvent::AgentLoopError { id, error } = ev {
    //                     bail!(
    //                         "Encountered an error running the agent loop for agent '{}': {:?}",
    //                         id.agent_id(),
    //                         error
    //                     );
    //                 }
    //             },
    //             SessionEventKind::AgentStateChange { to, .. } => match &to.active_state {
    //                 ActiveState::Idle => {
    //                     break;
    //                 },
    //                 ActiveState::Errored => {
    //                     error!("agent encountered an error");
    //                     break;
    //                 },
    //                 _ => (),
    //             },
    //             _ => (),
    //         }
    //     }
    //
    //     if let Ok(snapshot) = handle.export_snapshot().await {
    //         let _ = tokio::fs::write("snapshot.json",
    // serde_json::to_string_pretty(&snapshot)?).await;     }
    //
    //     Ok(ExitCode::SUCCESS)
    // }
    //
    // async fn init_session(&self) -> Result<(Session, Vec<SessionInitWarning>)> {
    //     let mut builder = SessionBuilder::new();
    //
    //     if let Some(id) = self.resume.as_ref() {
    //         builder.from_id(id).await?;
    //     }
    //
    //     if let Some(agent) = self.agent.as_ref() {
    //         builder.with_agent(agent.clone());
    //     }
    //
    //     if let Some(model) = self.model.as_ref() {
    //         builder.with_model(model.clone());
    //     }
    //
    //     builder.build().await
    // }
    fn output_format(&self) -> OutputFormat {
        self.output_format.unwrap_or(OutputFormat::Text)
    }

    async fn handle_output(&self, evt: &AgentEvent) -> Result<()> {
        match self.output_format() {
            OutputFormat::Text => {
                if let AgentEvent::AgentLoop(evt) = &evt {
                    match &evt.kind {
                        AgentLoopEventKind::AssistantText(text) => {
                            print!("{}", text);
                            std::io::stdout().flush();
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
            OutputFormat::JsonStreaming => Ok(()),
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
