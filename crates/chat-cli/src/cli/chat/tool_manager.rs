use std::borrow::Borrow;
use std::collections::{
    HashMap,
    HashSet,
};
use std::future::Future;
use std::hash::{
    DefaultHasher,
    Hasher,
};
use std::io::{
    BufWriter,
    Write,
};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::{
    Arc,
    RwLock as SyncRwLock,
};
use std::time::{
    Duration,
    Instant,
};

use crossterm::{
    cursor,
    execute,
    queue,
    style,
    terminal,
};
use eyre::Report;
use futures::{
    StreamExt,
    future,
    stream,
};
use regex::Regex;
use tokio::signal::ctrl_c;
use tokio::sync::{
    Mutex,
    Notify,
    RwLock,
};
use tokio::task::JoinHandle;
use tracing::{
    error,
    warn,
};

use crate::api_client::model::{
    ToolResult,
    ToolResultContentBlock,
    ToolResultStatus,
};
use crate::cli::agent::{
    Agent,
    McpServerConfig,
};
use crate::cli::chat::cli::prompts::GetPromptError;
use crate::cli::chat::consts::DUMMY_TOOL_NAME;
use crate::cli::chat::message::AssistantToolUse;
use crate::cli::chat::server_messenger::{
    ServerMessengerBuilder,
    UpdateEventMessage,
};
use crate::cli::chat::tools::custom_tool::{
    CustomTool,
    CustomToolClient,
};
use crate::cli::chat::tools::execute::ExecuteCommand;
use crate::cli::chat::tools::fs_read::FsRead;
use crate::cli::chat::tools::fs_write::FsWrite;
use crate::cli::chat::tools::gh_issue::GhIssue;
use crate::cli::chat::tools::knowledge::Knowledge;
use crate::cli::chat::tools::thinking::Thinking;
use crate::cli::chat::tools::use_aws::UseAws;
use crate::cli::chat::tools::{
    Tool,
    ToolOrigin,
    ToolSpec,
};
use crate::database::Database;
use crate::database::settings::Setting;
use crate::mcp_client::{
    JsonRpcResponse,
    Messenger,
    PromptGet,
};
use crate::os::Os;
use crate::telemetry::TelemetryThread;
use crate::util::MCP_SERVER_TOOL_DELIMITER;
use crate::util::directories::home_dir;

const NAMESPACE_DELIMITER: &str = "___";
// This applies for both mcp server and tool name since in the end the tool name as seen by the
// model is just {server_name}{NAMESPACE_DELIMITER}{tool_name}
const VALID_TOOL_NAME: &str = "^[a-zA-Z][a-zA-Z0-9_]*$";
const SPINNER_CHARS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn workspace_mcp_config_path(os: &Os) -> eyre::Result<PathBuf> {
    Ok(os.env.current_dir()?.join(".amazonq").join("mcp.json"))
}

pub fn global_mcp_config_path(os: &Os) -> eyre::Result<PathBuf> {
    Ok(home_dir(os)?.join(".aws").join("amazonq").join("mcp.json"))
}

/// Messages used for communication between the tool initialization thread and the loading
/// display thread. These messages control the visual loading indicators shown to
/// the user during tool initialization.
enum LoadingMsg {
    /// Indicates a tool has finished initializing successfully and should be removed from
    /// the loading display. The String parameter is the name of the tool that
    /// completed initialization.
    Done { name: String, time: String },
    /// Represents an error that occurred during tool initialization.
    /// Contains the name of the server that failed to initialize and the error message.
    Error {
        name: String,
        msg: eyre::Report,
        time: String,
    },
    /// Represents a warning that occurred during tool initialization.
    /// Contains the name of the server that generated the warning and the warning message.
    Warn {
        name: String,
        msg: eyre::Report,
        time: String,
    },
    /// Signals that the loading display thread should terminate.
    /// This is sent when all tool initialization is complete or when the application is shutting
    /// down.
    Terminate { still_loading: Vec<String> },
}

/// Used to denote the loading outcome associated with a server.
/// This is mainly used in the non-interactive mode to determine if there is any fatal errors to
/// surface (since we would only want to surface fatal errors in non-interactive mode).
#[derive(Clone, Debug)]
pub enum LoadingRecord {
    Success(String),
    Warn(String),
    Err(String),
}

#[derive(Default)]
pub struct ToolManagerBuilder {
    prompt_list_sender: Option<std::sync::mpsc::Sender<Vec<String>>>,
    prompt_list_receiver: Option<std::sync::mpsc::Receiver<Option<String>>>,
    conversation_id: Option<String>,
    agent: Option<Agent>,
}

impl ToolManagerBuilder {
    pub fn prompt_list_sender(mut self, sender: std::sync::mpsc::Sender<Vec<String>>) -> Self {
        self.prompt_list_sender.replace(sender);
        self
    }

    pub fn prompt_list_receiver(mut self, receiver: std::sync::mpsc::Receiver<Option<String>>) -> Self {
        self.prompt_list_receiver.replace(receiver);
        self
    }

    pub fn conversation_id(mut self, conversation_id: &str) -> Self {
        self.conversation_id.replace(conversation_id.to_string());
        self
    }

    pub fn agent(mut self, agent: Agent) -> Self {
        self.agent.replace(agent);
        self
    }

