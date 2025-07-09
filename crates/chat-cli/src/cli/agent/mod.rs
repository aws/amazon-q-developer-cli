#![allow(dead_code)]
use std::borrow::Borrow;
use std::collections::{
    HashMap,
    HashSet,
};
use std::ffi::OsStr;
use std::io::{
    self,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};

use context_migrate::ContextMigrate;
use crossterm::style::Stylize as _;
use crossterm::{
    queue,
    style,
};
use eyre::bail;
use regex::Regex;
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::fs::ReadDir;
use tracing::{
    error,
    warn,
};

use super::chat::tools::custom_tool::CustomToolConfig;
use super::chat::tools::{
    DEFAULT_APPROVE,
    NATIVE_TOOLS,
    ToolOrigin,
};
use crate::os::Os;
use crate::util::{
    MCP_SERVER_TOOL_DELIMITER,
    directories,
};

mod context_migrate;

// This is to mirror claude's config set up
#[derive(Clone, Serialize, Deserialize, Debug, Default, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase", transparent)]
pub struct McpServerConfig {
    pub mcp_servers: HashMap<String, CustomToolConfig>,
}

impl McpServerConfig {
    pub async fn load_from_file(os: &Os, path: impl AsRef<Path>) -> eyre::Result<Self> {
        let contents = os.fs.read(path.as_ref()).await?;
        let value = serde_json::from_slice::<serde_json::Value>(&contents)?;
        // We need to extract mcp_servers field from the value because we have annotated
        // [McpServerConfig] with transparent. Transparent was added because we want to preserve
        // the type in agent.
        let config = value
            .get("mcpServers")
            .cloned()
            .ok_or(eyre::eyre!("No mcp servers found in config"))?;
        Ok(serde_json::from_value(config)?)
    }

    pub async fn save_to_file(&self, os: &Os, path: impl AsRef<Path>) -> eyre::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        os.fs.write(path.as_ref(), json).await?;
        Ok(())
    }
}

/// An [Agent] is a declarative way of configuring a given instance of q chat. Currently, it is
/// impacting q chat in via influenicng [ContextManager] and [ToolManager].
/// Changes made to [ContextManager] and [ToolManager] do not persist across sessions.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// Agent names are derived from the file name. Thus they are skipped for
    /// serializing
    #[serde(skip)]
    pub name: String,
    /// This field is not model facing and is mostly here for users to discern between agents
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub mcp_servers: McpServerConfig,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub alias: HashMap<String, String>,
    #[serde(default)]
    pub allowed_tools: HashSet<String>,
    #[serde(default)]
    pub included_files: Vec<String>,
    #[serde(default)]
    pub create_hooks: serde_json::Value,
    #[serde(default)]
    pub prompt_hooks: serde_json::Value,
    #[serde(default)]
    pub tools_settings: HashMap<String, serde_json::Value>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            description: Some("Default agent".to_string()),
            prompt: Default::default(),
            mcp_servers: Default::default(),
            tools: NATIVE_TOOLS.iter().copied().map(str::to_string).collect::<Vec<_>>(),
            alias: Default::default(),
            allowed_tools: {
                let mut set = HashSet::<String>::new();
                let default_approve = DEFAULT_APPROVE.iter().copied().map(str::to_string);
                set.extend(default_approve);
                set
            },
            included_files: vec!["AmazonQ.md", "README.md", ".amazonq/rules/**/*.md"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            create_hooks: Default::default(),
            prompt_hooks: Default::default(),
            tools_settings: Default::default(),
            path: None,
        }
    }
}

impl Agent {
    /// Retrieves an agent by name. It does so via first seeking the given agent under local dir,
    /// and falling back to global dir if it does not exist in local.
    pub async fn get_agent_by_name(os: &Os, agent_name: &str) -> eyre::Result<(Agent, PathBuf)> {
        let config_path: Result<PathBuf, PathBuf> = 'config: {
            // local first, and then fall back to looking at global
            let local_config_dir = directories::chat_local_agent_dir()?.join(agent_name);
            if os.fs.exists(&local_config_dir) {
                break 'config Ok::<PathBuf, PathBuf>(local_config_dir);
            }

            let global_config_dir = directories::chat_global_agent_path(os)?.join(format!("{agent_name}.json"));
            if os.fs.exists(&global_config_dir) {
                break 'config Ok(global_config_dir);
            }

            Err(global_config_dir)
        };

