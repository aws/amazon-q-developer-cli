use std::collections::HashMap;
use std::hash::{
    DefaultHasher,
    Hasher,
};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{
    Arc,
    RwLock as SyncRwLock,
};

use convert_case::Casing;
use crossterm::{
    cursor,
    execute,
    queue,
    style,
    terminal,
};
use fig_api_client::model::{
    ToolResult,
    ToolResultContentBlock,
    ToolResultStatus,
};
use futures::{
    StreamExt,
    stream,
};
use mcp_client::{
    JsonRpcResponse,
    PromptGet,
};
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::error;

use super::command::PromptsGetCommand;
use super::message::AssistantToolUse;
use super::tools::custom_tool::{
    CustomToolClient,
    CustomToolConfig,
};
use super::tools::execute_bash::ExecuteBash;
use super::tools::fs_read::FsRead;
use super::tools::fs_write::FsWrite;
use super::tools::gh_issue::GhIssue;
use super::tools::memory::Memory;
use super::tools::use_aws::UseAws;
use super::tools::{
    Tool,
    ToolOrigin,
};
use crate::tools::ToolSpec;
use crate::tools::custom_tool::CustomTool;

const NAMESPACE_DELIMITER: &str = "___";
// This applies for both mcp server and tool name since in the end the tool name as seen by the
// model is just {server_name}{NAMESPACE_DELIMITER}{tool_name}
const VALID_TOOL_NAME: &str = "^[a-zA-Z][a-zA-Z0-9_]*$";
const SPINNER_CHARS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Debug, Error)]
pub enum GetPromptError {
    #[error("Prompt with name {0} does not exist")]
    PromptNotFound(String),
    #[error("Prompt {0} is offered by more than one server. Use one of the following {1}")]
    AmbiguousPrompt(String, String),
    #[error("Missing client")]
    MissingClient,
    #[error("Missing prompt name")]
    MissingPromptName,
    #[error("Synchronization error: {0}")]
    Synchronization(String),
    #[error("Missing prompt bundle")]
    MissingPromptInfo,
    #[error(transparent)]
    General(#[from] eyre::Report),
}

/// Messages used for communication between the tool initialization thread and the loading
/// display thread. These messages control the visual loading indicators shown to
/// the user during tool initialization.
enum LoadingMsg {
    /// Indicates a new tool is being initialized and should be added to the loading
    /// display. The String parameter is the name of the tool being initialized.
    Add(String),
    /// Indicates a tool has finished initializing successfully and should be removed from
    /// the loading display. The String parameter is the name of the tool that
    /// completed initialization.
    Done(String),
    /// Represents an error that occurred during tool initialization.
    /// Contains the name of the server that failed to initialize and the error message.
    Error { name: String, msg: eyre::Report },
    /// Represents a warning that occurred during tool initialization.
    /// Contains the name of the server that generated the warning and the warning message.
    Warn { name: String, msg: eyre::Report },
}

/// Represents the state of a loading indicator for a tool being initialized.
///
/// This struct tracks timing information for each tool's loading status display in the terminal.
///
/// # Fields
/// * `init_time` - When initialization for this tool began, used to calculate load time
struct StatusLine {
    init_time: std::time::Instant,
}

// This is to mirror claude's config set up
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    mcp_servers: HashMap<String, CustomToolConfig>,
}