    pub async fn build(
        mut self,
        os: &mut Os,
        mut output: Box<dyn Write + Send + Sync + 'static>,
        interactive: bool,
    ) -> eyre::Result<ToolManager> {
        let McpServerConfig { mcp_servers } = self.agent.as_ref().map(|a| a.mcp_servers.clone()).unwrap_or_default();
        debug_assert!(self.conversation_id.is_some());
        let conversation_id = self.conversation_id.ok_or(eyre::eyre!("Missing conversation id"))?;

        // Separate enabled and disabled servers
        let (enabled_servers, disabled_servers): (Vec<_>, Vec<_>) = mcp_servers
            .into_iter()
            .partition(|(_, server_config)| !server_config.disabled);

        // Prepare disabled servers for display
        let disabled_servers_display: Vec<String> = disabled_servers
            .iter()
            .map(|(server_name, _)| server_name.clone())
            .collect();

        let pre_initialized = enabled_servers
            .into_iter()
            .filter_map(|(server_name, server_config)| {
                if server_name.contains(MCP_SERVER_TOOL_DELIMITER) {
                    let _ = queue!(
                        output,
                        style::SetForegroundColor(style::Color::Red),
                        style::Print("✗ Invalid server name "),
                        style::SetForegroundColor(style::Color::Blue),
                        style::Print(&server_name),
                        style::ResetColor,
                        style::Print(". Server name cannot contain "),
                        style::SetForegroundColor(style::Color::Yellow),
                        style::Print(MCP_SERVER_TOOL_DELIMITER),
                        style::ResetColor,
                        style::Print("\n")
                    );
                    None
                } else if server_name == "builtin" {
                    let _ = queue!(
                        output,
                        style::SetForegroundColor(style::Color::Red),
                        style::Print("✗ Invalid server name "),
                        style::SetForegroundColor(style::Color::Blue),
                        style::Print(&server_name),
                        style::ResetColor,
                        style::Print(". Server name cannot contain reserved word "),
                        style::SetForegroundColor(style::Color::Yellow),
                        style::Print("builtin"),
                        style::ResetColor,
                        style::Print(" (it is used to denote native tools)\n")
                    );
                    None
                } else {
                    let custom_tool_client = CustomToolClient::from_config(server_name.clone(), server_config, os);
                    Some((server_name, custom_tool_client))
                }
            })
            .collect::<Vec<(String, _)>>();

        let mut loading_servers = HashMap::<String, Instant>::new();
        for (server_name, _) in &pre_initialized {
            let init_time = std::time::Instant::now();
            loading_servers.insert(server_name.clone(), init_time);
        }
        let total = loading_servers.len();

        // Spawn a task for displaying the mcp loading statuses.
        // This is only necessary when we are in interactive mode AND there are servers to load.
        // Otherwise we do not need to be spawning this.
        let (loading_display_task, loading_status_sender) = if interactive
            && (total > 0 || !disabled_servers.is_empty())
        {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<LoadingMsg>(50);
            let disabled_servers_display_clone = disabled_servers_display.clone();
            (
                Some(tokio::task::spawn(async move {
                    let mut spinner_logo_idx: usize = 0;
                    let mut complete: usize = 0;
                    let mut failed: usize = 0;

                    // Show disabled servers immediately
                    for server_name in &disabled_servers_display_clone {
                        queue_disabled_message(server_name, &mut output)?;
                    }

                    if total > 0 {
                        queue_init_message(spinner_logo_idx, complete, failed, total, &mut output)?;
                    }

                    loop {
                        match tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
                            Ok(Some(recv_result)) => match recv_result {
                                LoadingMsg::Done { name, time } => {
                                    complete += 1;
                                    execute!(
                                        output,
                                        cursor::MoveToColumn(0),
                                        cursor::MoveUp(1),
                                        terminal::Clear(terminal::ClearType::CurrentLine),
                                    )?;
                                    queue_success_message(&name, &time, &mut output)?;
                                    queue_init_message(spinner_logo_idx, complete, failed, total, &mut output)?;
                                },
                                LoadingMsg::Error { name, msg, time } => {
                                    failed += 1;
                                    execute!(
                                        output,
                                        cursor::MoveToColumn(0),
                                        cursor::MoveUp(1),
                                        terminal::Clear(terminal::ClearType::CurrentLine),
                                    )?;
                                    queue_failure_message(&name, &msg, time.as_str(), &mut output)?;
                                    queue_init_message(spinner_logo_idx, complete, failed, total, &mut output)?;
                                },
                                LoadingMsg::Warn { name, msg, time } => {
                                    complete += 1;
                                    execute!(
                                        output,
                                        cursor::MoveToColumn(0),
                                        cursor::MoveUp(1),
                                        terminal::Clear(terminal::ClearType::CurrentLine),
                                    )?;
                                    let msg = eyre::eyre!(msg.to_string());
                                    queue_warn_message(&name, &msg, time.as_str(), &mut output)?;
                                    queue_init_message(spinner_logo_idx, complete, failed, total, &mut output)?;
                                },
                                LoadingMsg::Terminate { still_loading } => {
                                    if !still_loading.is_empty() && total > 0 {
                                        execute!(
                                            output,
                                            cursor::MoveToColumn(0),
                                            cursor::MoveUp(1),
                                            terminal::Clear(terminal::ClearType::CurrentLine),
                                        )?;
                                        let msg = still_loading.iter().fold(String::new(), |mut acc, server_name| {
                                            acc.push_str(format!("\n - {server_name}").as_str());
                                            acc
                                        });
                                        let msg = eyre::eyre!(msg);
                                        queue_incomplete_load_message(complete, total, &msg, &mut output)?;
                                    } else if total > 0 {
                                        // Clear the loading line if we have enabled servers
                                        execute!(
                                            output,
                                            cursor::MoveToColumn(0),
                                            cursor::MoveUp(1),
                                            terminal::Clear(terminal::ClearType::CurrentLine),
                                        )?;
                                    }
                                    execute!(output, style::Print("\n"),)?;
                                    break;
                                },
                            },
                            Err(_e) => {
                                spinner_logo_idx = (spinner_logo_idx + 1) % SPINNER_CHARS.len();
                                execute!(
                                    output,
                                    cursor::SavePosition,
                                    cursor::MoveToColumn(0),
                                    cursor::MoveUp(1),
                                    style::Print(SPINNER_CHARS[spinner_logo_idx]),
                                    cursor::RestorePosition
                                )?;
                            },
                            _ => break,
                        }
                        output.flush()?;
                    }
                    Ok::<_, eyre::Report>(())
                })),
                Some(tx),
            )
        } else {
            (None, None)
        };

        let mut clients = HashMap::<String, Arc<CustomToolClient>>::new();
        let mut loading_status_sender_clone = loading_status_sender.clone();
        let conv_id_clone = conversation_id.clone();
        let regex = Regex::new(VALID_TOOL_NAME)?;
        let new_tool_specs = Arc::new(Mutex::new(HashMap::new()));
        let new_tool_specs_clone = new_tool_specs.clone();
        let has_new_stuff = Arc::new(AtomicBool::new(false));
        let has_new_stuff_clone = has_new_stuff.clone();
        let pending = Arc::new(RwLock::new(HashSet::<String>::new()));
        let pending_clone = pending.clone();
        let (mut msg_rx, messenger_builder) = ServerMessengerBuilder::new(20);
        let telemetry_clone = os.telemetry.clone();
        let notify = Arc::new(Notify::new());
        let notify_weak = Arc::downgrade(&notify);
        let load_record = Arc::new(Mutex::new(HashMap::<String, Vec<LoadingRecord>>::new()));
        let load_record_clone = load_record.clone();
        let agent = Arc::new(Mutex::new(self.agent.unwrap_or_default()));
        let agent_clone = agent.clone();
        let database = os.database.clone();

