use std::collections::HashMap;
use std::hash::{
    DefaultHasher,
    Hasher,
};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;

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
use mcp_client::PromptGet;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::Mutex;
use tracing::error;

use super::parser::ToolUse;
use super::prompt::PromptGetInfo;
use super::tools::Tool;
use super::tools::custom_tool::{
    CustomToolClient,
    CustomToolConfig,
};
use super::tools::execute_bash::ExecuteBash;
use super::tools::fs_read::FsRead;
use super::tools::fs_write::FsWrite;
use super::tools::gh_issue::GhIssue;
use super::tools::use_aws::UseAws;
use crate::cli::chat::tools::ToolSpec;
use crate::cli::chat::tools::custom_tool::CustomTool;

const NAMESPACE_DELIMITER: &str = "___";
// This applies for both mcp server and tool name since in the end the tool name as seen by the
// model is just {server_name}{NAMESPACE_DELIMITER}{tool_name}
const VALID_TOOL_NAME: &str = "^[a-zA-Z][a-zA-Z0-9_]*$";
const SPINNER_CHARS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

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
                let mut global_conf = serde_json::from_slice::<Self>(&global_buf)?;
                let local_conf = serde_json::from_slice::<Self>(&local_buf)?;
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
            (None, Some(local_buf)) => serde_json::from_slice::<Self>(&local_buf)?,
            (Some(global_buf), None) => serde_json::from_slice::<Self>(&global_buf)?,
            _ => Default::default(),
        };
        output.flush()?;
        Ok(conf)
    }
}

#[derive(Default)]
pub struct ToolManagerBuilder {
    mcp_server_config: Option<McpServerConfig>,
    prompt_list_sender: Option<std::sync::mpsc::Sender<Vec<PromptGetInfo>>>,
    prompt_list_receiver: Option<std::sync::mpsc::Receiver<()>>,
}

impl ToolManagerBuilder {
    pub fn msp_server_config(mut self, config: McpServerConfig) -> Self {
        self.mcp_server_config.replace(config);
        self
    }

    pub fn prompt_list_sender(mut self, sender: std::sync::mpsc::Sender<Vec<PromptGetInfo>>) -> Self {
        self.prompt_list_sender.replace(sender);
        self
    }

    pub fn prompt_list_receiver(mut self, receiver: std::sync::mpsc::Receiver<()>) -> Self {
        self.prompt_list_receiver.replace(receiver);
        self
    }

