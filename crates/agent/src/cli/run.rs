use std::io::Write as _;
use std::process::ExitCode;
use std::sync::Arc;

use agent::agent_config::load_agents;
use agent::agent_loop::protocol::{
    AgentLoopEventKind,
    EndReason,
};
use agent::api_client::ApiClient;
use agent::mcp::McpManager;
use agent::protocol::{
    AgentEvent,
    ApprovalResult,
    InputItem,
    SendApprovalResultArgs,
    SendPromptArgs,
};
use agent::rts::{
    RtsModel,
    RtsModelState,
};
use agent::types::AgentSnapshot;
use agent::{
    Agent,
    AgentHandle,
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
use tracing::{
    debug,
    error,
    info,
    warn,
};

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
        // TODO - implement resume. For now, just use a new default snapshot every time.
        let mut snapshot = AgentSnapshot::default();

        // Create the RTS model
        let model = {
            let rts_state: RtsModelState = snapshot
                .model_state
                .as_ref()
                .and_then(|s| {
                    serde_json::from_value(s.clone())
                        .map_err(|err| error!(?err, ?s, "failed to deserialize RTS state"))
                        .ok()
                })
                .unwrap_or({
                    let state = RtsModelState::new();
                    info!(?state.conversation_id, "generated new conversation id");
                    state
                });
            Arc::new(RtsModel::new(
                ApiClient::new().await?,
                rts_state.conversation_id,
                rts_state.model_id,
            ))
        };

        // Override the agent config if a custom agent name was provided.
        if let Some(name) = &self.agent {
            let (configs, _) = load_agents().await?;
            if let Some(cfg) = configs.into_iter().find(|c| c.name() == name.as_str()) {
                snapshot.agent_config = cfg.config().clone();
            } else {
                bail!("unable to find agent with name: {}", name);
            }
        };

        let agent = Agent::new(snapshot, model, McpManager::new().spawn()).await?.spawn();

        self.main_loop(agent).await
    }

    async fn main_loop(&self, mut agent: AgentHandle) -> Result<ExitCode> {
        let initial_prompt = self.prompt.join(" ");

        // First, wait for agent initialization
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

        // Holds the final result of the user turn.
        #[allow(unused_assignments)]
        let mut user_turn_metadata = None;

        loop {
            let Ok(evt) = agent.recv().await else {
                bail!("channel closed");
            };
            debug!(?evt, "received new agent event");

            // First, print output
            self.handle_output_format_printing(&evt).await?;

            // Check for exit conditions
            match &evt {
                AgentEvent::AgentLoop(evt) => {
                    if let AgentLoopEventKind::UserTurnEnd(metadata) = &evt.kind {
                        user_turn_metadata = Some(metadata.clone());
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

        if self.output_format == Some(OutputFormat::Json) {
            let md = user_turn_metadata.expect("user turn metadata should exist");
            let is_error = md.end_reason != EndReason::UserTurnEnd || md.result.as_ref().is_none_or(|v| v.is_err());
            let result = md.result.and_then(|r| r.ok().map(|m| m.text()));

            let output = JsonOutput {
                result,
                is_error,
                number_of_requests: md.total_request_count,
                number_of_cycles: md.number_of_cycles,
                duration_ms: md.turn_duration.map(|d| d.as_millis() as u32).unwrap_or_default(),
            };
            println!("{}", serde_json::to_string(&output)?);
        }

        Ok(ExitCode::SUCCESS)
    }

    async fn handle_output_format_printing(&self, evt: &AgentEvent) -> Result<()> {
        match self.output_format.unwrap_or(OutputFormat::Text) {
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
                    if let AgentLoopEventKind::Stream(stream_event) = &evt.kind {
                        println!("{}", serde_json::to_string(stream_event)?);
                    }
                }
                Ok(())
            },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, strum::EnumString)]
#[strum(serialize_all = "kebab-case")]
enum OutputFormat {
    Text,
    Json,
    JsonStreaming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonOutput {
    /// Whether or not the user turn completed successfully
    is_error: bool,
    /// Text from the final message, if available
    result: Option<String>,
    /// The number of requests sent to the model
    number_of_requests: u32,
    /// The number of tool use / tool result pairs in the turn
    ///
    /// This could be less than the number of requests in the case of retries
    number_of_cycles: u32,
    /// Duration of the turn, in milliseconds
    duration_ms: u32,
}