        tokio::spawn(async move {
            let mut record_temp_buf = Vec::<u8>::new();
            let mut initialized = HashSet::<String>::new();

            enum ToolFilter {
                All,
                List(HashSet<String>),
            }

            impl ToolFilter {
                pub fn should_include(&self, tool_name: &str) -> bool {
                    match self {
                        Self::All => true,
                        Self::List(set) => set.contains(tool_name),
                    }
                }
            }

            while let Some(msg) = msg_rx.recv().await {
                record_temp_buf.clear();
                // For now we will treat every list result as if they contain the
                // complete set of tools. This is not necessarily true in the future when
                // request method on the mcp client no longer buffers all the pages from
                // list calls.
                match msg {
                    UpdateEventMessage::ToolsListResult { server_name, result } => {
                        let time_taken = loading_servers
                            .remove(&server_name)
                            .map_or("0.0".to_owned(), |init_time| {
                                let time_taken = (std::time::Instant::now() - init_time).as_secs_f64().abs();
                                format!("{:.2}", time_taken)
                            });
                        pending_clone.write().await.remove(&server_name);
                        let (tool_filter, alias_list) = {
                            let agent_lock = agent_clone.lock().await;

                            // We will assume all tools are allowed if the tool list consists of 1
                            // element and it's a *
                            let tool_filter = if agent_lock.tools.len() == 1
                                && agent_lock.tools.first().map(String::as_str).is_some_and(|c| c == "*")
                            {
                                ToolFilter::All
                            } else {
                                let set = agent_lock
                                    .tools
                                    .iter()
                                    .filter(|tool_name| tool_name.starts_with(&format!("@{server_name}")))
                                    .map(|full_name| {
                                        match full_name.split_once(MCP_SERVER_TOOL_DELIMITER) {
                                            Some((_, tool_name)) if !tool_name.is_empty() => tool_name,
                                            _ => "*",
                                        }
                                        .to_string()
                                    })
                                    .collect::<HashSet<_>>();

                                if set.contains("*") {
                                    ToolFilter::All
                                } else {
                                    ToolFilter::List(set)
                                }
                            };

                            let server_prefix = format!("@{server_name}");
                            let alias_list = agent_lock.tool_aliases.iter().fold(
                                HashMap::<HostToolName, ModelToolName>::new(),
                                |mut acc, (full_path, model_tool_name)| {
                                    if full_path.starts_with(&server_prefix) {
                                        if let Some((_, host_tool_name)) =
                                            full_path.split_once(MCP_SERVER_TOOL_DELIMITER)
                                        {
                                            acc.insert(host_tool_name.to_string(), model_tool_name.clone());
                                        }
                                    }
                                    acc
                                },
                            );

                            (tool_filter, alias_list)
                        };

                        match result {
                            Ok(result) => {
                                let mut specs = result
                                    .tools
                                    .into_iter()
                                    .filter_map(|v| serde_json::from_value::<ToolSpec>(v).ok())
                                    .filter(|spec| tool_filter.should_include(&spec.name))
                                    .collect::<Vec<_>>();
                                let mut sanitized_mapping = HashMap::<ModelToolName, ToolInfo>::new();
                                let process_result = process_tool_specs(
                                    &database,
                                    conv_id_clone.as_str(),
                                    &server_name,
                                    &mut specs,
                                    &mut sanitized_mapping,
                                    &alias_list,
                                    &regex,
                                    &telemetry_clone,
                                )
                                .await;
                                if let Some(sender) = &loading_status_sender_clone {
                                    // Anomalies here are not considered fatal, thus we shall give
                                    // warnings.
                                    let msg = match process_result {
                                        Ok(_) => LoadingMsg::Done {
                                            name: server_name.clone(),
                                            time: time_taken.clone(),
                                        },
                                        Err(ref e) => LoadingMsg::Warn {
                                            name: server_name.clone(),
                                            msg: eyre::eyre!(e.to_string()),
                                            time: time_taken.clone(),
                                        },
                                    };
                                    if let Err(e) = sender.send(msg).await {
                                        warn!(
                                            "Error sending update message to display task: {:?}\nAssume display task has completed",
                                            e
                                        );
                                        loading_status_sender_clone.take();
                                    }
                                }
                                new_tool_specs_clone
                                    .lock()
                                    .await
                                    .insert(server_name.clone(), (sanitized_mapping, specs));
                                has_new_stuff_clone.store(true, Ordering::Release);
                                // Maintain a record of the server load:
                                let mut buf_writer = BufWriter::new(&mut record_temp_buf);
                                if let Err(e) = &process_result {
                                    let _ = queue_warn_message(
                                        server_name.as_str(),
                                        e,
                                        time_taken.as_str(),
                                        &mut buf_writer,
                                    );
                                } else {
                                    let _ = queue_success_message(
                                        server_name.as_str(),
                                        time_taken.as_str(),
                                        &mut buf_writer,
                                    );
                                }
                                let _ = buf_writer.flush();
                                drop(buf_writer);
                                let record = String::from_utf8_lossy(&record_temp_buf).to_string();
                                let record = if process_result.is_err() {
                                    LoadingRecord::Warn(record)
                                } else {
                                    LoadingRecord::Success(record)
                                };
                                load_record_clone
                                    .lock()
                                    .await
                                    .entry(server_name.clone())
                                    .and_modify(|load_record| {
                                        load_record.push(record.clone());
                                    })
                                    .or_insert(vec![record]);
                            },
                            Err(e) => {
                                // Log error to chat Log
                                error!("Error loading server {server_name}: {:?}", e);
                                // Maintain a record of the server load:
                                let mut buf_writer = BufWriter::new(&mut record_temp_buf);
                                let _ = queue_failure_message(server_name.as_str(), &e, &time_taken, &mut buf_writer);
                                let _ = buf_writer.flush();
                                drop(buf_writer);
                                let record = String::from_utf8_lossy(&record_temp_buf).to_string();
                                let record = LoadingRecord::Err(record);
                                load_record_clone
                                    .lock()
                                    .await
                                    .entry(server_name.clone())
                                    .and_modify(|load_record| {
                                        load_record.push(record.clone());
                                    })
                                    .or_insert(vec![record]);
                                // Errors surfaced at this point (i.e. before [process_tool_specs]
                                // is called) are fatals and should be considered errors
                                if let Some(sender) = &loading_status_sender_clone {
                                    let msg = LoadingMsg::Error {
                                        name: server_name.clone(),
                                        msg: e,
                                        time: time_taken,
                                    };
                                    if let Err(e) = sender.send(msg).await {
                                        warn!(
                                            "Error sending update message to display task: {:?}\nAssume display task has completed",
                                            e
                                        );
                                        loading_status_sender_clone.take();
                                    }
                                }
                            },
                        }
                        if let Some(notify) = notify_weak.upgrade() {
                            initialized.insert(server_name);
                            if initialized.len() >= total {
                                notify.notify_one();
                            }
                        }
                    },
                    UpdateEventMessage::PromptsListResult {
                        server_name: _,
                        result: _,
                    } => {},
                    UpdateEventMessage::ResourcesListResult {
                        server_name: _,
                        result: _,
                    } => {},
                    UpdateEventMessage::ResourceTemplatesListResult {
                        server_name: _,
                        result: _,
                    } => {},
                    UpdateEventMessage::InitStart { server_name } => {
                        pending_clone.write().await.insert(server_name.clone());
                        loading_servers.insert(server_name, std::time::Instant::now());
                    },
                }
            }
        });

        for (mut name, init_res) in pre_initialized {
            let messenger = messenger_builder.build_with_name(name.clone());
            match init_res {
                Ok(mut client) => {
                    client.assign_messenger(Box::new(messenger));
                    let mut client = Arc::new(client);
                    while let Some(collided_client) = clients.insert(name.clone(), client) {
                        // to avoid server name collision we are going to circumvent this by
                        // appending the name with 1
                        name.push('1');
                        client = collided_client;
                    }
                },
                Err(e) => {
                    error!("Error initializing mcp client for server {}: {:?}", name, &e);
                    os.telemetry
                        .send_mcp_server_init(&os.database, conversation_id.clone(), name, Some(e.to_string()), 0)
                        .await
                        .ok();
                    let _ = messenger.send_tools_list_result(Err(e)).await;
                },
            }
        }