impl McpServerConfig {
    pub async fn load_config(output: &mut impl Write) -> eyre::Result<Self> {
        let mut cwd = std::env::current_dir()?;
        cwd.push(".amazonq/mcp.json");
        let expanded_path = shellexpand::tilde("~/.aws/amazonq/mcp.json");
        let global_path = PathBuf::from(expanded_path.as_ref());
        let global_buf = tokio::fs::read(global_path).await.ok();
        let local_buf = tokio::fs::read(cwd).await.ok();
        let conf = match (global_buf, local_buf) {
            (Some(global_buf), Some(local_buf)) => {
                let mut global_conf = Self::from_slice(&global_buf, output, "global")?;
                let local_conf = Self::from_slice(&local_buf, output, "local")?;
                for (server_name, config) in local_conf.mcp_servers {
                    if global_conf.mcp_servers.insert(server_name.clone(), config).is_some() {
                        queue!(
                            output,
                            style::SetForegroundColor(style::Color::Yellow),
                            style::Print("WARNING: "),
                            style::ResetColor,
                            style::Print("MCP config conflict for "),
                            style::SetForegroundColor(style::Color::Green),
                            style::Print(server_name),
                            style::ResetColor,
                            style::Print(". Using workspace version.\n")
                        )?;
                    }
                }
                global_conf
            },
            (None, Some(local_buf)) => Self::from_slice(&local_buf, output, "local")?,
            (Some(global_buf), None) => Self::from_slice(&global_buf, output, "global")?,
            _ => Default::default(),
        };
        output.flush()?;
        Ok(conf)
    }

    fn from_slice(slice: &[u8], output: &mut impl Write, location: &str) -> eyre::Result<McpServerConfig> {
        match serde_json::from_slice::<Self>(slice) {
            Ok(config) => Ok(config),
            Err(e) => {
                queue!(
                    output,
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print("WARNING: "),
                    style::ResetColor,
                    style::Print(format!("Error reading {location} mcp config: {e}\n")),
                    style::Print("Please check to make sure config is correct. Discarding.\n"),
                )?;
                Ok(McpServerConfig::default())
            },
        }
    }
}

#[derive(Default)]
pub struct ToolManagerBuilder {
    mcp_server_config: Option<McpServerConfig>,
    prompt_list_sender: Option<std::sync::mpsc::Sender<Vec<String>>>,
    prompt_list_receiver: Option<std::sync::mpsc::Receiver<Option<String>>>,
    conversation_id: Option<String>,
}

impl ToolManagerBuilder {
    pub fn mcp_server_config(mut self, config: McpServerConfig) -> Self {
        self.mcp_server_config.replace(config);
        self
    }

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