    pub fn build(mut self) -> eyre::Result<ToolManager> {
        let McpServerConfig { mcp_servers } = self.mcp_server_config.ok_or(eyre::eyre!("Missing mcp server config"))?;
        let regex = regex::Regex::new(VALID_TOOL_NAME)?;
        let mut hasher = DefaultHasher::new();
        let pre_initialized = mcp_servers
            .into_iter()
            .map(|(server_name, server_config)| {
                let snaked_cased_name = server_name.to_case(convert_case::Case::Snake);
                let sanitized_server_name = sanitize_server_name(snaked_cased_name, &regex, &mut hasher);
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
                            let fail_load_msg = msg.to_string();
                            let fail_load_msg = eyre::eyre!(fail_load_msg);
                            queue_failure_message(&name, &fail_load_msg, &mut stdout_lock)?;
                            let total = loading_servers.len();
                            queue_init_message(spinner_logo_idx, complete, failed, total, &mut stdout_lock)?;
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
        // TODO: accommodate hot reload of mcp servers
        if let (Some(sender), Some(receiver)) = (sender, receiver) {
            let clients_arc = Arc::new(clients.clone());
            tokio::task::spawn_blocking(move || {
                let receiver = Arc::new(std::sync::Mutex::new(receiver));
                loop {
                    receiver.lock().map_err(|e| eyre::eyre!("{:?}", e))?.recv()?;
                    let sender_clone = sender.clone();
                    let clients = clients_arc.clone();
                    let prompt_gets = clients
                        .iter()
                        .map(|(n, c)| (n.clone(), c.list_prompt_gets()))
                        .collect::<Vec<_>>();
                    if let Err(e) = sender_clone.send(prompt_gets) {
                        error!("Error sending prompts to chat helper: {:?}", e);
                    }
                }
                #[allow(unreachable_code)]
                Ok::<(), eyre::Report>(())
            });
        }

        Ok(ToolManager {
            clients,
            loading_display_task,
            loading_status_sender,
        })
    }
}

#[derive(Default)]
pub struct ToolManager {
    pub clients: HashMap<String, Arc<CustomToolClient>>,
    loading_display_task: Option<std::thread::JoinHandle<Result<(), eyre::Report>>>,
    loading_status_sender: Option<std::sync::mpsc::Sender<LoadingMsg>>,
}

impl ToolManager {
    pub async fn load_tools(&mut self) -> eyre::Result<HashMap<String, ToolSpec>> {
        let tx = self.loading_status_sender.take();
        let display_task = self.loading_display_task.take();
        let tool_specs = {
            let tool_specs = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("tools/tool_index.json"))?;
            Arc::new(Mutex::new(tool_specs))
        };
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
                async move {
                    let tool_spec = client_clone.init().await;
                    match tool_spec {
                        Ok((name, specs)) => {
                            // Each mcp server might have multiple tools.
                            // To avoid naming conflicts we are going to namespace it.
                            // This would also help us locate which mcp server to call the tool from.
                            let mut out_of_spec_tool_names = Vec::<String>::new();
                            for mut spec in specs {
                                if !regex_clone.is_match(&spec.name) {
                                    out_of_spec_tool_names.push(spec.name.clone());
                                    continue;
                                }
                                spec.name = format!("{}{}{}", name, NAMESPACE_DELIMITER, spec.name);
                                if spec.name.len() > 64 {
                                    out_of_spec_tool_names.push(spec.name.clone());
                                    continue;
                                }
                                tool_specs_clone.lock().await.insert(spec.name.clone(), spec);
                            }
                            if let Some(tx_clone) = &tx_clone {
                                let send_result = if !out_of_spec_tool_names.is_empty() {
                                    let msg = out_of_spec_tool_names.iter().fold(
                                        String::from("The following tool names are out of spec. They will be excluded from the list of available tools:\n"),
                                        |mut acc, name| {
                                            let msg = if name.len() > 64 {
                                                "exceeded max length of 64"
                                            } else {
                                                "must be complied with ^[a-zA-Z][a-zA-Z0-9_]*$"
                                            };
                                            acc.push_str(format!("  - {} ({})\n", name, msg).as_str());
                                            acc
                                        }
                                    );
                                    tx_clone.send(LoadingMsg::Error {
                                        name: name.clone(),
                                        msg: eyre::eyre!(msg),
                                    })
                                    // TODO: if no tools are valid, we need to offload the server
                                    // from the fleet (i.e. kill the server)
                                } else {
                                    tx_clone.send(LoadingMsg::Done(name.clone()))
                                };
                                if let Err(e) = send_result {
                                    error!("Error while sending status update to display task: {:?}", e);
                                }
                            }
                        },
                        Err(e) => {
                            error!("Error obtaining tool spec for {}: {:?}", server_name_clone, e);
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
                    Ok::<(), eyre::Report>(())
                }
            })
            .collect::<Vec<_>>();
        // TODO: do we want to introduce a timeout here?
        stream::iter(load_tool)
            .map(|async_closure| tokio::task::spawn(async_closure))
            .buffer_unordered(20)
            .collect::<Vec<_>>()
            .await;
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
        Ok(tool_specs)
    }

    pub fn get_tool_from_tool_use(&self, value: ToolUse) -> Result<Tool, ToolResult> {
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
            // Note that this name is namespaced with server_name{DELIMITER}tool_name
            name => {
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

    #[allow(clippy::type_complexity)]
    pub fn get_prompt_gets(&self) -> Vec<(&String, Arc<std::sync::RwLock<HashMap<String, PromptGet>>>)> {
        self.clients
            .iter()
            .map(|(k, v)| {
                let prompts = v.list_prompt_gets();
                (k, prompts)
            })
            .collect()
    }
}

fn sanitize_server_name(orig: String, regex: &regex::Regex, hasher: &mut impl Hasher) -> String {
    if regex.is_match(&orig) {
        return orig;
    }
    let sanitized: String = orig
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || *c == '_')
        .collect();
    if sanitized.is_empty() {
        hasher.write(orig.as_bytes());
        let hash = hasher.finish().to_string();
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
        style::Print(" has failed to load:\n"),
        style::Print(fail_load_msg),
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
        let sanitized_server_name = sanitize_server_name(orig_name.to_string(), &regex, &mut hasher);
        assert_eq!(sanitized_server_name, "awslabscdkmcpserver");

        let orig_name = "good_name";
        let sanitized_good_name = sanitize_server_name(orig_name.to_string(), &regex, &mut hasher);
        assert_eq!(sanitized_good_name, orig_name.to_string());

        let all_bad_name = "@@@@@";
        let sanitized_all_bad_name = sanitize_server_name(all_bad_name.to_string(), &regex, &mut hasher);
        assert!(regex.is_match(&sanitized_all_bad_name));
    }
}