        // Set up task to handle prompt requests
        let sender = self.prompt_list_sender.take();
        let receiver = self.prompt_list_receiver.take();
        let prompts = Arc::new(SyncRwLock::new(HashMap::default()));
        // TODO: accommodate hot reload of mcp servers
        if let (Some(sender), Some(receiver)) = (sender, receiver) {
            let clients = clients.iter().fold(HashMap::new(), |mut acc, (n, c)| {
                acc.insert(n.clone(), Arc::downgrade(c));
                acc
            });
            let prompts_clone = prompts.clone();
            tokio::task::spawn_blocking(move || {
                let receiver = Arc::new(std::sync::Mutex::new(receiver));
                loop {
                    let search_word = receiver.lock().map_err(|e| eyre::eyre!("{:?}", e))?.recv()?;
                    if clients
                        .values()
                        .any(|client| client.upgrade().is_some_and(|c| c.is_prompts_out_of_date()))
                    {
                        let mut prompts_wl = prompts_clone.write().map_err(|e| {
                            eyre::eyre!(
                                "Error retrieving write lock on prompts for tab complete {}",
                                e.to_string()
                            )
                        })?;
                        *prompts_wl = clients.iter().fold(
                            HashMap::<String, Vec<PromptBundle>>::new(),
                            |mut acc, (server_name, client)| {
                                let Some(client) = client.upgrade() else {
                                    return acc;
                                };
                                let prompt_gets = client.list_prompt_gets();
                                let Ok(prompt_gets) = prompt_gets.read() else {
                                    tracing::error!("Error retrieving read lock for prompt gets for tab complete");
                                    return acc;
                                };
                                for (prompt_name, prompt_get) in prompt_gets.iter() {
                                    acc.entry(prompt_name.clone())
                                        .and_modify(|bundles| {
                                            bundles.push(PromptBundle {
                                                server_name: server_name.to_owned(),
                                                prompt_get: prompt_get.clone(),
                                            });
                                        })
                                        .or_insert(vec![PromptBundle {
                                            server_name: server_name.to_owned(),
                                            prompt_get: prompt_get.clone(),
                                        }]);
                                }
                                client.prompts_updated();
                                acc
                            },
                        );
                    }
                    let prompts_rl = prompts_clone.read().map_err(|e| {
                        eyre::eyre!(
                            "Error retrieving read lock on prompts for tab complete {}",
                            e.to_string()
                        )
                    })?;
                    let filtered_prompts = prompts_rl
                        .iter()
                        .flat_map(|(prompt_name, bundles)| {
                            if bundles.len() > 1 {
                                bundles
                                    .iter()
                                    .map(|b| format!("{}/{}", b.server_name, prompt_name))
                                    .collect()
                            } else {
                                vec![prompt_name.to_owned()]
                            }
                        })
                        .filter(|n| {
                            if let Some(p) = &search_word {
                                n.contains(p)
                            } else {
                                true
                            }
                        })
                        .collect::<Vec<_>>();
                    if let Err(e) = sender.send(filtered_prompts) {
                        error!("Error sending prompts to chat helper: {:?}", e);
                    }
                }
                #[allow(unreachable_code)]
                Ok::<(), eyre::Report>(())
            });
        }

        Ok(ToolManager {
            conversation_id,
            clients,
            prompts,
            pending_clients: pending,
            notify: Some(notify),
            loading_status_sender,
            loading_display_task,
            new_tool_specs,
            has_new_stuff,
            is_interactive: interactive,
            mcp_load_record: load_record,
            agent,
            disabled_servers: disabled_servers_display,
            ..Default::default()
        })
    }
}

#[derive(Clone, Debug)]
/// A collection of information that is used for the following purposes:
/// - Checking if prompt info cached is out of date
/// - Retrieve new prompt info
pub struct PromptBundle {
    /// The server name from which the prompt is offered / exposed
    pub server_name: String,
    /// The prompt get (info with which a prompt is retrieved) cached
    pub prompt_get: PromptGet,
}

/// Categorizes different types of tool name validation failures:
/// - `TooLong`: The tool name exceeds the maximum allowed length
/// - `IllegalChar`: The tool name contains characters that are not allowed
/// - `EmptyDescription`: The tool description is empty or missing
#[allow(dead_code)]
enum OutOfSpecName {
    TooLong(String),
    IllegalChar(String),
    EmptyDescription(String),
}

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct ToolInfo {
    pub server_name: String,
    pub host_tool_name: HostToolName,
}

impl Borrow<HostToolName> for ToolInfo {
    fn borrow(&self) -> &HostToolName {
        &self.host_tool_name
    }
}

impl std::hash::Hash for ToolInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.host_tool_name.hash(state);
    }
}

/// Tool name as recognized by the model. This is [HostToolName] post sanitization.
type ModelToolName = String;

/// Tool name as recognized by the host (i.e. Q CLI). This is identical to how each MCP server
/// exposed them.
type HostToolName = String;

/// MCP server name as they are defined in the config
type ServerName = String;

/// A list of new tools to be included in the main chat loop.
/// The vector of [ToolSpec] is a comprehensive list of all tools exposed by the server.
/// The hashmap of [ModelToolName]: [HostToolName] are mapping of tool names that have been changed
/// (which is a subset of the tools that are in the aforementioned vector)
/// Note that [ToolSpec] is model facing and thus will have names that are model facing (i.e. model
/// tool name).
type NewToolSpecs = Arc<Mutex<HashMap<ServerName, (HashMap<ModelToolName, ToolInfo>, Vec<ToolSpec>)>>>;

#[derive(Default, Debug)]
/// Manages the lifecycle and interactions with tools from various sources, including MCP servers.
/// This struct is responsible for initializing tools, handling tool requests, and maintaining
/// a cache of available prompts from connected servers.
pub struct ToolManager {
    /// Unique identifier for the current conversation.
    /// This ID is used to track and associate tools with a specific chat session.
    pub conversation_id: String,

    /// Map of server names to their corresponding client instances.
    /// These clients are used to communicate with MCP servers.
    pub clients: HashMap<String, Arc<CustomToolClient>>,

    /// A list of client names that are still in the process of being initialized
    pub pending_clients: Arc<RwLock<HashSet<String>>>,

    /// Flag indicating whether new tool specifications have been added since the last update.
    /// When set to true, it signals that the tool manager needs to refresh its internal state
    /// to incorporate newly available tools from MCP servers.
    pub has_new_stuff: Arc<AtomicBool>,

    /// Storage for newly discovered tool specifications from MCP servers that haven't yet been
    /// integrated into the main tool registry. This field holds a thread-safe reference to a map
    /// of server names to their tool specifications and name mappings, allowing concurrent updates
    /// from server initialization processes.
    new_tool_specs: NewToolSpecs,

    /// Cache for prompts collected from different servers.
    /// Key: prompt name
    /// Value: a list of PromptBundle that has a prompt of this name.
    /// This cache helps resolve prompt requests efficiently and handles
    /// cases where multiple servers offer prompts with the same name.
    pub prompts: Arc<SyncRwLock<HashMap<String, Vec<PromptBundle>>>>,

    /// A notifier to understand if the initial loading has completed.
    /// This is only used for initial loading and is discarded after.
    notify: Option<Arc<Notify>>,

    /// Channel sender for communicating with the loading display thread.
    /// Used to send status updates about tool initialization progress.
    loading_status_sender: Option<tokio::sync::mpsc::Sender<LoadingMsg>>,