        match config_path {
            Ok(config_path) => {
                let content = os.fs.read(&config_path).await?;
                Ok((serde_json::from_slice::<Agent>(&content)?, config_path))
            },
            Err(global_config_dir) if agent_name == "default" => {
                os.fs
                    .create_dir_all(
                        global_config_dir
                            .parent()
                            .ok_or(eyre::eyre!("Failed to retrieve global agent config parent path"))?,
                    )
                    .await?;
                os.fs.create_new(&global_config_dir).await?;

                let default_agent = Agent::default();
                let content = serde_json::to_string_pretty(&default_agent)?;
                os.fs.write(&global_config_dir, content.as_bytes()).await?;

                Ok((default_agent, global_config_dir))
            },
            _ => bail!("Agent {agent_name} does not exist"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum PermissionEvalResult {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Default, Debug)]
pub struct Agents {
    pub agents: HashMap<String, Agent>,
    pub active_idx: String,
    pub trust_all_tools: bool,
}

impl Agents {
    /// This function assumes the relevant transformation to the tool names have been done:
    /// - model tool name -> host tool name
    /// - custom tool namespacing
    pub fn trust_tools(&mut self, tool_names: Vec<String>) {
        if let Some(agent) = self.get_active_mut() {
            agent.allowed_tools.extend(tool_names);
        }
    }

    /// This function assumes the relevant transformation to the tool names have been done:
    /// - model tool name -> host tool name
    /// - custom tool namespacing
    pub fn untrust_tools(&mut self, tool_names: &[String]) {
        if let Some(agent) = self.get_active_mut() {
            agent.allowed_tools.retain(|t| !tool_names.contains(t));
        }
    }

    pub fn get_active(&self) -> Option<&Agent> {
        self.agents.get(&self.active_idx)
    }

    pub fn get_active_mut(&mut self) -> Option<&mut Agent> {
        self.agents.get_mut(&self.active_idx)
    }

    pub fn switch(&mut self, name: &str) -> eyre::Result<&Agent> {
        if !self.agents.contains_key(name) {
            eyre::bail!("No agent with name {name} found");
        }
        self.active_idx = name.to_string();
        self.agents
            .get(name)
            .ok_or(eyre::eyre!("No agent with name {name} found"))
    }

    /// Migrated from [reload_profiles] from context.rs. It loads the active agent from disk and
    /// replaces its in-memory counterpart with it.
    pub async fn reload_agents(&mut self, os: &mut Os, output: &mut impl Write) -> eyre::Result<()> {
        let persona_name = self.get_active().map(|a| a.name.as_str());
        let mut new_self = Self::load(os, persona_name, true, output).await;
        std::mem::swap(self, &mut new_self);
        Ok(())
    }

    pub fn list_agents(&self) -> eyre::Result<Vec<String>> {
        Ok(self.agents.keys().cloned().collect::<Vec<_>>())
    }

    /// Migrated from [create_profile] from context.rs, which was creating profiles under the
    /// global directory. We shall preserve this implicit behavior for now until further notice.
    pub async fn create_agent(&mut self, os: &Os, name: &str) -> eyre::Result<()> {
        validate_agent_name(name)?;

        let agent_path = directories::chat_global_agent_path(os)?.join(format!("{name}.json"));
        if agent_path.exists() {
            return Err(eyre::eyre!("Agent '{}' already exists", name));
        }

        let agent = Agent {
            name: name.to_string(),
            path: Some(agent_path.clone()),
            ..Default::default()
        };
        let contents = serde_json::to_string_pretty(&agent)
            .map_err(|e| eyre::eyre!("Failed to serialize profile configuration: {}", e))?;

        if let Some(parent) = agent_path.parent() {
            os.fs.create_dir_all(parent).await?;
        }
        os.fs.write(&agent_path, contents).await?;

        self.agents.insert(name.to_string(), agent);

        Ok(())
    }

    /// Migrated from [delete_profile] from context.rs, which was deleting profiles under the
    /// global directory. We shall preserve this implicit behavior for now until further notice.
    pub async fn delete_agent(&mut self, os: &Os, name: &str) -> eyre::Result<()> {
        if name == self.active_idx.as_str() {
            eyre::bail!("Cannot delete the active agent. Switch to another agent first");
        }

        let to_delete = self
            .agents
            .get(name)
            .ok_or(eyre::eyre!("Agent '{name}' does not exist"))?;
        match to_delete.path.as_ref() {
            Some(path) if path.exists() => {
                os.fs.remove_file(path).await?;
            },
            _ => eyre::bail!("Agent {name} does not have an associated path"),
        }

        self.agents.remove(name);

        Ok(())
    }

    /// Migrated from [load] from context.rs, which was loading profiles under the
    /// local and global directory. We shall preserve this implicit behavior for now until further
    /// notice.
    /// In addition to loading, this function also calls the function responsible for migrating
    /// existing context into agent.
    pub async fn load(
        os: &mut Os,
        mut agent_name: Option<&str>,
        skip_migration: bool,
        output: &mut impl Write,
    ) -> Self {
        let (chosen_name, new_agents) = if !skip_migration {
            match migrate(os).await {
                Ok((i, new_agents)) => (i, new_agents),
                Err(e) => {
                    warn!("Migration did not happen for the following reason: {e}. This is not necessarily an error");
                    (None, vec![])
                },
            }
        } else {
            (None, vec![])
        };

        if let Some(name) = chosen_name.as_ref() {
            agent_name.replace(name.as_str());
        }

        let mut local_agents = 'local: {
            // We could be launching from the home dir, in which case the global and local agents
            // are the same set of agents. If that is the case, we simply skip this.
            match (std::env::current_dir(), directories::home_dir(os)) {
                (Ok(cwd), Ok(home_dir)) if cwd == home_dir => break 'local Vec::<Agent>::new(),
                _ => {
                    // noop, we keep going with the extraction of local agents (even if we have an
                    // error retrieving cwd or home_dir)
                },
            }

            let Ok(path) = directories::chat_local_agent_dir() else {
                break 'local Vec::<Agent>::new();
            };
            let Ok(files) = os.fs.read_dir(path).await else {
                break 'local Vec::<Agent>::new();
            };
            load_agents_from_entries(files).await
        };

        let mut global_agents = 'global: {
            let Ok(path) = directories::chat_global_agent_path(os) else {
                break 'global Vec::<Agent>::new();
            };
            let files = match os.fs.read_dir(&path).await {
                Ok(files) => files,
                Err(e) => {
                    if matches!(e.kind(), io::ErrorKind::NotFound) {
                        if let Err(e) = os.fs.create_dir_all(&path).await {
                            error!("Error creating global agent dir: {:?}", e);
                        }
                    }
                    break 'global Vec::<Agent>::new();
                },
            };
            load_agents_from_entries(files).await
        }
        .into_iter()
        .chain(new_agents)
        .collect::<Vec<_>>();

        // Here we also want to make sure the example config is written to disk if it's not already
        // there.
        // Note that this config is not what q chat uses. It merely serves as an example.
        'example_config: {
            let Ok(path) = directories::example_agent_config(os) else {
                error!("Error obtaining example agent path.");
                break 'example_config;
            };
            if os.fs.exists(&path) {
                break 'example_config;
            }

            // At this point the agents dir would have been created. All we have to worry about is
            // the creation of the example config
            if let Err(e) = os.fs.create_new(&path).await {
                error!("Error creating example agent config: {e}.");
                break 'example_config;
            }

            let example_agent = Agent {
                // This is less important than other fields since names are derived from the name
                // of the config file and thus will not be persisted
                name: "example".to_string(),
                description: Some("This is an example agent config (and will not be loaded unless you change it to have .json extension)".to_string()),
                tools: {
                    NATIVE_TOOLS
                        .iter()
                        .copied()
                        .map(str::to_string)
                        .chain(vec![
                            format!("@mcp_server_name{MCP_SERVER_TOOL_DELIMITER}mcp_tool_name"),
                            "@mcp_server_name_without_tool_specification_to_include_all_tools".to_string(),
                        ])
                        .collect::<Vec<_>>()
                },
                ..Default::default()
            };
            let Ok(content) = serde_json::to_string_pretty(&example_agent) else {
                error!("Error serializing example agent config");
                break 'example_config;
            };
            if let Err(e) = os.fs.write(&path, &content).await {
                error!("Error writing example agent config to file: {e}");
                break 'example_config;
            };
        }

