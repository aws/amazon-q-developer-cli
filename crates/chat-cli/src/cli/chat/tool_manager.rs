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
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
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
use futures::future;
use regex::Regex;
use rmcp::ServiceError;
use rmcp::model::{
    GetPromptRequestParam,
    GetPromptResult,
    Prompt,
};
use tokio::signal::ctrl_c;
use tokio::sync::{
    Mutex,
    Notify,
    RwLock,
};
use tokio::task::JoinHandle;
use tracing::{
    error,
    info,
    warn,
};

use super::tools::custom_tool::CustomToolConfig;
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
use crate::cli::chat::tools::custom_tool::CustomTool;
use crate::cli::chat::tools::execute::ExecuteCommand;
use crate::cli::chat::tools::fs_read::FsRead;
use crate::cli::chat::tools::fs_write::FsWrite;
use crate::cli::chat::tools::gh_issue::GhIssue;
use crate::cli::chat::tools::introspect::Introspect;
use crate::cli::chat::tools::knowledge::Knowledge;
use crate::cli::chat::tools::thinking::Thinking;
use crate::cli::chat::tools::todo::TodoList;
use crate::cli::chat::tools::use_aws::UseAws;
use crate::cli::chat::tools::{
    Tool,
    ToolOrigin,
    ToolSpec,
};
use crate::database::Database;
use crate::database::settings::Setting;
use crate::mcp_client::messenger::Messenger;
use crate::mcp_client::{
    InitializedMcpClient,
    InnerService,
    McpClientService,
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
    /// Indicates that a server requires user authentication and provides a sign-in link.
    /// This message is used to notify the user about authentication requirements for MCP servers
    /// that need OAuth or other authentication methods. Contains the server name and the
    /// authentication message (typically a URL or instructions).
    SignInNotice { name: String },
}

/// Used to denote the loading outcome associated with a server.
/// This is mainly used in the non-interactive mode to determine if there is any fatal errors to
/// surface (since we would only want to surface fatal errors in non-interactive mode).
#[derive(Clone, Debug)]
pub enum LoadingRecord {
    Success(String, String),
    Warn(String, String),
    Err(String, String),
}

impl LoadingRecord {
    pub fn success(msg: String) -> Self {
        let timestamp = chrono::Local::now().format("%Y:%H:%S").to_string();
        LoadingRecord::Success(timestamp, msg)
    }

    pub fn warn(msg: String) -> Self {
        let timestamp = chrono::Local::now().format("%Y:%H:%S").to_string();
        LoadingRecord::Warn(timestamp, msg)
    }

    pub fn err(msg: String) -> Self {
        let timestamp = chrono::Local::now().format("%Y:%H:%S").to_string();
        LoadingRecord::Err(timestamp, msg)
    }
}

pub struct ToolManagerBuilder {
    prompt_query_result_sender: Option<tokio::sync::broadcast::Sender<PromptQueryResult>>,
    prompt_query_receiver: Option<tokio::sync::broadcast::Receiver<PromptQuery>>,
    prompt_query_sender: Option<tokio::sync::broadcast::Sender<PromptQuery>>,
    prompt_query_result_receiver: Option<tokio::sync::broadcast::Receiver<PromptQueryResult>>,
    messenger_builder: Option<ServerMessengerBuilder>,
    conversation_id: Option<String>,
    has_new_stuff: Arc<AtomicBool>,
    mcp_load_record: Arc<Mutex<HashMap<String, Vec<LoadingRecord>>>>,
    new_tool_specs: NewToolSpecs,
    pending_clients: Option<Arc<RwLock<HashSet<String>>>>,
    is_first_launch: bool,
    agent: Option<Arc<Mutex<Agent>>>,
}

impl Default for ToolManagerBuilder {
    fn default() -> Self {
        Self {
            prompt_query_result_sender: Default::default(),
            prompt_query_receiver: Default::default(),
            prompt_query_sender: Default::default(),
            prompt_query_result_receiver: Default::default(),
            messenger_builder: Default::default(),
            conversation_id: Default::default(),
            has_new_stuff: Default::default(),
            mcp_load_record: Default::default(),
            new_tool_specs: Default::default(),
            pending_clients: Default::default(),
            is_first_launch: true,
            agent: Default::default(),
        }
    }
}

impl From<&mut ToolManager> for ToolManagerBuilder {
    fn from(value: &mut ToolManager) -> Self {
        Self {
            conversation_id: Some(value.conversation_id.clone()),
            agent: Some(value.agent.clone()),
            prompt_query_sender: value
                .prompts_sender_receiver_pair
                .as_ref()
                .map(|(sender, _)| sender.clone()),
            prompt_query_result_receiver: value.prompts_sender_receiver_pair.take().map(|(_, receiver)| receiver),
            messenger_builder: value.messenger_builder.take(),
            has_new_stuff: value.has_new_stuff.clone(),
            mcp_load_record: value.mcp_load_record.clone(),
            new_tool_specs: value.new_tool_specs.clone(),
            pending_clients: Some(value.pending_clients.clone()),
            // if we are getting a builder from an instantiated tool manager this field would be
            // false
            is_first_launch: false,
            ..Default::default()
        }
    }
}

impl ToolManagerBuilder {
    pub fn prompt_query_result_sender(mut self, sender: tokio::sync::broadcast::Sender<PromptQueryResult>) -> Self {
        self.prompt_query_result_sender.replace(sender);
        self
    }

