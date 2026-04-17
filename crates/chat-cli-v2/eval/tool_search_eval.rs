//! End-to-end evaluation of the ToolSearch feature.
//!
//! Loads test scenarios from an eval_results.json file and runs them through a real agent.
//! Records ToolSearch calls, MCP tool calls, and the final response for each scenario.
//!
//! Usage:
//!   cargo run -p chat_cli_v2 --example tool_search_eval -- \
//!     --agent <agent-name> [--scenario-id cat1_exact_intent] [--runs 10]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent::Agent;
use agent::agent_config::load_agents;
use agent::mcp::McpManager;
use agent::protocol::{
    AgentEvent,
    AgentStopReason,
    ApprovalResult,
    ContentChunk,
    PermissionOptionId,
    SendApprovalResultArgs,
    SendPromptArgs,
    UpdateEvent,
};
use agent::tools::{
    BuiltInTool,
    ToolKind,
};
use agent::types::AgentSnapshot;
use agent::util::providers::RealProvider;
use chat_cli_v2::agent::rts::{
    RtsModel,
    RtsState,
};
use chat_cli_v2::api_client::ApiClient;
use chat_cli_v2::database::Database;
use chat_cli_v2::os::{
    Env,
    Fs,
};
use clap::Parser;
use eyre::{
    Result,
    bail,
};
use serde::{
    Deserialize,
    Serialize,
};

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

#[derive(Debug, Parser)]
struct Args {
    /// Name of the agent config to load (must exist in ~/.kiro/agents or .kiro/agents)
    #[arg(long)]
    agent: String,

    /// Path to the eval scenarios JSON file
    #[arg(long, default_value = "crates/chat-cli-v2/eval/eval_scenarios.json")]
    scenarios: PathBuf,

    /// Run only the scenario with this id (e.g. cat1_exact_intent). Runs all if omitted.
    #[arg(long)]
    scenario_id: Option<String>,

    /// Number of times to run each scenario (default: 1)
    #[arg(long, default_value = "1")]
    runs: usize,
}

// ---------------------------------------------------------------------------
// Scenario input types (from eval_results.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct EvalSuite {
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    id: String,
    query: String,
    #[serde(default)]
    expected_tool: Option<String>,
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolSearchCall {
    tool_id: Option<String>,
    query: Option<String>,
    tools_returned: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvalResult {
    scenario_id: String,
    run: usize,
    query: String,
    expected_tool: Option<String>,
    tool_search_calls: Vec<ToolSearchCall>,
    mcp_tool_calls: Vec<String>,
    final_response: Option<String>,
    error: Option<String>,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let suite: EvalSuite = serde_json::from_str(&std::fs::read_to_string(&args.scenarios)?)?;
    let scenarios: Vec<Scenario> = match &args.scenario_id {
        Some(id) => {
            let s = suite
                .scenarios
                .into_iter()
                .find(|s| &s.id == id)
                .ok_or_else(|| eyre::eyre!("scenario '{}' not found", id))?;
            vec![s]
        },
        None => suite.scenarios,
    };

    let (configs, _) = load_agents(&RealProvider).await?;
    let agent_config = configs
        .into_iter()
        .find(|c| c.name() == args.agent.as_str())
        .ok_or_else(|| eyre::eyre!("agent '{}' not found", args.agent))?;

    println!(
        "Running {} scenario(s) x {} run(s) with agent '{}'",
        scenarios.len(),
        args.runs,
        args.agent
    );

    let output_dir = PathBuf::from("crates/chat-cli-v2/eval/output");
    std::fs::create_dir_all(&output_dir)?;

    for run in 1..=args.runs {
        for scenario in &scenarios {
            let output_path = output_dir.join(format!("{}.jsonl", scenario.id));
            println!(
                "\n--- [{}] run {}/{} | {} ---",
                scenario.id, run, args.runs, scenario.query
            );
            let result = match run_query(&scenario.query, agent_config.clone()).await {
                Ok(r) => {
                    println!("  ToolSearch calls: {}", r.tool_search_calls.len());
                    println!("  MCP tool calls: {:?}", r.mcp_tool_calls);
                    println!("  Response: {}", r.final_response.as_deref().unwrap_or("<none>"));
                    EvalResult {
                        scenario_id: scenario.id.clone(),
                        run,
                        expected_tool: scenario.expected_tool.clone(),
                        ..r
                    }
                },
                Err(e) => {
                    println!("  ERROR: {e}");
                    EvalResult {
                        scenario_id: scenario.id.clone(),
                        run,
                        query: scenario.query.clone(),
                        expected_tool: scenario.expected_tool.clone(),
                        tool_search_calls: vec![],
                        mcp_tool_calls: vec![],
                        final_response: None,
                        error: Some(e.to_string()),
                    }
                },
            };

            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&output_path)?;
            serde_json::to_writer(&mut f, &result)?;
            writeln!(f)?;
        }
    }

    println!("\nResults written to {}/", output_dir.display());

    Ok(())
}

// ---------------------------------------------------------------------------
// Per-query agent run
// ---------------------------------------------------------------------------