        let local_names = local_agents.iter().map(|a| a.name.as_str()).collect::<HashSet<&str>>();
        global_agents.retain(|a| {
            // If there is a naming conflict for agents, we would retain the local instance
            let name = a.name.as_str();
            if local_names.contains(name) {
                let _ = queue!(
                    output,
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print("WARNING: "),
                    style::ResetColor,
                    style::Print("Agent conflict for "),
                    style::SetForegroundColor(style::Color::Green),
                    style::Print(name),
                    style::ResetColor,
                    style::Print(". Using workspace version.\n")
                );
                false
            } else {
                true
            }
        });

        local_agents.append(&mut global_agents);

        // If we are told which agent to set as active, we will fall back to a default whose
        // lifetime matches that of the session
        if agent_name.is_none() {
            local_agents.push(Agent::default());
        }

        let _ = output.flush();

        Self {
            agents: local_agents
                .into_iter()
                .map(|a| (a.name.clone(), a))
                .collect::<HashMap<_, _>>(),
            active_idx: agent_name.unwrap_or("default").to_string(),
            ..Default::default()
        }
    }

    /// Returns a label to describe the permission status for a given tool.
    pub fn display_label(&self, tool_name: &str, origin: &ToolOrigin) -> String {
        let tool_trusted = self.get_active().is_some_and(|a| {
            a.allowed_tools.iter().any(|name| {
                // Here the tool names can take the following forms:
                // - @{server_name}{delimiter}{tool_name}
                // - native_tool_name
                name == tool_name
                    || name.strip_prefix("@").is_some_and(|remainder| {
                        remainder
                            .split_once(MCP_SERVER_TOOL_DELIMITER)
                            .is_some_and(|(_left, right)| right == tool_name)
                            || remainder == <ToolOrigin as Borrow<str>>::borrow(origin)
                    })
            })
        });

        if tool_trusted || self.trust_all_tools {
            format!("* {}", "trusted".dark_green().bold())
        } else {
            self.default_permission_label(tool_name)
        }
    }

    /// Provide default permission labels for the built-in set of tools.
    // This "static" way avoids needing to construct a tool instance.
    fn default_permission_label(&self, tool_name: &str) -> String {
        let label = match tool_name {
            "fs_read" => "trusted".dark_green().bold(),
            "fs_write" => "not trusted".dark_grey(),
            #[cfg(not(windows))]
            "execute_bash" => "trust read-only commands".dark_grey(),
            #[cfg(windows)]
            "execute_cmd" => "trust read-only commands".dark_grey(),
            "use_aws" => "trust read-only commands".dark_grey(),
            "report_issue" => "trusted".dark_green().bold(),
            "thinking" => "trusted (prerelease)".dark_green().bold(),
            _ if self.trust_all_tools => "trusted".dark_grey().bold(),
            _ => "not trusted".dark_grey(),
        };

        format!("{} {label}", "*".reset())
    }
}