    pub fn build(mut self) -> eyre::Result<ToolManager> {
        let McpServerConfig { mcp_servers } = self.mcp_server_config.ok_or(eyre::eyre!("Missing mcp server config"))?;
        debug_assert!(self.conversation_id.is_some());
        let conversation_id = self.conversation_id.ok_or(eyre::eyre!("Missing conversation id"))?;
        let regex = regex::Regex::new(VALID_TOOL_NAME)?;
        let mut hasher = DefaultHasher::new();
        let pre_initialized = mcp_servers
            .into_iter()
            .map(|(server_name, server_config)| {
                let snaked_cased_name = server_name.to_case(convert_case::Case::Snake);
                let sanitized_server_name = sanitize_name(snaked_cased_name, &regex, &mut hasher);
                let custom_tool_client = CustomToolClient::from_config(sanitized_server_name.clone(), server_config);
                (sanitized_server_name, custom_tool_client)
            })
            .collect::<Vec<(String, _)>>();

        // Send up task to update user on server loading status
        let (tx, rx) = std::sync::mpsc::channel::<LoadingMsg>();
        // Using a hand rolled thread because it's just easier to do this than do deal with the Send
        // requirements that comes with holding onto the stdout lock.
        let loading_display_task = std::thread::spawn(move || {
            let stdout = std::io::stdout();
            let mut stdout_lock = stdout.lock();
            let mut loading_servers = HashMap::<String, StatusLine>::new();
            let mut spinner_logo_idx: usize = 0;
            let mut complete: usize = 0;
            let mut failed: usize = 0;
            loop {
                match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(recv_result) => match recv_result {
                        LoadingMsg::Add(name) => {
                            let init_time = std::time::Instant::now();
                            let status_line = StatusLine { init_time };
                            execute!(stdout_lock, cursor::MoveToColumn(0))?;
                            if !loading_servers.is_empty() {
                                // TODO: account for terminal width
                                execute!(stdout_lock, cursor::MoveUp(1))?;
                            }
                            loading_servers.insert(name.clone(), status_line);
                            let total = loading_servers.len();
                            execute!(stdout_lock, terminal::Clear(terminal::ClearType::CurrentLine))?;
                            queue_init_message(spinner_logo_idx, complete, failed, total, &mut stdout_lock)?;
                            stdout_lock.flush()?;
                        },
                        LoadingMsg::Done(name) => {
                            if let Some(status_line) = loading_servers.get(&name) {
                                complete += 1;
                                let time_taken =
                                    (std::time::Instant::now() - status_line.init_time).as_secs_f64().abs();
                                let time_taken = format!("{:.2}", time_taken);
                                execute!(
                                    stdout_lock,
                                    cursor::MoveToColumn(0),
                                    cursor::MoveUp(1),
                                    terminal::Clear(terminal::ClearType::CurrentLine),
                                )?;
                                queue_success_message(&name, &time_taken, &mut stdout_lock)?;
                                let total = loading_servers.len();
                                queue_init_message(spinner_logo_idx, complete, failed, total, &mut stdout_lock)?;
                                stdout_lock.flush()?;
                            }
                        },
                        LoadingMsg::Error { name, msg } => {
                            failed += 1;
                            execute!(
                                stdout_lock,
                                cursor::MoveToColumn(0),
                                cursor::MoveUp(1),
                                terminal::Clear(terminal::ClearType::CurrentLine),
                            )?;
                            queue_failure_message(&name, &msg, &mut stdout_lock)?;
                            let total = loading_servers.len();
                            queue_init_message(spinner_logo_idx, complete, failed, total, &mut stdout_lock)?;
                        },
                        LoadingMsg::Warn { name, msg } => {
                            complete += 1;
                            execute!(
                                stdout_lock,
                                cursor::MoveToColumn(0),
                                cursor::MoveUp(1),
                                terminal::Clear(terminal::ClearType::CurrentLine),
                            )?;
                            let msg = eyre::eyre!(msg.to_string());
                            queue_warn_message(&name, &msg, &mut stdout_lock)?;
                            let total = loading_servers.len();
                            queue_init_message(spinner_logo_idx, complete, failed, total, &mut stdout_lock)?;
                            stdout_lock.flush()?;
                        },
                    },
                    Err(RecvTimeoutError::Timeout) => {
                        spinner_logo_idx = (spinner_logo_idx + 1) % SPINNER_CHARS.len();
                        execute!(
                            stdout_lock,
                            cursor::SavePosition,
                            cursor::MoveToColumn(0),
                            cursor::MoveUp(1),
                            style::Print(SPINNER_CHARS[spinner_logo_idx]),
                            cursor::RestorePosition
                        )?;
                    },
                    _ => break,
                }
            }
            Ok::<_, eyre::Report>(())
        });
        let mut clients = HashMap::<String, Arc<CustomToolClient>>::new();
        for (mut name, init_res) in pre_initialized {
            let _ = tx.send(LoadingMsg::Add(name.clone()));
            match init_res {
                Ok(client) => {
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
                    let event = fig_telemetry::EventType::McpServerInit {
                        conversation_id: conversation_id.clone(),
                        init_failure_reason: Some(e.to_string()),
                        number_of_tools: 0,
                    };
                    tokio::spawn(async move {
                        let app_event = fig_telemetry::AppTelemetryEvent::new(event).await;
                        fig_telemetry::dispatch_or_send_event(app_event).await;
                    });
                    let _ = tx.send(LoadingMsg::Error {
                        name: name.clone(),
                        msg: e,
                    });
                },
            }
        }
        let loading_display_task = Some(loading_display_task);
        let loading_status_sender = Some(tx);