async fn run_query(query: &str, agent_config: agent::agent_config::LoadedAgentConfig) -> Result<EvalResult> {
    let mut database = Database::new().await?;
    let api_client = ApiClient::new(&Env::new(), &Fs::new(), &mut database, None).await?;
    let rts_state = Arc::new(RtsState::new(uuid::Uuid::new_v4().to_string()));
    let model = Arc::new(RtsModel::new(api_client, rts_state));

    let mut snapshot = AgentSnapshot::new_empty(agent_config);
    snapshot.settings.tool_search_enabled = true;
    // Disable thresholds so tool search always activates regardless of spec size
    snapshot.settings.tool_search_min_pct = None;
    snapshot.settings.tool_search_min_tokens = None;

    let global_mcp_path = dirs::home_dir().map(|h| h.join(".kiro").join("settings").join("mcp.json"));

    let mut agent = Agent::new(
        snapshot,
        None, // local_mcp_path
        global_mcp_path.as_ref(),
        model,
        McpManager::default().spawn(),
        false,      // is_subagent
        None,       // code_intelligence
        None,       // knowledge_provider
        None,       // task_store
        Vec::new(), // available_agent_configs
    )
    .await?
    .spawn();

    // Wait for agent initialization, then wait for all MCP servers to finish initializing.
    // We track which servers are still pending and wait until all have either succeeded or failed.
    loop {
        match agent.recv().await? {
            AgentEvent::Initialized => break,
            AgentEvent::Stop(AgentStopReason::Error(e)) => bail!("init error: {e}"),
            _ => {},
        }
    }
    // Drain MCP events: wait until no new Mcp event arrives within 2s.
    // This ensures all MCP servers (including slow ones like builder-mcp) have finished
    // listing their tools before we send the prompt.
    loop {
        match tokio::time::timeout(Duration::from_secs(2), agent.recv()).await {
            Ok(Ok(AgentEvent::Mcp(_))) => {},
            Ok(Ok(AgentEvent::InitializeUpdate(_))) => {},
            Ok(Ok(AgentEvent::Stop(AgentStopReason::Error(e)))) => bail!("init error: {e}"),
            _ => break, // timeout or non-MCP event — MCP is settled
        }
    }

    agent
        .send_prompt(SendPromptArgs {
            content: vec![ContentChunk::Text(query.to_string())],
            should_continue_turn: None,
        })
        .await?;

    let mut tool_search_calls: Vec<ToolSearchCall> = Vec::new();
    let mut mcp_tool_calls: Vec<String> = Vec::new();
    // Pending ToolSearch calls by id, waiting for ToolCallFinished to extract returned tools
    let mut pending_tool_searches: std::collections::HashMap<String, (Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    let mut final_response: Option<String> = None;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);

    loop {
        let evt = tokio::time::timeout_at(deadline, agent.recv()).await??;

        match &evt {
            AgentEvent::Update(UpdateEvent::ToolCall(tc)) => match &tc.tool.kind {
                ToolKind::BuiltIn(BuiltInTool::ToolSearch(lt)) => {
                    println!("  [tool_search] tool_id={:?} query={:?}", lt.tool_id, lt.query);
                    pending_tool_searches.insert(tc.id.clone(), (lt.tool_id.clone(), lt.query.clone()));
                },
                ToolKind::Mcp(mcp) => {
                    println!(
                        "  [mcp_call] {}::{} input={}",
                        mcp.server_name, mcp.tool_name, tc.tool_use_block.input
                    );
                    mcp_tool_calls.push(format!("{}::{}", mcp.server_name, mcp.tool_name));
                },
                _ => {},
            },

            AgentEvent::Update(UpdateEvent::ToolCallFinished { tool_call, result }) => {
                if let Some((tool_id, query_param)) = pending_tool_searches.remove(&tool_call.id) {
                    let call = ToolSearchCall {
                        tool_id,
                        query: query_param,
                        tools_returned: extract_tool_search_results(result),
                    };
                    println!("  [tool_search] returned: {:?}", call.tools_returned);
                    tool_search_calls.push(call);
                }
            },

            AgentEvent::EndTurn(meta) => {
                final_response = meta.result.as_ref().and_then(|r| r.as_ref().ok()).map(|m| m.text());
                break;
            },

            AgentEvent::Stop(AgentStopReason::Error(e)) => bail!("agent error: {e}"),
            AgentEvent::Stop(_) => break,

            AgentEvent::ApprovalRequest(req) => {
                agent
                    .send_tool_use_approval_result(SendApprovalResultArgs {
                        id: req.id.clone(),
                        result: ApprovalResult {
                            option_id: PermissionOptionId::AllowOnce,
                            reason: None,
                            trust_option: None,
                        },
                    })
                    .await?;
            },

            _ => {},
        }
    }

    Ok(EvalResult {
        scenario_id: String::new(),
        run: 0,
        query: query.to_string(),
        expected_tool: None,
        tool_search_calls,
        mcp_tool_calls,
        final_response,
        error: None,
    })
}

/// Extract the list of tool names from a ToolSearch result JSON.
fn extract_tool_search_results(result: &agent::protocol::ToolCallResult) -> Vec<String> {
    let text = match result {
        agent::protocol::ToolCallResult::Success(output) => output
            .items
            .iter()
            .filter_map(|item| match item {
                agent::tools::ToolExecutionOutputItem::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => return vec![],
    };

    // ToolSearch returns {"tools": [{"server_name": "...", "tool_name": "...", ...}]}
    #[derive(Deserialize)]
    struct ToolSearchResponse {
        tools: Vec<ToolEntry>,
    }
    #[derive(Deserialize)]
    struct ToolEntry {
        server_name: String,
        tool_name: String,
    }

    serde_json::from_str::<ToolSearchResponse>(&text)
        .map(|r| {
            r.tools
                .iter()
                .map(|t| format!("{}::{}", t.server_name, t.tool_name))
                .collect()
        })
        .unwrap_or_default()
}