async fn load_agents_from_entries(mut files: ReadDir) -> Vec<Agent> {
    let mut res = Vec::<Agent>::new();
    while let Ok(Some(file)) = files.next_entry().await {
        let file_path = &file.path();
        if file_path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|s| s == "json")
        {
            let content = match tokio::fs::read(file_path).await {
                Ok(content) => content,
                Err(e) => {
                    let file_path = file_path.to_string_lossy();
                    tracing::error!("Error reading agent file {file_path}: {:?}", e);
                    continue;
                },
            };
            let mut agent = match serde_json::from_slice::<Agent>(&content) {
                Ok(mut agent) => {
                    agent.path = Some(file_path.clone());
                    agent
                },
                Err(e) => {
                    let file_path = file_path.to_string_lossy();
                    tracing::error!("Error deserializing agent file {file_path}: {:?}", e);
                    continue;
                },
            };
            if let Some(name) = Path::new(&file.file_name()).file_stem() {
                agent.name = name.to_string_lossy().to_string();
                res.push(agent);
            } else {
                let file_path = file_path.to_string_lossy();
                tracing::error!("Unable to determine agent name from config file at {file_path}, skipping");
            }
        }
    }
    res
}

fn validate_agent_name(name: &str) -> eyre::Result<()> {
    // Check if name is empty
    if name.is_empty() {
        eyre::bail!("Agent name cannot be empty");
    }

    // Check if name contains only allowed characters and starts with an alphanumeric character
    let re = Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$")?;
    if !re.is_match(name) {
        eyre::bail!(
            "Agent name must start with an alphanumeric character and can only contain alphanumeric characters, hyphens, and underscores"
        );
    }

    Ok(())
}