    pub fn prompt_query_receiver(mut self, receiver: tokio::sync::broadcast::Receiver<PromptQuery>) -> Self {
        self.prompt_query_receiver.replace(receiver);
        self
    }

    pub fn prompt_query_sender(mut self, sender: tokio::sync::broadcast::Sender<PromptQuery>) -> Self {
        self.prompt_query_sender.replace(sender);
        self
    }

    pub fn prompt_query_result_receiver(
        mut self,
        receiver: tokio::sync::broadcast::Receiver<PromptQueryResult>,
    ) -> Self {
        self.prompt_query_result_receiver.replace(receiver);
        self
    }

    pub fn conversation_id(mut self, conversation_id: &str) -> Self {
        self.conversation_id.replace(conversation_id.to_string());
        self
    }

    pub fn agent(mut self, agent: Agent) -> Self {
        let agent = Arc::new(Mutex::new(agent));
        self.agent.replace(agent);
        self
    }

    /// Creates a [ToolManager] based on the current fields populated, which consists of the
    /// following:
    /// - Instantiates child processes associated with the list of mcp servers in scope
    /// - Spawns a loading display task that is used to show server loading status (if applicable)
    /// - Spawns the orchestrator task (see [spawn_orchestrator_task] for more detail) (if
    ///   applicable)
    /// - Finally, creates an instance of [ToolManager]
    pub async fn build(
        mut self,
        os: &mut Os,
        mut output: Box<dyn Write + Send + Sync + 'static>,
        interactive: bool,
    ) -> eyre::Result<ToolManager> {
        let McpServerConfig { mcp_servers } = match &self.agent {
            Some(agent) => agent.lock().await.mcp_servers.clone(),
            None => Default::default(),
        };
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
            .iter()
            .filter(|(server_name, _)| {
                if server_name == "builtin" {
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
                    false
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();

        let mut clients = HashMap::<String, InitializedMcpClient>::new();
        let new_tool_specs = self.new_tool_specs;
        let has_new_stuff = self.has_new_stuff;
        let pending = self.pending_clients.unwrap_or(Arc::new(RwLock::new({
            let mut pending = HashSet::<String>::new();
            pending.extend(pre_initialized.iter().map(|(name, _)| name.clone()));
            pending
        })));
        let notify = Arc::new(Notify::new());
        let load_record = self.mcp_load_record;
        let agent = self.agent.unwrap_or_default();
        let database = os.database.clone();
        let mut messenger_builder = self.messenger_builder.take();

        let mut loading_servers = HashMap::<String, Instant>::new();
        for (server_name, _) in &pre_initialized {
            let init_time = std::time::Instant::now();
            loading_servers.insert(server_name.clone(), init_time);
        }
        let total = loading_servers.len();

        // Spawn a task for displaying the mcp loading statuses.
        // This is only necessary when we are in interactive mode AND there are servers to load.
        // Otherwise we do not need to be spawning this.
        let (loading_display_task, loading_status_sender) =
            spawn_display_task(interactive, total, disabled_servers, output);

        // This is the orchestrator task that serves as a bridge between tool manager and mcp
        // clients for server initiated async events
        if let (Some(prompt_list_sender), Some(prompt_list_receiver)) = (
            self.prompt_query_result_sender.clone(),
            self.prompt_query_receiver.as_ref().map(|r| r.resubscribe()),
        ) {
            let (msg_rx, builder) = ServerMessengerBuilder::new(20);
            messenger_builder.replace(builder);

            let has_new_stuff = has_new_stuff.clone();
            let notify_weak = Arc::downgrade(&notify);
            let telemetry = os.telemetry.clone();
            let loading_status_sender = loading_status_sender.clone();
            let new_tool_specs = new_tool_specs.clone();
            let conv_id = conversation_id.clone();
            let pending = pending.clone();
            let regex = Regex::new(VALID_TOOL_NAME)?;

            spawn_orchestrator_task(
                has_new_stuff,
                loading_servers,
                msg_rx,
                prompt_list_receiver,
                prompt_list_sender,
                pending,
                agent.clone(),
                database,
                regex,
                notify_weak,
                load_record.clone(),
                telemetry,
                loading_status_sender,
                new_tool_specs,
                total,
                conv_id,
            );
        }

        debug_assert!(messenger_builder.is_some());
        let messenger_builder = messenger_builder.unwrap();
        let pre_initialized = enabled_servers
            .into_iter()
            .map(|(server_name, server_config)| {
                (
                    server_name.clone(),
                    McpClientService::new(
                        server_name.clone(),
                        server_config,
                        messenger_builder.build_with_name(server_name),
                    ),
                )
            })
            .collect::<Vec<_>>();

        for (mut name, mcp_client) in pre_initialized {
            let init_res = mcp_client.init(os).await;
            match init_res {
                Ok(mut running_service) => {
                    while let Some(collided_service) = clients.insert(name.clone(), running_service) {
                        // to avoid server name collision we are going to circumvent this by
                        // appending the name with 1
                        name.push('1');
                        running_service = collided_service;
                    }
                },
                Err(e) => {
                    error!("Error initializing mcp client for server {}: {:?}", name, &e);
                    os.telemetry
                        .send_mcp_server_init(
                            &os.database,
                            conversation_id.clone(),
                            name.clone(),
                            Some(e.to_string()),
                            0,
                            Some("".to_string()),
                            Some("".to_string()),
                            0,
                        )
                        .await
                        .ok();

                    let temp_messenger = messenger_builder.build_with_name(name);
                    let _ = temp_messenger
                        .send_tools_list_result(Err(ServiceError::UnexpectedResponse), None)
                        .await;
                },
            }
        }

        Ok(ToolManager {
            conversation_id,
            clients,
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
            prompts_sender_receiver_pair: {
                if let (Some(sender), Some(receiver)) = (self.prompt_query_sender, self.prompt_query_result_receiver) {
                    Some((sender, receiver))
                } else {
                    None
                }
            },
            messenger_builder: Some(messenger_builder),
            is_first_launch: self.is_first_launch,
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
    pub prompt_get: Prompt,
}

#[derive(Clone, Debug)]
pub enum PromptQuery {
    List,
    Search(Option<String>),
}

#[derive(Clone, Debug)]
pub enum PromptQueryResult {
    List(HashMap<String, Vec<PromptBundle>>),
    Search(Vec<String>),
}

/// Categorizes different types of tool name validation failures:
/// - `TooLong`: The tool name exceeds the maximum allowed length
/// - `IllegalChar`: The tool name contains characters that are not allowed
/// - `EmptyDescription`: The tool description is empty or missing
#[allow(dead_code)]
enum ToolValidationViolation {
    TooLong(String),
    IllegalChar(String),
    EmptyDescription(String),
    DescriptionTooLong(String),
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

/// A pair of channels used for prompt list communication between the tool manager and chat helper.
/// The sender broadcasts a list of available prompt names, while the receiver listens for
/// search queries to filter the prompt list.
type PromptsChannelPair = (
    tokio::sync::broadcast::Sender<PromptQuery>,
    tokio::sync::broadcast::Receiver<PromptQueryResult>,
);

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
    pub clients: HashMap<String, InitializedMcpClient>,

    /// A list of client names that are still in the process of being initialized
    pub pending_clients: Arc<RwLock<HashSet<String>>>,

    /// Flag indicating whether new tool specifications have been added since the last update.
    /// When set to true, it signals that the tool manager needs to refresh its internal state
    /// to incorporate newly available tools from MCP servers.
    pub has_new_stuff: Arc<AtomicBool>,

    /// Used by methods on the [ToolManager] to retrieve information from the orchestrator thread
    prompts_sender_receiver_pair: Option<PromptsChannelPair>,

    /// Storage for newly discovered tool specifications from MCP servers that haven't yet been
    /// integrated into the main tool registry. This field holds a thread-safe reference to a map
    /// of server names to their tool specifications and name mappings, allowing concurrent updates
    /// from server initialization processes.
    new_tool_specs: NewToolSpecs,

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

    /// A builder for mcp clients to communicate with the orchestrator task
    /// We need to store this for when we switch agent - we need to be spawning messengers that are
    /// already listened to by the orchestrator task
    messenger_builder: Option<ServerMessengerBuilder>,

    /// A collection of preferences that pertains to the conversation
    /// As far as tool manager goes, this is relevant for tool and server filters
    /// We need to put this behind a lock because the orchestrator task depends on agent
    pub agent: Arc<Mutex<Agent>>,

    is_first_launch: bool,
}

impl Clone for ToolManager {
    fn clone(&self) -> Self {
        Self {
            conversation_id: self.conversation_id.clone(),
            has_new_stuff: self.has_new_stuff.clone(),
            new_tool_specs: self.new_tool_specs.clone(),
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
    /// Swapping agent involves the following:
    /// - Dropping all of the clients first to avoid resource contention
    /// - Clearing fields that are already referenced by background tasks. We can't simply spawn new
    ///   instances of these fields because one or more background tasks are already depending on it
    /// - Building a new tool manager builder from the current tool manager
    /// - Building a tool manager from said tool manager builder
    /// - Swapping the old with the new (the old would be dropped after we exit the scope of this
    ///   function)
    /// - Calling load tools
    pub async fn swap_agent(&mut self, os: &mut Os, output: &mut impl Write, agent: &Agent) -> eyre::Result<()> {
        let to_evict = self.clients.drain().collect::<Vec<_>>();
        tokio::spawn(async move {
            for (server_name, initialized_client) in to_evict {
                info!("Evicting {server_name} due to agent swap");
                match initialized_client {
                    InitializedMcpClient::Pending(handle) => {
                        let server_name_clone = server_name.clone();
                        tokio::spawn(async move {
                            match handle.await {
                                Ok(Ok(client)) => {
                                    let InnerService::Original(client) = client.inner_service else {
                                        unreachable!();
                                    };
                                    match client.cancel().await {
                                        Ok(_) => info!("Server {server_name_clone} evicted due to agent swap"),
                                        Err(e) => error!("Server {server_name_clone} has failed to cancel: {e}"),
                                    }
                                },
                                Ok(Err(_)) | Err(_) => {
                                    error!("Server {server_name_clone} has failed to cancel");
                                },
                            }
                        });
                    },
                    InitializedMcpClient::Ready(running_service) => {
                        let InnerService::Original(client) = running_service.inner_service else {
                            unreachable!();
                        };
                        match client.cancel().await {
                            Ok(_) => info!("Server {server_name} evicted due to agent swap"),
                            Err(e) => error!("Server {server_name} has failed to cancel: {e}"),
                        }
                    },
                }
            }
        });

        let mut agent_lock = self.agent.lock().await;
        *agent_lock = agent.clone();
        drop(agent_lock);

        self.mcp_load_record.lock().await.clear();

        let builder = ToolManagerBuilder::from(&mut *self);
        let mut new_tool_manager = builder.build(os, Box::new(std::io::sink()), true).await?;
        std::mem::swap(self, &mut new_tool_manager);

        self.load_tools(os, output).await?;

        Ok(())
    }

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
                        name == DUMMY_TOOL_NAME
                            || is_allow_all
                            || is_allow_native
                            || tool_list.contains(name)
                            || tool_list.contains(&format!("@builtin/{name}"))
                    })
                    .collect::<HashMap<_, _>>();
            if !crate::cli::chat::tools::thinking::Thinking::is_enabled(os) {
                tool_specs.remove("thinking");
            }
            if !crate::cli::chat::tools::knowledge::Knowledge::is_enabled(os) {
                tool_specs.remove("knowledge");
            }
            if !crate::cli::chat::tools::todo::TodoList::is_enabled(os) {
                tool_specs.remove("todo_list");
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

        // We need to cast it to erase the type otherwise the compiler will default to static
        // dispatch, which would result in an error of inconsistent match arm return type.
        let timeout_fut: Pin<Box<dyn Future<Output = ()>>> = if self.clients.is_empty() || !self.is_first_launch {
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
                .any(|(_, records)| records.iter().any(|record| matches!(record, LoadingRecord::Err(..))))
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

    pub async fn get_tool_from_tool_use(&mut self, value: AssistantToolUse) -> Result<Tool, ToolResult> {
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
            "introspect" => Tool::Introspect(serde_json::from_value::<Introspect>(value.args).map_err(map_err)?),
            "thinking" => Tool::Thinking(serde_json::from_value::<Thinking>(value.args).map_err(map_err)?),
            "knowledge" => Tool::Knowledge(serde_json::from_value::<Knowledge>(value.args).map_err(map_err)?),
            "todo_list" => Tool::Todo(serde_json::from_value::<TodoList>(value.args).map_err(map_err)?),
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
                let Some(client) = self.clients.get_mut(server_name) else {
                    return Err(ToolResult {
                        tool_use_id: value.id,
                        content: vec![ToolResultContentBlock::Text(format!(
                            "The tool, \"{server_name}\" is not supported by the client"
                        ))],
                        status: ToolResultStatus::Error,
                    });
                };

                let running_service = client.get_running_service().await.map_err(|e| ToolResult {
                    tool_use_id: value.id.clone(),
                    content: vec![ToolResultContentBlock::Text(format!("Mcp tool client not ready: {e}"))],
                    status: ToolResultStatus::Error,
                })?;

                Tool::Custom(CustomTool {
                    name: tool_name.to_owned(),
                    server_name: server_name.to_owned(),
                    client: running_service.clone(),
                    params: value.args.as_object().cloned(),
                })
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
                let record = LoadingRecord::err(msg);
                record_lock
                    .entry(server_name)
                    .and_modify(|v| v.push(record.clone()))
                    .or_insert(vec![record]);
            }
        }
    }

    pub async fn list_prompts(&self) -> Result<HashMap<String, Vec<PromptBundle>>, GetPromptError> {
        if let Some((query_sender, query_result_receiver)) = &self.prompts_sender_receiver_pair {
            let mut new_receiver = query_result_receiver.resubscribe();
            query_sender
                .send(PromptQuery::List)
                .map_err(|e| GetPromptError::General(eyre::eyre!(e)))?;
            let query_result = new_receiver
                .recv()
                .await
                .map_err(|e| GetPromptError::General(eyre::eyre!(e)))?;

            Ok(match query_result {
                PromptQueryResult::List(list) => list,
                PromptQueryResult::Search(_) => return Err(GetPromptError::IncorrectResponseType),
            })
        } else {
            Err(GetPromptError::MissingChannel)
        }
    }

    pub async fn get_prompt(
        &mut self,
        name: String,
        arguments: Option<Vec<String>>,
    ) -> Result<GetPromptResult, GetPromptError> {
        let (server_name, prompt_name) = match name.split_once('/') {
            None => (None::<String>, Some(name.clone())),
            Some((server_name, prompt_name)) => (Some(server_name.to_string()), Some(prompt_name.to_string())),
        };
        let prompt_name = prompt_name.ok_or(GetPromptError::MissingPromptName)?;

        if let Some((query_sender, query_result_receiver)) = &self.prompts_sender_receiver_pair {
            query_sender
                .send(PromptQuery::List)
                .map_err(|e| GetPromptError::General(eyre::eyre!(e)))?;
            let prompts = query_result_receiver
                .resubscribe()
                .recv()
                .await
                .map_err(|e| GetPromptError::General(eyre::eyre!(e)))?;
            let PromptQueryResult::List(prompts) = prompts else {
                return Err(GetPromptError::IncorrectResponseType);
            };

            match (prompts.get(&prompt_name), server_name.as_ref()) {
                // If we have more than one eligible clients but no server name specified
                (Some(bundles), None) if bundles.len() > 1 => {
                    Err(GetPromptError::AmbiguousPrompt(prompt_name.clone(), {
                        bundles.iter().fold("\n".to_string(), |mut acc, b| {
                            acc.push_str(&format!("- @{}/{}\n", b.server_name, prompt_name));
                            acc
                        })
                    }))
                },
                // Normal case where we have enough info to proceed
                // Note that if bundle exists, it should never be empty
                (Some(bundles), sn) => {
                    let bundle = if bundles.len() > 1 {
                        let Some(sn) = sn else {
                            return Err(GetPromptError::AmbiguousPrompt(prompt_name.clone(), {
                                bundles.iter().fold("\n".to_string(), |mut acc, b| {
                                    acc.push_str(&format!("- @{}/{}\n", b.server_name, prompt_name));
                                    acc
                                })
                            }));
                        };
                        let bundle = bundles.iter().find(|b| b.server_name == *sn);
                        match bundle {
                            Some(bundle) => bundle,
                            None => {
                                return Err(GetPromptError::AmbiguousPrompt(prompt_name.clone(), {
                                    bundles.iter().fold("\n".to_string(), |mut acc, b| {
                                        acc.push_str(&format!("- @{}/{}\n", b.server_name, prompt_name));
                                        acc
                                    })
                                }));
                            },
                        }
                    } else {
                        bundles.first().ok_or(GetPromptError::MissingPromptInfo)?
                    };

                    let server_name = &bundle.server_name;
                    let client = self.clients.get_mut(server_name).ok_or(GetPromptError::MissingClient)?;
                    let PromptBundle { prompt_get, .. } = bundle;
                    let arguments = if let (Some(schema), Some(value)) = (&prompt_get.arguments, &arguments) {
                        let params = schema.iter().zip(value.iter()).fold(
                            HashMap::<String, String>::new(),
                            |mut acc, (prompt_get_arg, value)| {
                                acc.insert(prompt_get_arg.name.clone(), value.clone());
                                acc
                            },
                        );
                        Some(
                            params
                                .into_iter()
                                .map(|(k, v)| (k, serde_json::Value::String(v)))
                                .collect(),
                        )
                    } else {
                        None
                    };

                    let params = GetPromptRequestParam { name, arguments };
                    let running_service = client.get_running_service().await?;
                    let resp = running_service.get_prompt(params).await?;

                    Ok(resp)
                },
                (None, _) => Err(GetPromptError::PromptNotFound(prompt_name)),
            }
        } else {
            Err(GetPromptError::MissingChannel)
        }
    }

    pub async fn pending_clients(&self) -> Vec<String> {
        self.pending_clients.read().await.iter().cloned().collect::<Vec<_>>()
    }
}

type DisplayTaskJoinHandle = JoinHandle<Result<(), eyre::Report>>;
type LoadingStatusSender = tokio::sync::mpsc::Sender<LoadingMsg>;

/// This function spawns a background task whose sole responsibility is to listen for incoming
/// server loading status and display them to the output.
/// It returns a join handle to the task as well as a sender with which loading status is to be
/// reported.
fn spawn_display_task(
    interactive: bool,
    total: usize,
    disabled_servers: Vec<(String, CustomToolConfig)>,
    mut output: Box<dyn Write + Send + Sync + 'static>,
) -> (Option<DisplayTaskJoinHandle>, Option<LoadingStatusSender>) {
    if interactive && (total > 0 || !disabled_servers.is_empty()) {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<LoadingMsg>(50);
        (
            Some(tokio::task::spawn(async move {
                let mut spinner_logo_idx: usize = 0;
                let mut complete: usize = 0;
                let mut failed: usize = 0;

                // Show disabled servers immediately
                for (server_name, _) in &disabled_servers {
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
                            LoadingMsg::SignInNotice { name } => {
                                execute!(
                                    output,
                                    cursor::MoveToColumn(0),
                                    cursor::MoveUp(1),
                                    terminal::Clear(terminal::ClearType::CurrentLine),
                                )?;
                                queue_oauth_message(&name, &mut output)?;
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
    }
}

/// This function spawns the orchestrator task that has the following responsibilities:
/// - Listens for server driven events (see [UpdateEventMessage] for a list of current applicable
///   events). These are things such as tool list (because we fetch tools in the background), prompt
///   list, tool list update, and prompt list updates. In the future, if when we support sampling
///   and we have not yet moved to the official rust MCP crate, we would also be using this task to
///   facilitate it.
/// - Listens for prompt list request and serve them. Unlike tools, we do *not* cache prompts on the
///   conversation state. This is because prompts do not need to be sent to the model every turn.
///   Instead, the prompts are cached in a hashmap that is owned by the orchestrator task.
///
/// Note that there should be exactly one instance of this task running per session. Should there
/// be any need to instantiate a new [ToolManager] (e.g. swapping agents), see
/// [ToolManager::swap_agent] for how this should be done.
#[allow(clippy::too_many_arguments)]
fn spawn_orchestrator_task(
    has_new_stuff: Arc<AtomicBool>,
    mut loading_servers: HashMap<String, Instant>,
    mut msg_rx: tokio::sync::mpsc::Receiver<UpdateEventMessage>,
    mut prompt_list_receiver: tokio::sync::broadcast::Receiver<PromptQuery>,
    mut prompt_list_sender: tokio::sync::broadcast::Sender<PromptQueryResult>,
    pending: Arc<RwLock<HashSet<String>>>,
    agent: Arc<Mutex<Agent>>,
    database: Database,
    regex: Regex,
    notify_weak: std::sync::Weak<Notify>,
    load_record: Arc<Mutex<HashMap<String, Vec<LoadingRecord>>>>,
    telemetry: TelemetryThread,
    loading_status_sender: Option<LoadingStatusSender>,
    new_tool_specs: NewToolSpecs,
    total: usize,
    conv_id: String,
) {
    tokio::spawn(async move {
        use tokio::sync::broadcast::Sender as BroadcastSender;
        use tokio::sync::mpsc::Sender as MpscSender;

        let mut record_temp_buf = Vec::<u8>::new();
        let mut initialized = HashSet::<String>::new();
        let mut prompts = HashMap::<String, Vec<PromptBundle>>::new();

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

        // We separate this into its own function for ease of maintenance since things written
        // in select arms don't have type hints
        #[inline]
        async fn handle_prompt_queries(
            query: PromptQuery,
            prompts: &HashMap<String, Vec<PromptBundle>>,
            prompt_query_response_sender: &mut BroadcastSender<PromptQueryResult>,
        ) {
            match query {
                PromptQuery::List => {
                    let query_res = PromptQueryResult::List(prompts.clone());
                    if let Err(e) = prompt_query_response_sender.send(query_res) {
                        error!("Error sending prompts to chat helper: {:?}", e);
                    }
                },
                PromptQuery::Search(search_word) => {
                    let filtered_prompts = prompts
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

                    let query_res = PromptQueryResult::Search(filtered_prompts);
                    if let Err(e) = prompt_query_response_sender.send(query_res) {
                        error!("Error sending prompts to chat helper: {:?}", e);
                    }
                },
            }
        }

        // We separate this into its own function for ease of maintenance since things written
        // in select arms don't have type hints
        #[inline]
        #[allow(clippy::too_many_arguments)]
        async fn handle_messenger_msg(
            msg: UpdateEventMessage,
            loading_servers: &mut HashMap<String, Instant>,
            record_temp_buf: &mut Vec<u8>,
            pending: &Arc<RwLock<HashSet<String>>>,
            agent: &Arc<Mutex<Agent>>,
            database: &Database,
            conv_id: &str,
            regex: &Regex,
            telemetry_clone: &TelemetryThread,
            mut loading_status_sender: Option<&MpscSender<LoadingMsg>>,
            new_tool_specs: &NewToolSpecs,
            has_new_stuff: &Arc<AtomicBool>,
            load_record: &Arc<Mutex<HashMap<String, Vec<LoadingRecord>>>>,
            notify_weak: &std::sync::Weak<Notify>,
            initialized: &mut HashSet<String>,
            prompts: &mut HashMap<String, Vec<PromptBundle>>,
            total: usize,
        ) {
            record_temp_buf.clear();
            // For now we will treat every list result as if they contain the
            // complete set of tools. This is not necessarily true in the future when
            // request method on the mcp client no longer buffers all the pages from
            // list calls.
            match msg {
                UpdateEventMessage::ListToolsResult {
                    server_name,
                    result,
                    peer,
                } => {
                    let time_taken = loading_servers
                        .remove(&server_name)
                        .map_or("0.0".to_owned(), |init_time| {
                            let time_taken = (std::time::Instant::now() - init_time).as_secs_f64().abs();
                            format!("{:.2}", time_taken)
                        });
                    pending.write().await.remove(&server_name);

                    let result_tools = match &result {
                        Ok(tools_result) => {
                            let names: Vec<String> =
                                tools_result.tools.iter().map(|tool| tool.name.to_string()).collect();
                            names
                        },
                        Err(_) => vec![],
                    };

                    let (tool_filter, alias_list) = {
                        let agent_lock = agent.lock().await;

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
                                    if let Some((_, host_tool_name)) = full_path.split_once(MCP_SERVER_TOOL_DELIMITER) {
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
                            if let Some(peer) = peer {
                                if peer.is_transport_closed() {
                                    error!(
                                        "Received tool list result from {server_name} but transport has been closed. Ignoring."
                                    );
                                    return;
                                }
                            } else {
                                error!("Received tool list result from {server_name} without a peer. Ignoring.");
                                return;
                            }

                            let mut specs = result
                                .tools
                                .into_iter()
                                .map(|v| ToolSpec {
                                    name: v.name.to_string(),
                                    description: v.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                                    input_schema: crate::cli::chat::tools::InputSchema(v.schema_as_json_value()),
                                    tool_origin: ToolOrigin::Native,
                                })
                                .filter(|spec| tool_filter.should_include(&spec.name))
                                .collect::<Vec<_>>();
                            let mut sanitized_mapping = HashMap::<ModelToolName, ToolInfo>::new();
                            let process_result = process_tool_specs(
                                database,
                                conv_id,
                                &server_name,
                                &mut specs,
                                &mut sanitized_mapping,
                                &alias_list,
                                regex,
                                telemetry_clone,
                                &result_tools,
                            )
                            .await;

                            if let Some(sender) = &loading_status_sender {
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
                                    loading_status_sender.take();
                                }
                            }
                            new_tool_specs
                                .lock()
                                .await
                                .insert(server_name.clone(), (sanitized_mapping, specs));
                            has_new_stuff.store(true, Ordering::Release);
                            // Maintain a record of the server load:
                            let mut buf_writer = BufWriter::new(&mut *record_temp_buf);
                            if let Err(e) = &process_result {
                                let _ =
                                    queue_warn_message(server_name.as_str(), e, time_taken.as_str(), &mut buf_writer);
                            } else {
                                let _ =
                                    queue_success_message(server_name.as_str(), time_taken.as_str(), &mut buf_writer);
                            }
                            let _ = buf_writer.flush();
                            drop(buf_writer);
                            let record = String::from_utf8_lossy(record_temp_buf).to_string();
                            let record = if process_result.is_err() {
                                LoadingRecord::warn(record)
                            } else {
                                LoadingRecord::success(record)
                            };
                            load_record
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
                            let mut buf_writer = BufWriter::new(&mut *record_temp_buf);
                            let fail_load_msg = eyre::eyre!("{}", e);
                            let _ = queue_failure_message(
                                server_name.as_str(),
                                &fail_load_msg,
                                &time_taken,
                                &mut buf_writer,
                            );
                            let _ = buf_writer.flush();
                            drop(buf_writer);
                            let record = String::from_utf8_lossy(record_temp_buf).to_string();
                            let record = LoadingRecord::err(record);
                            load_record
                                .lock()
                                .await
                                .entry(server_name.clone())
                                .and_modify(|load_record| {
                                    load_record.push(record.clone());
                                })
                                .or_insert(vec![record]);
                            // Errors surfaced at this point (i.e. before [process_tool_specs]
                            // is called) are fatals and should be considered errors
                            if let Some(sender) = &loading_status_sender {
                                let msg = LoadingMsg::Error {
                                    name: server_name.clone(),
                                    msg: eyre::eyre!("{}", e.to_string()),
                                    time: time_taken,
                                };
                                if let Err(e) = sender.send(msg).await {
                                    warn!(
                                        "Error sending update message to display task: {:?}\nAssume display task has completed",
                                        e
                                    );
                                    loading_status_sender.take();
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
                UpdateEventMessage::ListPromptsResult {
                    server_name,
                    result,
                    peer,
                } => match result {
                    Ok(prompt_list_result) => {
                        if let Some(peer) = peer {
                            if peer.is_transport_closed() {
                                error!(
                                    "Received prompt list result from {server_name} but transport has been closed. Ignoring."
                                );
                                return;
                            }
                        } else {
                            error!("Received prompt list result from {server_name} without a peer. Ignoring.");
                            return;
                        }
                        // We first need to clear all the PromptGets that are associated with
                        // this server because PromptsListResult is declaring what is available
                        // (and not the diff)
                        prompts
                            .values_mut()
                            .for_each(|bundles| bundles.retain(|bundle| bundle.server_name != server_name));

                        // And then we update them with the new comers
                        for prompt in prompt_list_result.prompts {
                            prompts
                                .entry(prompt.name.clone())
                                .and_modify(|bundles| {
                                    bundles.push(PromptBundle {
                                        server_name: server_name.clone(),
                                        prompt_get: prompt.clone(),
                                    });
                                })
                                .or_insert_with(|| {
                                    vec![PromptBundle {
                                        server_name: server_name.clone(),
                                        prompt_get: prompt,
                                    }]
                                });
                        }
                    },
                    Err(e) => {
                        error!("Error fetching prompts from server {server_name}: {:?}", e);
                        let mut buf_writer = BufWriter::new(&mut *record_temp_buf);
                        let msg = eyre::eyre!("{}", e);
                        let _ = queue_prompts_load_error_message(&server_name, &msg, &mut buf_writer);
                        let _ = buf_writer.flush();
                        drop(buf_writer);
                        let record = String::from_utf8_lossy(record_temp_buf).to_string();
                        let record = LoadingRecord::err(record);
                        load_record
                            .lock()
                            .await
                            .entry(server_name.clone())
                            .and_modify(|load_record| {
                                load_record.push(record.clone());
                            })
                            .or_insert(vec![record]);
                    },
                },
                UpdateEventMessage::ListResourcesResult { .. } => {},
                UpdateEventMessage::ResourceTemplatesListResult { .. } => {},
                UpdateEventMessage::OauthLink { server_name, link } => {
                    let mut buf_writer = BufWriter::new(&mut *record_temp_buf);
                    let msg = eyre::eyre!(link);
                    let _ = queue_oauth_message_with_link(server_name.as_str(), &msg, &mut buf_writer);
                    let _ = buf_writer.flush();
                    drop(buf_writer);
                    let record_str = String::from_utf8_lossy(record_temp_buf).to_string();
                    let record = LoadingRecord::warn(record_str.clone());
                    load_record
                        .lock()
                        .await
                        .entry(server_name.clone())
                        .and_modify(|load_record| {
                            load_record.push(record.clone());
                        })
                        .or_insert(vec![record]);
                    if let Some(sender) = &loading_status_sender {
                        let msg = LoadingMsg::SignInNotice {
                            name: server_name.clone(),
                        };
                        if let Err(e) = sender.send(msg).await {
                            warn!(
                                "Error sending update message to display task: {:?}\nAssume display task has completed",
                                e
                            );
                            loading_status_sender.take();
                        }
                    }
                },
                UpdateEventMessage::InitStart { server_name, .. } => {
                    pending.write().await.insert(server_name.clone());
                    loading_servers.insert(server_name, std::time::Instant::now());
                },
                UpdateEventMessage::Deinit { server_name, .. } => {
                    // Only prompts are stored here so we'll just be clearing that
                    // In the future if we are also storing tools, we need to make sure that
                    // the tools are also pruned.
                    for (_prompt_name, bundles) in prompts.iter_mut() {
                        bundles.retain(|bundle| bundle.server_name != server_name);
                    }
                    prompts.retain(|_, bundles| !bundles.is_empty());
                    has_new_stuff.store(true, Ordering::Release);
                },
            }
        }

        loop {
            tokio::select! {
                Ok(query) = prompt_list_receiver.recv() => {
                    handle_prompt_queries(query, &prompts, &mut prompt_list_sender).await;
                },
                Some(msg) = msg_rx.recv() => {
                    handle_messenger_msg(
                            msg,
                            &mut loading_servers,
                            &mut record_temp_buf,
                            &pending,
                            &agent,
                            &database,
                            conv_id.as_str(),
                            &regex,
                            &telemetry,
                            loading_status_sender.as_ref(),
                            &new_tool_specs,
                            &has_new_stuff,
                            &load_record,
                            &notify_weak,
                            &mut initialized,
                            &mut prompts,
                            total
                        ).await;
                },
                // Nothing else to poll
                else => {
                    tracing::info!("Tool manager orchestrator task exited");
                    break;
                },
            }
        }
    });
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
    result_tools: &[String],
) -> eyre::Result<()> {
    // Tools are subjected to the following validations:
    // 1. ^[a-zA-Z][a-zA-Z0-9_]*$,
    // 2. less than 64 characters in length
    // 3. a non-empty description
    //
    // For non-compliance due to point 1, we shall change it on behalf of the users.
    // For the rest, we simply throw a warning and reject the tool.
    let mut out_of_spec_tool_names = Vec::<ToolValidationViolation>::new();
    let mut hasher = DefaultHasher::new();
    let mut number_of_tools = 0_usize;

    let number_of_tools_in_mcp_server = result_tools.len();

    let all_tool_names = if !result_tools.is_empty() {
        Some(result_tools.join(","))
    } else {
        None
    };

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
            out_of_spec_tool_names.push(ToolValidationViolation::TooLong(spec.name.clone()));
            continue;
        } else if spec.description.is_empty() {
            out_of_spec_tool_names.push(ToolValidationViolation::EmptyDescription(spec.name.clone()));
            continue;
        }

        if spec.description.len() > 10_004 {
            spec.description.truncate(10_004);
            out_of_spec_tool_names.push(ToolValidationViolation::DescriptionTooLong(spec.name.clone()));
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
    let loaded_tool_names = if specs.is_empty() {
        None
    } else {
        Some(specs.iter().map(|spec| spec.name.clone()).collect::<Vec<_>>().join(","))
    };
    // Send server load success metric datum
    let conversation_id = conversation_id.to_string();
    let _ = telemetry
        .send_mcp_server_init(
            database,
            conversation_id,
            server_name.to_string(),
            None,
            number_of_tools,
            all_tool_names,
            loaded_tool_names,
            number_of_tools_in_mcp_server,
        )
        .await;
    // Tool name translation. This is beyond of the scope of what is
    // considered a "server load". Reasoning being:
    // - Failures here are not related to server load
    // - There is not a whole lot we can do with this data
    if !out_of_spec_tool_names.is_empty() {
        Err(eyre::eyre!(out_of_spec_tool_names.iter().fold(
            String::from(
                "The following tools are out of spec. They may have been excluded from the list of available tools:\n",
            ),
            |mut acc, name| {
                let (tool_name, msg) = match name {
                    ToolValidationViolation::TooLong(tool_name) => (
                        tool_name.as_str(),
                        "tool name exceeds max length of 64 when combined with server name",
                    ),
                    ToolValidationViolation::IllegalChar(tool_name) => (
                        tool_name.as_str(),
                        "tool name must be compliant with ^[a-zA-Z][a-zA-Z0-9_]*$",
                    ),
                    ToolValidationViolation::EmptyDescription(tool_name) => {
                        (tool_name.as_str(), "tool schema contains empty description")
                    },
                    ToolValidationViolation::DescriptionTooLong(tool_name) => (
                        tool_name.as_str(),
                        "tool description is longer than 10024 characters and has been truncated",
                    ),
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
            " - run with Q_LOG_LEVEL=trace and see $TMPDIR/qlog/{CHAT_BINARY_NAME}.log for detail\n"
        )),
        style::ResetColor,
    )?)
}

fn queue_oauth_message(name: &str, output: &mut impl Write) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Yellow),
        style::Print("⚠ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" requires OAuth authentication. Use /mcp to see the auth link\n"),
    )?)
}

fn queue_oauth_message_with_link(name: &str, msg: &eyre::Report, output: &mut impl Write) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::SetForegroundColor(style::Color::Yellow),
        style::Print("⚠ "),
        style::SetForegroundColor(style::Color::Blue),
        style::Print(name),
        style::ResetColor,
        style::Print(" requires OAuth authentication. Follow this link to proceed: \n"),
        style::SetForegroundColor(style::Color::Yellow),
        style::Print(msg),
        style::ResetColor,
        style::Print("\n")
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

fn queue_prompts_load_error_message(name: &str, msg: &eyre::Report, output: &mut impl Write) -> eyre::Result<()> {
    Ok(queue!(
        output,
        style::Print(format!("Prompt list for {name} failed with the following message: \n")),
        style::Print(msg),
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