    /// This is here so we can await it to avoid output buffer from the display task interleaving
    /// with other buffer displayed by chat.
    loading_display_task: Option<JoinHandle<Result<(), Report>>>,

    /// Mapping from sanitized tool names to original tool names.
    /// This is used to handle tool name transformations that may occur during initialization
    /// to ensure tool names comply with naming requirements.
    pub tn_map: HashMap<ModelToolName, ToolInfo>,

    /// A cache of tool's input schema for all of the available tools.
    /// This is mainly used to show the user what the tools look like from the perspective of the
    /// model.
    pub schema: HashMap<ModelToolName, ToolSpec>,

    is_interactive: bool,

    /// This serves as a record of the loading of mcp servers.
    /// The key of which is the server name as they are recognized by the current instance of chat
    /// (which may be different than how it is written in the config, depending of the presence of
    /// invalid characters).
    /// The value is the load message (i.e. load time, warnings, and errors)
    pub mcp_load_record: Arc<Mutex<HashMap<String, Vec<LoadingRecord>>>>,

    /// List of disabled MCP server names for display purposes
    disabled_servers: Vec<String>,

    /// A collection of preferences that pertains to the conversation.
    /// As far as tool manager goes, this is relevant for tool and server filters
    pub agent: Arc<Mutex<Agent>>,
}

impl Clone for ToolManager {
    fn clone(&self) -> Self {
        Self {
            conversation_id: self.conversation_id.clone(),
            clients: self.clients.clone(),
            has_new_stuff: self.has_new_stuff.clone(),
            new_tool_specs: self.new_tool_specs.clone(),
            prompts: self.prompts.clone(),
            tn_map: self.tn_map.clone(),
            schema: self.schema.clone(),
            is_interactive: self.is_interactive,
            mcp_load_record: self.mcp_load_record.clone(),
            disabled_servers: self.disabled_servers.clone(),
            ..Default::default()
        }
    }
}

impl ToolManager {
    pub async fn load_tools(
        &mut self,
        os: &mut Os,
        stderr: &mut impl Write,
    ) -> eyre::Result<HashMap<String, ToolSpec>> {
        let tx = self.loading_status_sender.take();
        let notify = self.notify.take();
        self.schema = {
            let tool_list = &self.agent.lock().await.tools;
            let is_allow_all = tool_list.len() == 1 && tool_list.first().is_some_and(|n| n == "*");
            let is_allow_native = tool_list.iter().any(|t| t.as_str() == "@builtin");
            let mut tool_specs =
                serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))?
                    .into_iter()
                    .filter(|(name, _)| {
                        use crate::cli::chat::tools::NATIVE_TOOLS;
                        name == DUMMY_TOOL_NAME
                            || is_allow_all
                            || is_allow_native
                            || tool_list.contains(name)
                            || tool_list.contains(&format!("@builtin/{name}"))
                            || NATIVE_TOOLS.contains(&name.as_str())
                    })
                    .collect::<HashMap<_, _>>();
            if !crate::cli::chat::tools::thinking::Thinking::is_enabled(os) {
                tool_specs.remove("thinking");
            }
            if !crate::cli::chat::tools::knowledge::Knowledge::is_enabled(os) {
                tool_specs.remove("knowledge");
            }

            #[cfg(windows)]
            {
                use serde_json::json;

                use crate::cli::chat::tools::InputSchema;

                tool_specs.remove("execute_bash");

                tool_specs.insert("execute_cmd".to_string(), ToolSpec {
                    name: "execute_cmd".to_string(),
                    description: "Execute the specified Windows command.".to_string(),
                    input_schema: InputSchema(json!({
                    "type": "object",
                    "properties": {
                    "command": {
                        "type": "string",
                        "description": "Windows command to execute"
                    },
                    "summary": {
                        "type": "string",
                        "description": "A brief explanation of what the command does"
                    }
                    },
                        "required": ["command"]})),
                    tool_origin: ToolOrigin::Native,
                });
            }