async fn migrate(os: &mut Os) -> eyre::Result<(Option<String>, Vec<Agent>)> {
    ContextMigrate::<'a'>::scan(os)
        .await?
        .prompt_migrate()
        .await?
        .migrate(os)
        .await?
        .prompt_set_default(os)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NullWriter;

    impl Write for NullWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    const INPUT: &str = r#"
            {
              "description": "My developer agent is used for small development tasks like solving open issues.",
              "prompt": "You are a principal developer who uses multiple agents to accomplish difficult engineering tasks",
              "mcpServers": {
                "fetch": { "command": "fetch3.1", "args": [] },
                "git": { "command": "git-mcp", "args": [] }
              },
              "tools": [                                    
                "@git",                                     
                "fs_read"
              ],
              "alias": {
                  "@gits/some_tool": "some_tool2"
              },
              "allowedTools": [                           
                "fs_read",                               
                "@fetch",
                "@gits/git_status"
              ],
              "includedFiles": [                        
                "~/my-genai-prompts/unittest.md"
              ],
              "createHooks": [                         
                "pwd && tree"
              ],
              "promptHooks": [                        
                "git status"
              ],
              "toolsSettings": {                     
                "fs_write": { "allowedPaths": ["~/**"] },
                "@git/git_status": { "git_user": "$GIT_USER" }
              }
            }
        "#;

    #[test]
    fn test_deser() {
        let agent = serde_json::from_str::<Agent>(INPUT).expect("Deserializtion failed");
        assert!(agent.mcp_servers.mcp_servers.contains_key("fetch"));
        assert!(agent.mcp_servers.mcp_servers.contains_key("git"));
        assert!(agent.alias.contains_key("@gits/some_tool"));
    }

    #[test]
    fn test_get_active() {
        let mut collection = Agents::default();
        assert!(collection.get_active().is_none());

        let agent = Agent::default();
        collection.agents.insert("default".to_string(), agent);
        collection.active_idx = "default".to_string();

        assert!(collection.get_active().is_some());
        assert_eq!(collection.get_active().unwrap().name, "default");
    }

    #[test]
    fn test_get_active_mut() {
        let mut collection = Agents::default();
        assert!(collection.get_active_mut().is_none());

        let agent = Agent::default();
        collection.agents.insert("default".to_string(), agent);
        collection.active_idx = "default".to_string();

        assert!(collection.get_active_mut().is_some());
        let active = collection.get_active_mut().unwrap();
        active.description = Some("Modified description".to_string());

        assert_eq!(
            collection.agents.get("default").unwrap().description,
            Some("Modified description".to_string())
        );
    }

    #[test]
    fn test_switch() {
        let mut collection = Agents::default();

        let default_agent = Agent::default();
        let dev_agent = Agent {
            name: "dev".to_string(),
            description: Some("Developer agent".to_string()),
            ..Default::default()
        };

        collection.agents.insert("default".to_string(), default_agent);
        collection.agents.insert("dev".to_string(), dev_agent);
        collection.active_idx = "default".to_string();

        // Test successful switch
        let result = collection.switch("dev");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "dev");

        // Test switch to non-existent agent
        let result = collection.switch("nonexistent");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "No agent with name nonexistent found");
    }

    #[tokio::test]
    async fn test_list_agents() {
        let mut collection = Agents::default();

        // Add two agents
        let default_agent = Agent::default();
        let dev_agent = Agent {
            name: "dev".to_string(),
            description: Some("Developer agent".to_string()),
            ..Default::default()
        };

        collection.agents.insert("default".to_string(), default_agent);
        collection.agents.insert("dev".to_string(), dev_agent);

        let result = collection.list_agents();
        assert!(result.is_ok());

        let agents = result.unwrap();
        assert_eq!(agents.len(), 2);
        assert!(agents.contains(&"default".to_string()));
        assert!(agents.contains(&"dev".to_string()));
    }

    #[tokio::test]
    async fn test_create_agent() {
        let mut collection = Agents::default();
        let ctx = Os::new().await.unwrap();

        let agent_name = "test_agent";
        let result = collection.create_agent(&ctx, agent_name).await;
        assert!(result.is_ok());
        let agent_path = directories::chat_global_agent_path(&ctx)
            .expect("Error obtaining global agent path")
            .join(format!("{agent_name}.json"));
        assert!(agent_path.exists());
        assert!(collection.agents.contains_key(agent_name));

        // Test with creating a agent with the same name
        let result = collection.create_agent(&ctx, agent_name).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            format!("Agent '{agent_name}' already exists")
        );

        // Test invalid agent names
        let result = collection.create_agent(&ctx, "").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Agent name cannot be empty");

        let result = collection.create_agent(&ctx, "123-invalid!").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_agent() {
        let mut collection = Agents::default();
        let ctx = Os::new().await.unwrap();

        let agent_name_one = "test_agent_one";
        collection
            .create_agent(&ctx, agent_name_one)
            .await
            .expect("Failed to create agent");
        let agent_name_two = "test_agent_two";
        collection
            .create_agent(&ctx, agent_name_two)
            .await
            .expect("Failed to create agent");

        collection.switch(agent_name_one).expect("Failed to switch agent");

        // Should not be able to delete active agent
        let active = collection
            .get_active()
            .expect("Failed to obtain active agent")
            .name
            .clone();
        let result = collection.delete_agent(&ctx, &active).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Cannot delete the active agent. Switch to another agent first"
        );

        // Should be able to delete inactive agent
        let agent_two_path = collection
            .agents
            .get(agent_name_two)
            .expect("Failed to obtain agent that's yet to be deleted")
            .path
            .clone()
            .expect("agent should have path");
        let result = collection.delete_agent(&ctx, agent_name_two).await;
        assert!(result.is_ok());
        assert!(!collection.agents.contains_key(agent_name_two));
        assert!(!agent_two_path.exists());

        let result = collection.delete_agent(&ctx, "nonexistent").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Agent 'nonexistent' does not exist");
    }

    #[test]
    fn test_validate_agent_name() {
        // Valid names
        assert!(validate_agent_name("valid").is_ok());
        assert!(validate_agent_name("valid123").is_ok());
        assert!(validate_agent_name("valid-name").is_ok());
        assert!(validate_agent_name("valid_name").is_ok());
        assert!(validate_agent_name("123valid").is_ok());

        // Invalid names
        assert!(validate_agent_name("").is_err());
        assert!(validate_agent_name("-invalid").is_err());
        assert!(validate_agent_name("_invalid").is_err());
        assert!(validate_agent_name("invalid!").is_err());
        assert!(validate_agent_name("invalid space").is_err());
    }

    #[test]
    fn test_schema_gen() {
        use schemars::schema_for;
        let schema = schema_for!(Agent);
        println!("Schema for agent: {}", serde_json::to_string_pretty(&schema).unwrap());
    }
}
