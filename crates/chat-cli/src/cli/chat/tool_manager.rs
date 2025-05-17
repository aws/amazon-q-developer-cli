use std::collections::{
    HashMap,
    HashSet,
};
use std::future::Future;
use std::hash::{
    DefaultHasher,
    Hasher,
};
use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::pin::Pin;
use std::sync::atomic::{
    AtomicBool,
    AtomicU32,
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

use convert_case::Casing;
use crossterm::{
    cursor,
    execute,
    queue,
    style,
    terminal,
};
use futures::{
    StreamExt,
    future,
    stream,
};
use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use tokio::signal::ctrl_c;
use tokio::sync::{
    Mutex,
    Notify,
    RwLock,
};
use tracing::{
    error,
    warn,
};

use crate::api_client::model::{
    ToolResult,
    ToolResultContentBlock,
    ToolResultStatus,
};
use crate::cli::chat::command::PromptsGetCommand;
use crate::cli::chat::message::AssistantToolUse;
use crate::cli::chat::server_messenger::{
    ServerMessengerBuilder,
    UpdateEventMessage,
};
use crate::cli::chat::tools::custom_tool::{
    CustomTool,
    CustomToolClient,
    CustomToolConfig,
};
use crate::cli::chat::tools::execute_bash::ExecuteBash;
use crate::cli::chat::tools::fs_read::FsRead;
use crate::cli::chat::tools::fs_write::FsWrite;
use crate::cli::chat::tools::gh_issue::GhIssue;
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
    PromptGet,
};
use crate::platform::Context;
use crate::telemetry::TelemetryThread;
use crate::util::directories::home_dir;

const NAMESPACE_DELIMITER: &str = "___";
// This applies for both mcp server and tool name since in the end the tool name as seen by the
// model is just {server_name}{NAMESPACE_DELIMITER}{tool_name}
const VALID_TOOL_NAME: &str = "^[a-zA-Z][a-zA-Z0-9_]*$";
const SPINNER_CHARS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn workspace_mcp_config_path(ctx: &Context) -> eyre::Result<PathBuf> {
    Ok(ctx.env().current_dir()?.join(".amazonq").join("mcp.json"))
}

pub fn global_mcp_config_path(ctx: &Context) -> eyre::Result<PathBuf> {
    Ok(home_dir(ctx)?.join(".aws").join("amazonq").join("mcp.json"))
}

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