            tool_specs
        };
        let load_tools = self
            .clients
            .values()
            .map(|c| {
                let clone = Arc::clone(c);
                async move { clone.init().await }
            })
            .collect::<Vec<_>>();
        let initial_poll = stream::iter(load_tools)
            .map(|async_closure| tokio::spawn(async_closure))
            .buffer_unordered(20);
        tokio::spawn(async move {
            initial_poll.collect::<Vec<_>>().await;
        });
        // We need to cast it to erase the type otherwise the compiler will default to static
        // dispatch, which would result in an error of inconsistent match arm return type.
        let timeout_fut: Pin<Box<dyn Future<Output = ()>>> = if self.clients.is_empty() {
            // If there is no server loaded, we want to resolve immediately
            Box::pin(future::ready(()))
        } else if self.is_interactive {
            let init_timeout = os
                .database
                .settings
                .get_int(Setting::McpInitTimeout)
                .map_or(5000_u64, |s| s as u64);
            Box::pin(tokio::time::sleep(std::time::Duration::from_millis(init_timeout)))
        } else {
            // if it is non-interactive we will want to use the "mcp.noInteractiveTimeout"
            let init_timeout = os
                .database
                .settings
                .get_int(Setting::McpNoInteractiveTimeout)
                .map_or(30_000_u64, |s| s as u64);
            Box::pin(tokio::time::sleep(std::time::Duration::from_millis(init_timeout)))
        };
        let server_loading_fut: Pin<Box<dyn Future<Output = ()>>> = if let Some(notify) = notify {
            Box::pin(async move { notify.notified().await })
        } else {
            Box::pin(future::ready(()))
        };
        let loading_display_task = self.loading_display_task.take();
        tokio::select! {
            _ = timeout_fut => {
                if let Some(tx) = tx {
                    let still_loading = self.pending_clients.read().await.iter().cloned().collect::<Vec<_>>();
                    let _ = tx.send(LoadingMsg::Terminate { still_loading }).await;
                    if let Some(task) = loading_display_task {
                        let _ = tokio::time::timeout(
                            std::time::Duration::from_millis(80),
                            task
                        ).await;
                    }
                }
                if !self.clients.is_empty() && !self.is_interactive {
                    let _ = queue!(
                        stderr,
                        style::Print(
                            "Not all mcp servers loaded. Configure non-interactive timeout with q settings mcp.noInteractiveTimeout"
                        ),
                        style::Print("\n------\n")
                    );
                }
            },
            _ = server_loading_fut => {
                if let Some(tx) = tx {
                    let still_loading = self.pending_clients.read().await.iter().cloned().collect::<Vec<_>>();
                    let _ = tx.send(LoadingMsg::Terminate { still_loading }).await;
                }
            }
            _ = ctrl_c() => {
                if self.is_interactive {
                    if let Some(tx) = tx {
                        let still_loading = self.pending_clients.read().await.iter().cloned().collect::<Vec<_>>();
                        let _ = tx.send(LoadingMsg::Terminate { still_loading }).await;
                    }
                } else {
                    return Err(eyre::eyre!("User interrupted mcp server loading in non-interactive mode. Ending."));
                }
            }
        }
        if !self.is_interactive
            && self
                .mcp_load_record
                .lock()
                .await
                .iter()
                .any(|(_, records)| records.iter().any(|record| matches!(record, LoadingRecord::Err(_))))
        {
            queue!(
                stderr,
                style::Print(
                    "One or more mcp server did not load correctly. See $TMPDIR/qlog/chat.log for more details."
                ),
                style::Print("\n------\n")
            )?;
        }
        stderr.flush()?;
        self.update().await;
        Ok(self.schema.clone())
    }

    pub fn get_tool_from_tool_use(&self, value: AssistantToolUse) -> Result<Tool, ToolResult> {
        let map_err = |parse_error| ToolResult {
            tool_use_id: value.id.clone(),
            content: vec![ToolResultContentBlock::Text(format!(
                "Failed to validate tool parameters: {parse_error}. The model has either suggested tool parameters which are incompatible with the existing tools, or has suggested one or more tool that does not exist in the list of known tools."
            ))],
            status: ToolResultStatus::Error,
        };

        Ok(match value.name.as_str() {
            "fs_read" => Tool::FsRead(serde_json::from_value::<FsRead>(value.args).map_err(map_err)?),
            "fs_write" => Tool::FsWrite(serde_json::from_value::<FsWrite>(value.args).map_err(map_err)?),
            #[cfg(windows)]
            "execute_cmd" => {
                Tool::ExecuteCommand(serde_json::from_value::<ExecuteCommand>(value.args).map_err(map_err)?)
            },
            #[cfg(not(windows))]
            "execute_bash" => {
                Tool::ExecuteCommand(serde_json::from_value::<ExecuteCommand>(value.args).map_err(map_err)?)
            },
            "use_aws" => Tool::UseAws(serde_json::from_value::<UseAws>(value.args).map_err(map_err)?),
            "report_issue" => Tool::GhIssue(serde_json::from_value::<GhIssue>(value.args).map_err(map_err)?),
            "thinking" => Tool::Thinking(serde_json::from_value::<Thinking>(value.args).map_err(map_err)?),
            "knowledge" => Tool::Knowledge(serde_json::from_value::<Knowledge>(value.args).map_err(map_err)?),
            // Note that this name is namespaced with server_name{DELIMITER}tool_name
            name => {
                // Note: tn_map also has tools that underwent no transformation. In otherwords, if
                // it is a valid tool name, we should get a hit.
                let ToolInfo {
                    server_name,
                    host_tool_name: tool_name,
                } = match self.tn_map.get(name) {
                    Some(tool_info) => Ok::<&ToolInfo, ToolResult>(tool_info),
                    None => {
                        // No match, we throw an error
                        Err(ToolResult {
                            tool_use_id: value.id.clone(),
                            content: vec![ToolResultContentBlock::Text(format!(
                                "No tool with \"{name}\" is found"
                            ))],
                            status: ToolResultStatus::Error,
                        })
                    },
                }?;
                let Some(client) = self.clients.get(server_name) else {
                    return Err(ToolResult {
                        tool_use_id: value.id,
                        content: vec![ToolResultContentBlock::Text(format!(
                            "The tool, \"{server_name}\" is not supported by the client"
                        ))],
                        status: ToolResultStatus::Error,
                    });
                };
                // The tool input schema has the shape of { type, properties }.
                // The field "params" expected by MCP is { name, arguments }, where name is the
                // name of the tool being invoked,
                // https://spec.modelcontextprotocol.io/specification/2024-11-05/server/tools/#calling-tools.
                // The field "arguments" is where ToolUse::args belong.
                let mut params = serde_json::Map::<String, serde_json::Value>::new();
                params.insert("name".to_owned(), serde_json::Value::String(tool_name.to_owned()));
                params.insert("arguments".to_owned(), value.args);
                let params = serde_json::Value::Object(params);
                let custom_tool = CustomTool {
                    name: tool_name.to_owned(),
                    client: client.clone(),
                    method: "tools/call".to_owned(),
                    params: Some(params),
                };
                Tool::Custom(custom_tool)
            },
        })
    }

    /// Updates tool managers various states with new information
    pub async fn update(&mut self) {
        // A hashmap of <tool name, tool spec>
        let mut tool_specs = HashMap::<String, ToolSpec>::new();
        let new_tools = {
            let mut new_tool_specs = self.new_tool_specs.lock().await;
            new_tool_specs.drain().fold(
                HashMap::<ServerName, (HashMap<ModelToolName, ToolInfo>, Vec<ToolSpec>)>::new(),
                |mut acc, (server_name, v)| {
                    acc.insert(server_name, v);
                    acc
                },
            )
        };

        let mut updated_servers = HashSet::<ToolOrigin>::new();
        let mut conflicts = HashMap::<ServerName, String>::new();
        for (server_name, (tool_name_map, specs)) in new_tools {
            // First we evict the tools that were already in the tn_map
            self.tn_map.retain(|_, tool_info| tool_info.server_name != server_name);

            // And update them with the new tools queried
            // valid: tools that do not have conflicts in naming
            let (valid, invalid) = tool_name_map
                .into_iter()
                .partition::<HashMap<ModelToolName, ToolInfo>, _>(|(model_tool_name, _)| {
                    !self.tn_map.contains_key(model_tool_name)
                });
            // We reject tools that are conflicting with the existing tools by not including them
            // in the tn_map. We would also want to report this error.
            if !invalid.is_empty() {
                let msg = invalid.into_iter().fold("The following tools are rejected because they conflict with existing tools in names. Avoid this via setting aliases for them: \n".to_string(), |mut acc, (model_tool_name, tool_info)| {
                    acc.push_str(&format!(" - {} from {}\n", model_tool_name, tool_info.server_name));
                    acc
                });
                conflicts.insert(server_name, msg);
            }
            if let Some(spec) = specs.first() {
                updated_servers.insert(spec.tool_origin.clone());
            }
            // We want to filter for specs that are valid
            // Note that [ToolSpec::name] is a model facing name (thus you should be comparing it
            // with the keys of a tn_map)
            for spec in specs.into_iter().filter(|spec| valid.contains_key(&spec.name)) {
                tool_specs.insert(spec.name.clone(), spec);
            }

            self.tn_map.extend(valid);
        }

        // Update schema
        // As we are writing over the ensemble of tools in a given server, we will need to first
        // remove everything that it has.
        self.schema
            .retain(|_tool_name, spec| !updated_servers.contains(&spec.tool_origin));
        self.schema.extend(tool_specs);

        // if block here to avoid repeatedly asking for loc
        if !conflicts.is_empty() {
            let mut record_lock = self.mcp_load_record.lock().await;
            for (server_name, msg) in conflicts {
                let record = LoadingRecord::Err(msg);
                record_lock
                    .entry(server_name)
                    .and_modify(|v| v.push(record.clone()))
                    .or_insert(vec![record]);
            }
        }
    }

    #[allow(clippy::await_holding_lock)]
    pub async fn get_prompt(
        &self,
        name: String,
        arguments: Option<Vec<String>>,
    ) -> Result<JsonRpcResponse, GetPromptError> {
        let (server_name, prompt_name) = match name.split_once('/') {
            None => (None::<String>, Some(name.clone())),
            Some((server_name, prompt_name)) => (Some(server_name.to_string()), Some(prompt_name.to_string())),
        };
        let prompt_name = prompt_name.ok_or(GetPromptError::MissingPromptName)?;
        // We need to use a sync lock here because this lock is also used in a blocking thread,
        // necessitated by the fact that said thread is also responsible for using a sync channel,
        // which is itself necessitated by the fact that consumer of said channel is calling from a
        // sync function
        let mut prompts_wl = self
            .prompts
            .write()
            .map_err(|e| GetPromptError::Synchronization(e.to_string()))?;
        let mut maybe_bundles = prompts_wl.get(&prompt_name);
        let mut has_retried = false;
        'blk: loop {
            match (maybe_bundles, server_name.as_ref(), has_retried) {
                // If we have more than one eligible clients but no server name specified
                (Some(bundles), None, _) if bundles.len() > 1 => {
                    break 'blk Err(GetPromptError::AmbiguousPrompt(prompt_name.clone(), {
                        bundles.iter().fold("\n".to_string(), |mut acc, b| {
                            acc.push_str(&format!("- @{}/{}\n", b.server_name, prompt_name));
                            acc
                        })
                    }));
                },
                // Normal case where we have enough info to proceed
                // Note that if bundle exists, it should never be empty
                (Some(bundles), sn, _) => {
                    let bundle = if bundles.len() > 1 {
                        let Some(server_name) = sn else {
                            maybe_bundles = None;
                            continue 'blk;
                        };
                        let bundle = bundles.iter().find(|b| b.server_name == *server_name);
                        match bundle {
                            Some(bundle) => bundle,
                            None => {
                                maybe_bundles = None;
                                continue 'blk;
                            },
                        }
                    } else {
                        bundles.first().ok_or(GetPromptError::MissingPromptInfo)?
                    };
                    let server_name = bundle.server_name.clone();
                    let client = self.clients.get(&server_name).ok_or(GetPromptError::MissingClient)?;
                    // Here we lazily update the out of date cache
                    if client.is_prompts_out_of_date() {
                        let prompt_gets = client.list_prompt_gets();
                        let prompt_gets = prompt_gets
                            .read()
                            .map_err(|e| GetPromptError::Synchronization(e.to_string()))?;
                        for (prompt_name, prompt_get) in prompt_gets.iter() {
                            prompts_wl
                                .entry(prompt_name.clone())
                                .and_modify(|bundles| {
                                    let mut is_modified = false;
                                    for bundle in &mut *bundles {
                                        let mut updated_bundle = PromptBundle {
                                            server_name: server_name.clone(),
                                            prompt_get: prompt_get.clone(),
                                        };
                                        if bundle.server_name == *server_name {
                                            std::mem::swap(bundle, &mut updated_bundle);
                                            is_modified = true;
                                            break;
                                        }
                                    }
                                    if !is_modified {
                                        bundles.push(PromptBundle {
                                            server_name: server_name.clone(),
                                            prompt_get: prompt_get.clone(),
                                        });
                                    }
                                })
                                .or_insert(vec![PromptBundle {
                                    server_name: server_name.clone(),
                                    prompt_get: prompt_get.clone(),
                                }]);
                        }
                        client.prompts_updated();
                    }

                    let PromptBundle { prompt_get, .. } = prompts_wl
                        .get(&prompt_name)
                        .and_then(|bundles| bundles.iter().find(|b| b.server_name == server_name))
                        .ok_or(GetPromptError::MissingPromptInfo)?;

                    // Here we need to convert the positional arguments into key value pair
                    // The assignment order is assumed to be the order of args as they are
                    // presented in PromptGet::arguments
                    let args = if let (Some(schema), Some(value)) = (&prompt_get.arguments, &arguments) {
                        let params = schema.iter().zip(value.iter()).fold(
                            HashMap::<String, String>::new(),
                            |mut acc, (prompt_get_arg, value)| {
                                acc.insert(prompt_get_arg.name.clone(), value.clone());
                                acc
                            },
                        );
                        Some(serde_json::json!(params))
                    } else {
                        None
                    };
                    let params = {
                        let mut params = serde_json::Map::new();
                        params.insert("name".to_string(), serde_json::Value::String(prompt_name));
                        if let Some(args) = args {
                            params.insert("arguments".to_string(), args);
                        }
                        Some(serde_json::Value::Object(params))
                    };
                    let resp = client.request("prompts/get", params).await?;
                    break 'blk Ok(resp);
                },
                // If we have no eligible clients this would mean one of the following:
                // - The prompt does not exist, OR
                // - This is the first time we have a query / our cache is out of date
                // Both of which means we would have to requery
                (None, _, false) => {
                    has_retried = true;
                    self.refresh_prompts(&mut prompts_wl)?;
                    maybe_bundles = prompts_wl.get(&prompt_name);
                },
                (_, _, true) => {
                    break 'blk Err(GetPromptError::PromptNotFound(prompt_name));
                },
            }
        }
    }

    pub fn refresh_prompts(&self, prompts_wl: &mut HashMap<String, Vec<PromptBundle>>) -> Result<(), GetPromptError> {
        *prompts_wl = self.clients.iter().fold(
            HashMap::<String, Vec<PromptBundle>>::new(),
            |mut acc, (server_name, client)| {
                let prompt_gets = client.list_prompt_gets();
                let Ok(prompt_gets) = prompt_gets.read() else {
                    tracing::error!("Error encountered while retrieving read lock");
                    return acc;
                };
                for (prompt_name, prompt_get) in prompt_gets.iter() {
                    acc.entry(prompt_name.clone())
                        .and_modify(|bundles| {
                            bundles.push(PromptBundle {
                                server_name: server_name.to_owned(),
                                prompt_get: prompt_get.clone(),
                            });
                        })
                        .or_insert(vec![PromptBundle {
                            server_name: server_name.to_owned(),
                            prompt_get: prompt_get.clone(),
                        }]);
                }
                acc
            },
        );
        Ok(())
    }

    pub async fn pending_clients(&self) -> Vec<String> {
        self.pending_clients.read().await.iter().cloned().collect::<Vec<_>>()
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_tool_specs(
    database: &Database,
    conversation_id: &str,
    server_name: &str,
    specs: &mut Vec<ToolSpec>,
    tn_map: &mut HashMap<ModelToolName, ToolInfo>,
    alias_list: &HashMap<HostToolName, ModelToolName>,
    regex: &Regex,
    telemetry: &TelemetryThread,
) -> eyre::Result<()> {
    // Tools are subjected to the following validations:
    // 1. ^[a-zA-Z][a-zA-Z0-9_]*$,
    // 2. less than 64 characters in length
    // 3. a non-empty description
    //
    // For non-compliance due to point 1, we shall change it on behalf of the users.
    // For the rest, we simply throw a warning and reject the tool.
    let mut out_of_spec_tool_names = Vec::<OutOfSpecName>::new();
    let mut hasher = DefaultHasher::new();
    let mut number_of_tools = 0_usize;

    for spec in specs.iter_mut() {
        let model_tool_name = alias_list.get(&spec.name).cloned().unwrap_or({
            if !regex.is_match(&spec.name) {
                let mut sn = sanitize_name(spec.name.clone(), regex, &mut hasher);
                while tn_map.contains_key(&sn) {
                    sn.push('1');
                }
                sn
            } else {
                spec.name.clone()
            }
        });
        if model_tool_name.len() > 64 {
            out_of_spec_tool_names.push(OutOfSpecName::TooLong(spec.name.clone()));
            continue;
        } else if spec.description.is_empty() {
            out_of_spec_tool_names.push(OutOfSpecName::EmptyDescription(spec.name.clone()));
            continue;
        }
        tn_map.insert(model_tool_name.clone(), ToolInfo {
            server_name: server_name.to_string(),
            host_tool_name: spec.name.clone(),
        });
        spec.name = model_tool_name;
        spec.tool_origin = ToolOrigin::McpServer(server_name.to_string());
        number_of_tools += 1;
    }
    // Native origin is the default, and since this function never reads native tools, if we still
    // have it, that would indicate a tool that should not be included.
    specs.retain(|spec| !matches!(spec.tool_origin, ToolOrigin::Native));
    // Send server load success metric datum
    let conversation_id = conversation_id.to_string();
    let _ = telemetry
        .send_mcp_server_init(
            database,
            conversation_id,
            server_name.to_string(),
            None,
            number_of_tools,
        )
        .await;
    // Tool name translation. This is beyond of the scope of what is
    // considered a "server load". Reasoning being:
    // - Failures here are not related to server load
    // - There is not a whole lot we can do with this data
    if !out_of_spec_tool_names.is_empty() {
        Err(eyre::eyre!(out_of_spec_tool_names.iter().fold(
            String::from(
                "The following tools are out of spec. They will be excluded from the list of available tools:\n",
            ),
            |mut acc, name| {
                let (tool_name, msg) = match name {
                    OutOfSpecName::TooLong(tool_name) => (
                        tool_name.as_str(),
                        "tool name exceeds max length of 64 when combined with server name",
                    ),
                    OutOfSpecName::IllegalChar(tool_name) => (
                        tool_name.as_str(),
                        "tool name must be compliant with ^[a-zA-Z][a-zA-Z0-9_]*$",
                    ),
                    OutOfSpecName::EmptyDescription(tool_name) => {
                        (tool_name.as_str(), "tool schema contains empty description")
                    },
                };
                acc.push_str(format!(" - {} ({})\n", tool_name, msg).as_str());
                acc
            },
        )))
    } else {
        Ok(())
    }
}

fn sanitize_name(orig: String, regex: &regex::Regex, hasher: &mut impl Hasher) -> String {
    if regex.is_match(&orig) && !orig.contains(NAMESPACE_DELIMITER) {
        return orig;
    }
    let sanitized: String = orig
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || *c == '_')
        .collect::<String>()
        .replace(NAMESPACE_DELIMITER, "");
    if sanitized.is_empty() {
        hasher.write(orig.as_bytes());
        let hash = format!("{:03}", hasher.finish() % 1000);
        return format!("a{}", hash);
    }
    match sanitized.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => sanitized,
        Some(_) => {
            format!("a{}", sanitized)
        },
        None => {
            hasher.write(orig.as_bytes());
            format!("a{}", hasher.finish())
        },
    }
}