        // Set up task to handle prompt requests
        let sender = self.prompt_list_sender.take();
        let receiver = self.prompt_list_receiver.take();
        let prompts = Arc::new(SyncRwLock::new(HashMap::default()));
        // TODO: accommodate hot reload of mcp servers
        if let (Some(sender), Some(receiver)) = (sender, receiver) {
            let clients = clients.iter().fold(HashMap::new(), |mut acc, (n, c)| {
                acc.insert(n.to_string(), Arc::downgrade(c));
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
                                    acc.entry(prompt_name.to_string())
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
            loading_display_task,
            loading_status_sender,
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

/// Manages the lifecycle and interactions with tools from various sources, including MCP servers.
/// This struct is responsible for initializing tools, handling tool requests, and maintaining
/// a cache of available prompts from connected servers.
#[derive(Default)]
pub struct ToolManager {
    /// Unique identifier for the current conversation.
    /// This ID is used to track and associate tools with a specific chat session.
    pub conversation_id: String,

    /// Map of server names to their corresponding client instances.
    /// These clients are used to communicate with MCP servers.
    pub clients: HashMap<String, Arc<CustomToolClient>>,

    /// Cache for prompts collected from different servers.
    /// Key: prompt name
    /// Value: a list of PromptBundle that has a prompt of this name.
    /// This cache helps resolve prompt requests efficiently and handles
    /// cases where multiple servers offer prompts with the same name.
    pub prompts: Arc<SyncRwLock<HashMap<String, Vec<PromptBundle>>>>,

    /// Handle to the thread that displays loading status for tool initialization.
    /// This thread provides visual feedback to users during the tool loading process.
    loading_display_task: Option<std::thread::JoinHandle<Result<(), eyre::Report>>>,

    /// Channel sender for communicating with the loading display thread.
    /// Used to send status updates about tool initialization progress.
    loading_status_sender: Option<std::sync::mpsc::Sender<LoadingMsg>>,

    /// Mapping from sanitized tool names to original tool names.
    /// This is used to handle tool name transformations that may occur during initialization
    /// to ensure tool names comply with naming requirements.
    pub tn_map: HashMap<String, String>,

    /// A cache of tool's input schema for all of the available tools.
    /// This is mainly used to show the user what the tools look like from the perspective of the
    /// model.
    pub schema: HashMap<String, ToolSpec>,
}

impl ToolManager {
    pub async fn load_tools(&mut self) -> eyre::Result<HashMap<String, ToolSpec>> {
        let tx = self.loading_status_sender.take();
        let display_task = self.loading_display_task.take();
        let tool_specs = {
            let mut tool_specs =
                serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))?;
            if !crate::tools::think::Think::is_enabled() {
                tool_specs.remove("q_think_tool");
            }

            Arc::new(Mutex::new(tool_specs))
        };
        let conversation_id = self.conversation_id.clone();
        let regex = Arc::new(regex::Regex::new(VALID_TOOL_NAME)?);
        let load_tool = self
            .clients
            .iter()
            .map(|(server_name, client)| {
                let client_clone = client.clone();
                let server_name_clone = server_name.clone();
                let tx_clone = tx.clone();
                let regex_clone = regex.clone();
                let tool_specs_clone = tool_specs.clone();
                let conversation_id = conversation_id.clone();
                async move {
                    let tool_spec = client_clone.init().await;
                    let mut sanitized_mapping = HashMap::<String, String>::new();
                    match tool_spec {
                        Ok((server_name, specs)) => {
                            // Each mcp server might have multiple tools.
                            // To avoid naming conflicts we are going to namespace it.
                            // This would also help us locate which mcp server to call the tool from.
                            let mut out_of_spec_tool_names = Vec::<OutOfSpecName>::new();
                            let mut hasher = DefaultHasher::new();
                            let number_of_tools = specs.len();
                            // Sanitize tool names to ensure they comply with the naming requirements:
                            // 1. If the name already matches the regex pattern and doesn't contain the namespace delimiter, use it as is
                            // 2. Otherwise, remove invalid characters and handle special cases:
                            //    - Remove namespace delimiters
                            //    - Ensure the name starts with an alphabetic character
                            //    - Generate a hash-based name if the sanitized result is empty
                            // This ensures all tool names are valid identifiers that can be safely used in the system
                            // If after all of the aforementioned modification the combined tool
                            // name we have exceeds a length of 64, we surface it as an error
                            for mut spec in specs {
                                let sn  = if !regex_clone.is_match(&spec.name) {
                                    let mut sn = sanitize_name(spec.name.clone(), &regex_clone, &mut hasher);
                                    while sanitized_mapping.contains_key(&sn) {
                                        sn.push('1');
                                    }
                                    sn
                                } else {
                                    spec.name.clone()
                                };
                                let full_name = format!("{}{}{}", server_name, NAMESPACE_DELIMITER, sn);
                                if full_name.len() > 64 {
                                    out_of_spec_tool_names.push(OutOfSpecName::TooLong(spec.name));
                                    continue;
                                } else if spec.description.is_empty() {
                                    out_of_spec_tool_names.push(OutOfSpecName::EmptyDescription(spec.name));
                                    continue;
                                }
                                if sn != spec.name {
                                    sanitized_mapping.insert(full_name.clone(), format!("{}{}{}", server_name, NAMESPACE_DELIMITER, spec.name));
                                }
                                spec.name = full_name;
                                spec.tool_origin = ToolOrigin::McpServer(server_name.clone());
                                tool_specs_clone.lock().await.insert(spec.name.clone(), spec);
                            }
                            // Send server load success metric datum
                            tokio::spawn(async move {
                                let event = fig_telemetry::EventType::McpServerInit { conversation_id, init_failure_reason: None, number_of_tools  };
                                let app_event = fig_telemetry::AppTelemetryEvent::new(event).await;
                                fig_telemetry::dispatch_or_send_event(app_event).await;
                            });
                            // Tool name translation. This is beyond of the scope of what is
                            // considered a "server load". Reasoning being:
                            // - Failures here are not related to server load
                            // - There is not a whole lot we can do with this data
                            if let Some(tx_clone) = &tx_clone {
                                let send_result = if !out_of_spec_tool_names.is_empty() {
                                    let msg = out_of_spec_tool_names.iter().fold(
                                        String::from("The following tools are out of spec. They will be excluded from the list of available tools:\n"),
                                        |mut acc, name| {
                                            let (tool_name, msg) = match name {
                                                OutOfSpecName::TooLong(tool_name) => (tool_name.as_str(), "tool name exceeds max length of 64 when combined with server name"),
                                                OutOfSpecName::IllegalChar(tool_name) => (tool_name.as_str(), "tool name must be compliant with ^[a-zA-Z][a-zA-Z0-9_]*$"),
                                                OutOfSpecName::EmptyDescription(tool_name) => (tool_name.as_str(), "tool schema contains empty description"),
                                            };
                                            acc.push_str(format!("  - {} ({})\n", tool_name, msg).as_str());
                                            acc
                                        }
                                    );
                                    tx_clone.send(LoadingMsg::Error {
                                        name: server_name.clone(),
                                        msg: eyre::eyre!(msg),
                                    })
                                    // TODO: if no tools are valid, we need to offload the server
                                    // from the fleet (i.e. kill the server)
                                } else if !sanitized_mapping.is_empty() {
                                    let warn = sanitized_mapping.iter().fold(String::from("The following tool names are changed:\n"), |mut acc, (k, v)| {
                                        acc.push_str(format!(" - {} -> {}\n", v, k).as_str());
                                        acc
                                    });
                                    tx_clone.send(LoadingMsg::Warn {
                                        name: server_name.clone(),
                                        msg: eyre::eyre!(warn),
                                    })
                                } else {
                                    tx_clone.send(LoadingMsg::Done(server_name.clone()))
                                };
                                if let Err(e) = send_result {
                                    error!("Error while sending status update to display task: {:?}", e);
                                }
                            }
                        },
                        Err(e) => {
                            error!("Error obtaining tool spec for {}: {:?}", server_name_clone, e);
                            let init_failure_reason = Some(e.to_string());
                            tokio::spawn(async move {
                                let event = fig_telemetry::EventType::McpServerInit { conversation_id, init_failure_reason, number_of_tools: 0  };
                                let app_event = fig_telemetry::AppTelemetryEvent::new(event).await;
                                fig_telemetry::dispatch_or_send_event(app_event).await;
                            });
                            if let Some(tx_clone) = &tx_clone {
                                if let Err(e) = tx_clone.send(LoadingMsg::Error {
                                    name: server_name_clone,
                                    msg: e,
                                }) {
                                    error!("Error while sending status update to display task: {:?}", e);
                                }
                            }
                        },
                    }
                    Ok::<_, eyre::Report>(Some(sanitized_mapping))
                }
            })
            .collect::<Vec<_>>();
        // TODO: do we want to introduce a timeout here?
        self.tn_map = stream::iter(load_tool)
            .map(|async_closure| tokio::task::spawn(async_closure))
            .buffer_unordered(20)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .flatten()
            .flatten()
            .collect::<HashMap<_, _>>();
        drop(tx);
        if let Some(display_task) = display_task {
            if let Err(e) = display_task.join() {
                error!("Error while joining status display task: {:?}", e);
            }
        }
        let tool_specs = {
            let mutex =
                Arc::try_unwrap(tool_specs).map_err(|e| eyre::eyre!("Error unwrapping arc for tool specs {:?}", e))?;
            mutex.into_inner()
        };
        // caching the tool names for skim operations
        for tool_name in tool_specs.keys() {
            if !self.tn_map.contains_key(tool_name) {
                self.tn_map.insert(tool_name.clone(), tool_name.clone());
            }
        }
        self.schema = tool_specs.clone();
        Ok(tool_specs)
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
            "execute_bash" => Tool::ExecuteBash(serde_json::from_value::<ExecuteBash>(value.args).map_err(map_err)?),
            "use_aws" => Tool::UseAws(serde_json::from_value::<UseAws>(value.args).map_err(map_err)?),
            "report_issue" => Tool::GhIssue(serde_json::from_value::<GhIssue>(value.args).map_err(map_err)?),
            "memory" => Tool::Memory(serde_json::from_value::<Memory>(value.args).map_err(map_err)?),
            "q_think_tool" => {
                Tool::Think(serde_json::from_value::<crate::tools::think::Think>(value.args).map_err(map_err)?)
            },
            // Note that this name is namespaced with server_name{DELIMITER}tool_name
            name => {
                let name = self.tn_map.get(name).map_or(name, String::as_str);
                let (server_name, tool_name) = name.split_once(NAMESPACE_DELIMITER).ok_or(ToolResult {
                    tool_use_id: value.id.clone(),
                    content: vec![ToolResultContentBlock::Text(format!(
                        "The tool, \"{name}\" is supplied with incorrect name"
                    ))],
                    status: ToolResultStatus::Error,
                })?;
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

    #[allow(clippy::await_holding_lock)]
    pub async fn get_prompt(&self, get_command: PromptsGetCommand) -> Result<JsonRpcResponse, GetPromptError> {
        let (server_name, prompt_name) = match get_command.params.name.split_once('/') {
            None => (None::<String>, Some(get_command.params.name.clone())),
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
                                .entry(prompt_name.to_string())
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
                    let PromptsGetCommand { params, .. } = get_command;
                    let PromptBundle { prompt_get, .. } = prompts_wl
                        .get(&prompt_name)
                        .and_then(|bundles| bundles.iter().find(|b| b.server_name == server_name))
                        .ok_or(GetPromptError::MissingPromptInfo)?;
                    // Here we need to convert the positional arguments into key value pair
                    // The assignment order is assumed to be the order of args as they are
                    // presented in PromptGet::arguments
                    let args = if let (Some(schema), Some(value)) = (&prompt_get.arguments, &params.arguments) {
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
                    continue 'blk;
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
                    acc.entry(prompt_name.to_string())
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
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Blue),
        style::Print(format!(" {}", complete)),
        style::ResetColor,
        style::Print(" of "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(format!("{} ", total)),
        style::ResetColor,
        style::Print("mcp servers initialized\n"),
    )?)
}

fn queue_failure_message(name: &str, fail_load_msg: &eyre::Report, output: &mut impl Write) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Red),
        style::Print("✗ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" has failed to load:\n- "),
        style::Print(fail_load_msg),
        style::Print("\n"),
        style::Print("- run with Q_LOG_LEVEL=trace and see $TMPDIR/qlog for detail\n"),
        style::ResetColor,
    )?)
}

fn queue_warn_message(name: &str, msg: &eyre::Report, output: &mut impl Write) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Yellow),
        style::Print("⚠ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" has the following warning:\n"),
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