// This is to mirror claude's config set up
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub mcp_servers: HashMap<String, CustomToolConfig>,
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

    pub async fn load_from_file(ctx: &Context, path: impl AsRef<Path>) -> eyre::Result<Self> {
        let contents = ctx.fs().read_to_string(path.as_ref()).await?;
        Ok(serde_json::from_str(&contents)?)
    }

    pub async fn save_to_file(&self, ctx: &Context, path: impl AsRef<Path>) -> eyre::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        ctx.fs().write(path.as_ref(), json).await?;
        Ok(())
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
    is_interactive: bool,
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

    pub fn interactive(mut self, is_interactive: bool) -> Self {
        self.is_interactive = is_interactive;
        self
    }

    pub async fn build(
        mut self,
        telemetry: &TelemetryThread,
        mut output: Box<dyn Write + Send + Sync + 'static>,
    ) -> eyre::Result<ToolManager> {
        let McpServerConfig { mcp_servers } = self.mcp_server_config.ok_or(eyre::eyre!("Missing mcp server config"))?;
        debug_assert!(self.conversation_id.is_some());
        let conversation_id = self.conversation_id.ok_or(eyre::eyre!("Missing conversation id"))?;
        let regex = regex::Regex::new(VALID_TOOL_NAME)?;
        let mut hasher = DefaultHasher::new();
        let is_interactive = self.is_interactive;
        let pre_initialized = mcp_servers
            .into_iter()
            .map(|(server_name, server_config)| {
                let snaked_cased_name = server_name.to_case(convert_case::Case::Snake);
                let sanitized_server_name = sanitize_name(snaked_cased_name, &regex, &mut hasher);
                let custom_tool_client = CustomToolClient::from_config(sanitized_server_name.clone(), server_config);
                (sanitized_server_name, custom_tool_client)
            })
            .collect::<Vec<(String, _)>>();
        let mut loading_servers = HashMap::<String, Instant>::new();
        for (server_name, _) in &pre_initialized {
            let init_time = std::time::Instant::now();
            loading_servers.insert(server_name.clone(), init_time);
        }
        let total = loading_servers.len();
        let completed = Arc::new(AtomicU32::new(0));

        // Spawn a task for displaying the mcp loading statuses.
        // This is only necessary when we are in interactive mode AND there are servers to load.
        // Otherwise we do not need to be spawning this.
        let (_loading_display_task, loading_status_sender) = if is_interactive && total > 0 {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<LoadingMsg>(50);
            (
                Some(tokio::task::spawn(async move {
                    let mut spinner_logo_idx: usize = 0;
                    let mut complete: usize = 0;
                    let mut failed: usize = 0;
                    queue_init_message(spinner_logo_idx, complete, failed, total, &mut output)?;
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
                                LoadingMsg::Error { name, msg, time: _ } => {
                                    failed += 1;
                                    execute!(
                                        output,
                                        cursor::MoveToColumn(0),
                                        cursor::MoveUp(1),
                                        terminal::Clear(terminal::ClearType::CurrentLine),
                                    )?;
                                    queue_failure_message(&name, &msg, &mut output)?;
                                    queue_init_message(spinner_logo_idx, complete, failed, total, &mut output)?;
                                },
                                LoadingMsg::Warn { name, msg, time: _ } => {
                                    complete += 1;
                                    execute!(
                                        output,
                                        cursor::MoveToColumn(0),
                                        cursor::MoveUp(1),
                                        terminal::Clear(terminal::ClearType::CurrentLine),
                                    )?;
                                    let msg = eyre::eyre!(msg.to_string());
                                    queue_warn_message(&name, &msg, &mut output)?;
                                    queue_init_message(spinner_logo_idx, complete, failed, total, &mut output)?;
                                },
                                LoadingMsg::Terminate { still_loading } => {
                                    if !still_loading.is_empty() {
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
        let telemetry_clone = telemetry.clone();
        let notify = Arc::new(Notify::new());
        let notify_weak = Arc::downgrade(&notify);
        let completed_clone = completed.clone();
        tokio::spawn(async move {
            while let Some(msg) = msg_rx.recv().await {
                // For now we will treat every list result as if they contain the
                // complete set of tools. This is not necessarily true in the future when
                // request method on the mcp client no longer buffers all the pages from
                // list calls.
                match msg {
                    UpdateEventMessage::ToolsListResult { server_name, result } => {
                        let time_taken = loading_servers
                            .get(&server_name)
                            .map(|init_time| {
                                let time_taken = (std::time::Instant::now() - *init_time).as_secs_f64().abs();
                                format!("{:.2}", time_taken)
                            })
                            .unwrap_or_default();
                        pending_clone.write().await.remove(&server_name);
                        match result {
                            Ok(result) => {
                                let mut specs = result
                                    .tools
                                    .into_iter()
                                    .filter_map(|v| serde_json::from_value::<ToolSpec>(v).ok())
                                    .collect::<Vec<_>>();
                                let mut sanitized_mapping = HashMap::<String, String>::new();
                                let process_result = process_tool_specs(
                                    conv_id_clone.as_str(),
                                    &server_name,
                                    &mut specs,
                                    &mut sanitized_mapping,
                                    &regex,
                                    &telemetry_clone,
                                );
                                if let Some(sender) = &loading_status_sender_clone {
                                    // Anomalies here are not considered fatal, thus we shall give
                                    // warnings.
                                    let msg = match process_result {
                                        Ok(_) => LoadingMsg::Done {
                                            name: server_name.clone(),
                                            time: time_taken,
                                        },
                                        Err(e) => LoadingMsg::Warn {
                                            name: server_name.clone(),
                                            msg: e,
                                            time: time_taken,
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
                                    .insert(server_name, (sanitized_mapping, specs));
                                has_new_stuff_clone.store(true, Ordering::Release);
                            },
                            Err(e) => {
                                // Errors surfaced at this point (i.e. before [process_tool_specs]
                                // is called) are fatals and should be considered errors
                                if let Some(sender) = &loading_status_sender_clone {
                                    let msg = LoadingMsg::Error {
                                        name: server_name,
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
                            let completed = completed_clone.fetch_add(1, Ordering::AcqRel);
                            if completed + 1 >= (total as u32) {
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
                    },
                }
            }
        });
        for (mut name, init_res) in pre_initialized {
            match init_res {
                Ok(mut client) => {
                    let messenger = messenger_builder.build_with_name(client.get_server_name().to_owned());
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
                    telemetry
                        .send_mcp_server_init(conversation_id.clone(), Some(e.to_string()), 0)
                        .ok();
                    if let Some(tx) = &loading_status_sender {
                        let _ = tx
                            .send(LoadingMsg::Error {
                                name: name.clone(),
                                msg: e,
                                time: String::new(),
                            })
                            .await;
                    }
                    completed.fetch_add(1, Ordering::AcqRel);
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
            pending_clients: pending,
            notify: Some(notify),
            loading_status_sender,
            new_tool_specs,
            has_new_stuff,
            is_interactive,
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

type NewToolSpecs = Arc<Mutex<HashMap<String, (HashMap<String, String>, Vec<ToolSpec>)>>>;

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

    /// Mapping from sanitized tool names to original tool names.
    /// This is used to handle tool name transformations that may occur during initialization
    /// to ensure tool names comply with naming requirements.
    pub tn_map: HashMap<String, String>,

    /// A cache of tool's input schema for all of the available tools.
    /// This is mainly used to show the user what the tools look like from the perspective of the
    /// model.
    pub schema: HashMap<String, ToolSpec>,

    is_interactive: bool,
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
            ..Default::default()
        }
    }
}

impl ToolManager {
    pub async fn load_tools(&mut self, database: &Database) -> eyre::Result<HashMap<String, ToolSpec>> {
        let tx = self.loading_status_sender.take();
        let notify = self.notify.take();
        self.schema = {
            let mut tool_specs =
                serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))?;
            if !crate::cli::chat::tools::thinking::Thinking::is_enabled(database) {
                tool_specs.remove("thinking");
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
            let init_timeout = database
                .settings
                .get_int(Setting::McpInitTimeout)
                .map_or(5000_u64, |s| s as u64);
            Box::pin(tokio::time::sleep(std::time::Duration::from_millis(init_timeout)))
        } else {
            Box::pin(future::pending())
        };
        let server_loading_fut: Pin<Box<dyn Future<Output = ()>>> = if let Some(notify) = notify {
            Box::pin(async move { notify.notified().await })
        } else {
            Box::pin(future::ready(()))
        };
        tokio::select! {
            _ = timeout_fut => {
                if let Some(tx) = tx {
                    let still_loading = self.pending_clients.read().await.iter().cloned().collect::<Vec<_>>();
                    let _ = tx.send(LoadingMsg::Terminate { still_loading }).await;
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
            "execute_bash" => Tool::ExecuteBash(serde_json::from_value::<ExecuteBash>(value.args).map_err(map_err)?),
            "use_aws" => Tool::UseAws(serde_json::from_value::<UseAws>(value.args).map_err(map_err)?),
            "report_issue" => Tool::GhIssue(serde_json::from_value::<GhIssue>(value.args).map_err(map_err)?),
            "thinking" => Tool::Thinking(serde_json::from_value::<Thinking>(value.args).map_err(map_err)?),
            // Note that this name is namespaced with server_name{DELIMITER}tool_name
            name => {
                // Note: tn_map also has tools that underwent no transformation. In otherwords, if
                // it is a valid tool name, we should get a hit.
                let name = match self.tn_map.get(name) {
                    Some(name) => Ok::<&str, ToolResult>(name.as_str()),
                    None => {
                        // There are three possibilities:
                        // - The tool name supplied is valid, it's just missing the server name
                        // prefix.
                        // - The tool name supplied is valid, it's missing the server name prefix
                        // and there are more than one possible tools that fit this description.
                        // - No server has a tool with this name.
                        let candidates = self.tn_map.keys().filter(|n| n.ends_with(name)).collect::<Vec<_>>();
                        #[allow(clippy::comparison_chain)]
                        if candidates.len() == 1 {
                            Ok(candidates.first().map(|s| s.as_str()).unwrap())
                        } else if candidates.len() > 1 {
                            let mut content = candidates.iter().fold(
                                "There are multilple tools with given tool name: ".to_string(),
                                |mut acc, name| {
                                    acc.push_str(name);
                                    acc.push_str(", ");
                                    acc
                                },
                            );
                            content.push_str("specify a tool with its full name.");
                            Err(ToolResult {
                                tool_use_id: value.id.clone(),
                                content: vec![ToolResultContentBlock::Text(content)],
                                status: ToolResultStatus::Error,
                            })
                        } else {
                            Err(ToolResult {
                                tool_use_id: value.id.clone(),
                                content: vec![ToolResultContentBlock::Text(format!(
                                    "The tool, \"{name}\" is supplied with incorrect name"
                                ))],
                                status: ToolResultStatus::Error,
                            })
                        }
                    },
                }?;
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

    /// Updates tool managers various states with new information
    pub async fn update(&mut self) {
        // A hashmap of <tool name, tool spec>
        let mut tool_specs = HashMap::<String, ToolSpec>::new();
        let new_tools = {
            let mut new_tool_specs = self.new_tool_specs.lock().await;
            new_tool_specs.drain().fold(HashMap::new(), |mut acc, (k, v)| {
                acc.insert(k, v);
                acc
            })
        };
        let mut updated_servers = HashSet::<ToolOrigin>::new();
        for (server_name, (tool_name_map, specs)) in new_tools {
            let target = format!("{server_name}{NAMESPACE_DELIMITER}");
            self.tn_map.retain(|k, _| !k.starts_with(&target));
            for (k, v) in tool_name_map {
                self.tn_map.insert(k, v);
            }
            if let Some(spec) = specs.first() {
                updated_servers.insert(spec.tool_origin.clone());
            }
            for spec in specs {
                tool_specs.insert(spec.name.clone(), spec);
            }
        }
        // Caching the tool names for skim operations
        for tool_name in tool_specs.keys() {
            if !self.tn_map.contains_key(tool_name) {
                self.tn_map.insert(tool_name.clone(), tool_name.clone());
            }
        }
        // Update schema
        // As we are writing over the ensemble of tools in a given server, we will need to first
        // remove everything that it has.
        self.schema
            .retain(|_tool_name, spec| !updated_servers.contains(&spec.tool_origin));
        self.schema.extend(tool_specs);
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

    pub async fn pending_clients(&self) -> Vec<String> {
        self.pending_clients.read().await.iter().cloned().collect::<Vec<_>>()
    }
}

#[inline]
fn process_tool_specs(
    conversation_id: &str,
    server_name: &str,
    specs: &mut Vec<ToolSpec>,
    tn_map: &mut HashMap<String, String>,
    regex: &Regex,
    telemetry: &TelemetryThread,
) -> eyre::Result<()> {
    // Each mcp server might have multiple tools.
    // To avoid naming conflicts we are going to namespace it.
    // This would also help us locate which mcp server to call the tool from.
    let mut out_of_spec_tool_names = Vec::<OutOfSpecName>::new();
    let mut hasher = DefaultHasher::new();
    let number_of_tools = specs.len();
    // Sanitize tool names to ensure they comply with the naming requirements:
    // 1. If the name already matches the regex pattern and doesn't contain the namespace delimiter, use
    //    it as is
    // 2. Otherwise, remove invalid characters and handle special cases:
    //    - Remove namespace delimiters
    //    - Ensure the name starts with an alphabetic character
    //    - Generate a hash-based name if the sanitized result is empty
    // This ensures all tool names are valid identifiers that can be safely used in the system
    // If after all of the aforementioned modification the combined tool
    // name we have exceeds a length of 64, we surface it as an error
    for spec in specs {
        let sn = if !regex.is_match(&spec.name) {
            let mut sn = sanitize_name(spec.name.clone(), regex, &mut hasher);
            while tn_map.contains_key(&sn) {
                sn.push('1');
            }
            sn
        } else {
            spec.name.clone()
        };
        let full_name = format!("{}{}{}", server_name, NAMESPACE_DELIMITER, sn);
        if full_name.len() > 64 {
            out_of_spec_tool_names.push(OutOfSpecName::TooLong(spec.name.clone()));
            continue;
        } else if spec.description.is_empty() {
            out_of_spec_tool_names.push(OutOfSpecName::EmptyDescription(spec.name.clone()));
            continue;
        }
        if sn != spec.name {
            tn_map.insert(
                full_name.clone(),
                format!("{}{}{}", server_name, NAMESPACE_DELIMITER, spec.name),
            );
        }
        spec.name = full_name;
        spec.tool_origin = ToolOrigin::McpServer(server_name.to_string());
    }
    // Send server load success metric datum
    let conversation_id = conversation_id.to_string();
    let _ = telemetry.send_mcp_server_init(conversation_id, None, number_of_tools);
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
        // TODO: if no tools are valid, we need to offload the server
        // from the fleet (i.e. kill the server)
    } else if !tn_map.is_empty() {
        Err(eyre::eyre!(tn_map.iter().fold(
            String::from("The following tool names are changed:\n"),
            |mut acc, (k, v)| {
                acc.push_str(format!(" - {} -> {}\n", v, k).as_str());
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

fn queue_failure_message(name: &str, fail_load_msg: &eyre::Report, output: &mut impl Write) -> eyre::Result<()> {
    use crate::util::CHAT_BINARY_NAME;
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Red),
        style::Print("✗ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" has failed to load:\n - "),
        style::Print(fail_load_msg),
        style::Print("\n"),
        style::Print(format!(
            " - run with Q_LOG_LEVEL=trace and see $TMPDIR/{CHAT_BINARY_NAME} for detail\n"
        )),
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