fn queue_success_message(name: &str, time_taken: &str, output: &mut impl Write) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Green),
        style::Print("✓ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" loaded in "),
        style::SetForegroundColor(style::Color::Yellow),
        style::Print(format!("{time_taken} s\n")),
        style::ResetColor,
    )?)
}

fn queue_init_message(
    spinner_logo_idx: usize,
    complete: usize,
    failed: usize,
    total: usize,
    output: &mut impl Write,
) -> eyre::Result<()> {
    if total == complete {
        queue!(
            output,
            style::SetForegroundColor(style::Color::Green),
            style::Print("✓"),
            style::ResetColor,
        )?;
    } else if total == complete + failed {
        queue!(
            output,
            style::SetForegroundColor(style::Color::Red),
            style::Print("✗"),
            style::ResetColor,
        )?;
    } else {
        queue!(output, style::Print(SPINNER_CHARS[spinner_logo_idx]))?;
    }
    queue!(
        output,
        style::SetForegroundColor(style::Color::Blue),
        style::Print(format!(" {}", complete)),
        style::ResetColor,
        style::Print(" of "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(format!("{} ", total)),
        style::ResetColor,
        style::Print("mcp servers initialized."),
    )?;
    if total > complete + failed {
        queue!(
            output,
            style::SetForegroundColor(style::Color::Blue),
            style::Print(" ctrl-c "),
            style::ResetColor,
            style::Print("to start chatting now")
        )?;
    }
    Ok(queue!(output, style::Print("\n"))?)
}

fn queue_failure_message(
    name: &str,
    fail_load_msg: &eyre::Report,
    time: &str,
    output: &mut impl Write,
) -> eyre::Result<()> {
    use crate::util::CHAT_BINARY_NAME;
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Red),
        style::Print("✗ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" has failed to load after"),
        style::SetForegroundColor(style::Color::Yellow),
        style::Print(format!(" {time} s")),
        style::ResetColor,
        style::Print("\n - "),
        style::Print(fail_load_msg),
        style::Print("\n"),
        style::Print(format!(
            " - run with Q_LOG_LEVEL=trace and see $TMPDIR/{CHAT_BINARY_NAME} for detail\n"
        )),
        style::ResetColor,
    )?)
}

fn queue_warn_message(name: &str, msg: &eyre::Report, time: &str, output: &mut impl Write) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Yellow),
        style::Print("⚠ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" has loaded in"),
        style::SetForegroundColor(style::Color::Yellow),
        style::Print(format!(" {time} s")),
        style::ResetColor,
        style::Print(" with the following warning:\n"),
        style::Print(msg),
        style::ResetColor,
    )?)
}

fn queue_disabled_message(name: &str, output: &mut impl Write) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::DarkGrey),
        style::Print("○ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" is disabled\n"),
        style::ResetColor,
    )?)
}

fn queue_incomplete_load_message(
    complete: usize,
    total: usize,
    msg: &eyre::Report,
    output: &mut impl Write,
) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Yellow),
        style::Print("⚠"),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(format!(" {}", complete)),
        style::ResetColor,
        style::Print(" of "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(format!("{} ", total)),
        style::ResetColor,
        style::Print("mcp servers initialized."),
        style::ResetColor,
        // We expect the message start with a newline
        style::Print(" Servers still loading:"),
        style::Print(msg),
        style::ResetColor,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_server_name() {
        let regex = regex::Regex::new(VALID_TOOL_NAME).unwrap();
        let mut hasher = DefaultHasher::new();
        let orig_name = "@awslabs.cdk-mcp-server";
        let sanitized_server_name = sanitize_name(orig_name.to_string(), &regex, &mut hasher);
        assert_eq!(sanitized_server_name, "awslabscdkmcpserver");

        let orig_name = "good_name";
        let sanitized_good_name = sanitize_name(orig_name.to_string(), &regex, &mut hasher);
        assert_eq!(sanitized_good_name, orig_name);

        let all_bad_name = "@@@@@";
        let sanitized_all_bad_name = sanitize_name(all_bad_name.to_string(), &regex, &mut hasher);
        assert!(regex.is_match(&sanitized_all_bad_name));

        let with_delim = format!("a{}b{}c", NAMESPACE_DELIMITER, NAMESPACE_DELIMITER);
        let sanitized = sanitize_name(with_delim, &regex, &mut hasher);
        assert_eq!(sanitized, "abc");
    }
}
