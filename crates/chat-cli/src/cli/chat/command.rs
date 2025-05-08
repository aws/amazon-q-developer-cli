use std::collections::HashSet;

use clap::{
    Parser,
    Subcommand,
};
use eyre::{
    Result,
    anyhow,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::commands::clear::CLEAR_HANDLER;
use crate::cli::chat::commands::compact::COMPACT_HANDLER;
use crate::cli::chat::commands::context::CONTEXT_HANDLER;
use crate::cli::chat::commands::editor::EDITOR_HANDLER;
use crate::cli::chat::commands::execute::EXECUTE_HANDLER;
// Import static handlers
use crate::cli::chat::commands::help::HELP_HANDLER;
use crate::cli::chat::commands::issue::ISSUE_HANDLER;
use crate::cli::chat::commands::profile::PROFILE_HANDLER;
use crate::cli::chat::commands::quit::QUIT_HANDLER;
use crate::cli::chat::commands::tools::TOOLS_HANDLER;
use crate::cli::chat::commands::usage::USAGE_HANDLER;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Ask {
        prompt: String,
    },
    Execute {
        command: String,
    },
    Clear,
    Help {
        help_text: Option<String>,
    },
    Issue {
        prompt: Option<String>,
    },
    Quit,
    Profile {
        subcommand: ProfileSubcommand,
    },
    Context {
        subcommand: ContextSubcommand,
    },
    PromptEditor {
        initial_text: Option<String>,
    },
    Compact {
        prompt: Option<String>,
        show_summary: bool,
        help: bool,
    },
    Tools {
        subcommand: Option<ToolsSubcommand>,
    },
    Prompts {
        subcommand: Option<PromptsSubcommand>,
    },
    Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileSubcommand {
    List,
    Create { name: String },
    Delete { name: String },
    Set { name: String },
    Rename { old_name: String, new_name: String },
    Help,
}

impl ProfileSubcommand {
    const AVAILABLE_COMMANDS: &str = color_print::cstr! {"<cyan!>Available commands</cyan!>
  <em>help</em>                <black!>Show an explanation for the profile command</black!>
  <em>list</em>                <black!>List all available profiles</black!>
  <em>create <<name>></em>       <black!>Create a new profile with the specified name</black!>
  <em>delete <<name>></em>       <black!>Delete the specified profile</black!>
  <em>set <<name>></em>          <black!>Switch to the specified profile</black!>
  <em>rename <<old>> <<new>></em>  <black!>Rename a profile</black!>"};
    const CREATE_USAGE: &str = "/profile create <profile_name>";
    const DELETE_USAGE: &str = "/profile delete <profile_name>";
    const RENAME_USAGE: &str = "/profile rename <old_profile_name> <new_profile_name>";
    const SET_USAGE: &str = "/profile set <profile_name>";

    fn usage_msg(header: impl AsRef<str>) -> String {
        format!("{}\n\n{}", header.as_ref(), Self::AVAILABLE_COMMANDS)
    }

    pub fn to_handler(&self) -> &'static dyn CommandHandler {
        use crate::cli::chat::commands::profile::{
            CREATE_PROFILE_HANDLER,
            DELETE_PROFILE_HANDLER,
            HELP_PROFILE_HANDLER,
            LIST_PROFILE_HANDLER,
            RENAME_PROFILE_HANDLER,
            SET_PROFILE_HANDLER,
        };

        match self {
            ProfileSubcommand::Create { .. } => &CREATE_PROFILE_HANDLER,
            ProfileSubcommand::Delete { .. } => &DELETE_PROFILE_HANDLER,
            ProfileSubcommand::List => &LIST_PROFILE_HANDLER,
            ProfileSubcommand::Set { .. } => &SET_PROFILE_HANDLER,
            ProfileSubcommand::Rename { .. } => &RENAME_PROFILE_HANDLER,
            ProfileSubcommand::Help => &HELP_PROFILE_HANDLER,
        }
    }

    pub fn help_text() -> String {
        color_print::cformat!(
            r#"
<magenta,em>(Beta) Profile Management</magenta,em>

Profiles allow you to organize and manage different sets of context files for different projects or tasks.

{}

<cyan!>Notes</cyan!>
• The "global" profile contains context files that are available in all profiles
• The "default" profile is used when no profile is specified
• You can switch between profiles to work on different projects
• Each profile maintains its own set of context files
"#,
            Self::AVAILABLE_COMMANDS
        )
    }
}

#[derive(Parser, Debug, Clone)]
#[command(name = "hooks", disable_help_flag = true, disable_help_subcommand = true)]
struct HooksCommand {
    #[command(subcommand)]
    command: HooksSubcommand,
}

#[derive(Subcommand, Debug, Clone, Eq, PartialEq)]
pub enum HooksSubcommand {
    Add {
        name: String,

        #[arg(long, value_parser = ["per_prompt", "conversation_start"])]
        trigger: String,

        #[arg(long, value_parser = clap::value_parser!(String))]
        command: String,

        #[arg(long)]
        global: bool,
    },
    #[command(name = "rm")]
    Remove {
        name: String,

        #[arg(long)]
        global: bool,
    },
    Enable {
        name: String,

        #[arg(long)]
        global: bool,
    },
    Disable {
        name: String,

        #[arg(long)]
        global: bool,
    },
    EnableAll {
        #[arg(long)]
        global: bool,
    },
    DisableAll {
        #[arg(long)]
        global: bool,
    },
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextSubcommand {
    Show {
        expand: bool,
    },
    Add {
        global: bool,
        force: bool,
        paths: Vec<String>,
    },
    Remove {
        global: bool,
        paths: Vec<String>,
    },
    Clear {
        global: bool,
    },
    Hooks {
        subcommand: Option<HooksSubcommand>,
    },
    Help,
}

impl ContextSubcommand {
    const ADD_USAGE: &str = "/context add [--global] [--force] <path1> [path2...]";
    const AVAILABLE_COMMANDS: &str = color_print::cstr! {"<cyan!>Available commands</cyan!>
  <em>help</em>                           <black!>Show an explanation for the context command</black!>

  <em>show [--expand]</em>                <black!>Display the context rule configuration and matched files</black!>
                                          <black!>--expand: Print out each matched file's content</black!>

  <em>add [--global] [--force] <<paths...>></em>
                                 <black!>Add context rules (filenames or glob patterns)</black!>
                                 <black!>--global: Add to global rules (available in all profiles)</black!>
                                 <black!>--force: Include even if matched files exceed size limits</black!>

  <em>rm [--global] <<paths...>></em>       <black!>Remove specified rules from current profile</black!>
                                 <black!>--global: Remove specified rules globally</black!>

  <em>clear [--global]</em>               <black!>Remove all rules from current profile</black!>
                                 <black!>--global: Remove global rules</black!>

  <em>hooks</em>                          <black!>View and manage context hooks</black!>"};
    const CLEAR_USAGE: &str = "/context clear [--global]";
    const HOOKS_AVAILABLE_COMMANDS: &str = color_print::cstr! {"<cyan!>Available subcommands</cyan!>
  <em>hooks help</em>                         <black!>Show an explanation for context hooks commands</black!>

  <em>hooks add [--global] <<name>></em>        <black!>Add a new command context hook</black!>
                                         <black!>--global: Add to global hooks</black!>
         <em>--trigger <<trigger>></em>           <black!>When to trigger the hook, valid options: `per_prompt` or `conversation_start`</black!>
         <em>--command <<command>></em>             <black!>Shell command to execute</black!>

  <em>hooks rm [--global] <<name>></em>         <black!>Remove an existing context hook</black!>
                                         <black!>--global: Remove from global hooks</black!>

  <em>hooks enable [--global] <<name>></em>     <black!>Enable an existing context hook</black!>
                                         <black!>--global: Enable in global hooks</black!>

  <em>hooks disable [--global] <<name>></em>    <black!>Disable an existing context hook</black!>
                                         <black!>--global: Disable in global hooks</black!>

  <em>hooks enable-all [--global]</em>        <black!>Enable all existing context hooks</black!>
                                         <black!>--global: Enable all in global hooks</black!>

  <em>hooks disable-all [--global]</em>       <black!>Disable all existing context hooks</black!>
                                         <black!>--global: Disable all in global hooks</black!>"};
    const REMOVE_USAGE: &str = "/context rm [--global] <path1> [path2...]";
    const SHOW_USAGE: &str = "/context show [--expand]";

    pub fn to_handler(&self) -> &'static dyn CommandHandler {
        use crate::cli::chat::commands::context::{
            CONTEXT_HANDLER,
            add,
            clear,
            remove,
            show,
        };

        match self {
            ContextSubcommand::Add { .. } => &add::ADD_CONTEXT_HANDLER,
            ContextSubcommand::Remove { .. } => &remove::REMOVE_CONTEXT_HANDLER,
            ContextSubcommand::Clear { .. } => &clear::CLEAR_CONTEXT_HANDLER,
            ContextSubcommand::Show { .. } => &show::SHOW_CONTEXT_HANDLER,
            ContextSubcommand::Hooks { .. } => &CONTEXT_HANDLER, // Delegate to main context handler
            ContextSubcommand::Help => &CONTEXT_HANDLER,         // Delegate to main context handler
        }
    }

    fn usage_msg(header: impl AsRef<str>) -> String {
        format!("{}\n\n{}", header.as_ref(), Self::AVAILABLE_COMMANDS)
    }

    fn hooks_usage_msg(header: impl AsRef<str>) -> String {
        format!("{}\n\n{}", header.as_ref(), Self::HOOKS_AVAILABLE_COMMANDS)
    }

    pub fn help_text() -> String {
        color_print::cformat!(
            r#"
<magenta,em>(Beta) Context Rule Management</magenta,em>

Context rules determine which files are included in your Amazon Q session. 
The files matched by these rules provide Amazon Q with additional information 
about your project or environment. Adding relevant files helps Q generate 
more accurate and helpful responses.

In addition to files, you can specify hooks that will run commands and return 
the output as context to Amazon Q.

{}

<cyan!>Notes</cyan!>
• You can add specific files or use glob patterns (e.g., "*.py", "src/**/*.js")
• Profile rules apply only to the current profile
• Global rules apply across all profiles
• Context is preserved between chat sessions
"#,
            Self::AVAILABLE_COMMANDS
        )
    }

    pub fn hooks_help_text() -> String {
        color_print::cformat!(
            r#"
<magenta,em>(Beta) Context Hooks</magenta,em>

Use context hooks to specify shell commands to run. The output from these 
commands will be appended to the prompt to Amazon Q. Hooks can be defined 
in global or local profiles.

<cyan!>Usage: /context hooks [SUBCOMMAND]</cyan!>

<cyan!>Description</cyan!>
  Show existing global or profile-specific hooks.
  Alternatively, specify a subcommand to modify the hooks.

{}

<cyan!>Notes</cyan!>
• Hooks are executed in parallel
• 'conversation_start' hooks run on the first user prompt and are attached once to the conversation history sent to Amazon Q
• 'per_prompt' hooks run on each user prompt and are attached to the prompt, but are not stored in conversation history
"#,
            Self::HOOKS_AVAILABLE_COMMANDS
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolsSubcommand {
    Schema,
    Trust { tool_names: HashSet<String> },
    Untrust { tool_names: HashSet<String> },
    TrustAll { from_deprecated: bool },
    Reset,
    ResetSingle { tool_name: String },
    Help,
}

impl ToolsSubcommand {
    const AVAILABLE_COMMANDS: &str = color_print::cstr! {"<cyan!>Available subcommands</cyan!>
  <em>help</em>                           <black!>Show an explanation for the tools command</black!>
  <em>schema</em>                         <black!>Show the input schema for all available tools</black!>
  <em>trust <<tools...>></em>               <black!>Trust a specific tool or tools for the session</black!>
  <em>untrust <<tools...>></em>             <black!>Revert a tool or tools to per-request confirmation</black!>
  <em>trustall</em>                       <black!>Trust all tools (equivalent to deprecated /acceptall)</black!>
  <em>reset</em>                          <black!>Reset all tools to default permission levels</black!>
  <em>reset <<tool name>></em>              <black!>Reset a single tool to default permission level</black!>"};
    const BASE_COMMAND: &str = color_print::cstr! {"<cyan!>Usage: /tools [SUBCOMMAND]</cyan!>

<cyan!>Description</cyan!>
  Show the current set of tools and their permission setting.
  The permission setting states when user confirmation is required. Trusted tools never require confirmation.
  Alternatively, specify a subcommand to modify the tool permissions."};

    fn usage_msg(header: impl AsRef<str>) -> String {
        format!(
            "{}\n\n{}\n\n{}",
            header.as_ref(),
            Self::BASE_COMMAND,
            Self::AVAILABLE_COMMANDS
        )
    }

    pub fn help_text() -> String {
        color_print::cformat!(
            r#"
<magenta,em>Tool Permissions</magenta,em>

By default, Amazon Q will ask for your permission to use certain tools. You can control which tools you
trust so that no confirmation is required. These settings will last only for this session.

{}

{}"#,
            Self::BASE_COMMAND,
            Self::AVAILABLE_COMMANDS
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptsSubcommand {
    List { search_word: Option<String> },
    Get { get_command: PromptsGetCommand },
    Help,
}

impl PromptsSubcommand {
    const AVAILABLE_COMMANDS: &str = color_print::cstr! {"<cyan!>Available subcommands</cyan!>
  <em>help</em>                                                   <black!>Show an explanation for the prompts command</black!>
  <em>list [search word]</em>                                     <black!>List available prompts from a tool or show all available prompts</black!>"};
    const BASE_COMMAND: &str = color_print::cstr! {"<cyan!>Usage: /prompts [SUBCOMMAND]</cyan!>

<cyan!>Description</cyan!>
  Show the current set of reusuable prompts from the current fleet of mcp servers."};

    fn usage_msg(header: impl AsRef<str>) -> String {
        format!(
            "{}\n\n{}\n\n{}",
            header.as_ref(),
            Self::BASE_COMMAND,
            Self::AVAILABLE_COMMANDS
        )
    }

    pub fn help_text() -> String {
        color_print::cformat!(
            r#"
<magenta,em>Prompts</magenta,em>

Prompts are reusable templates that help you quickly access common workflows and tasks. 
These templates are provided by the mcp servers you have installed and configured.

To actually retrieve a prompt, directly start with the following command (without prepending /prompt get):
  <em>@<<prompt name>> [arg]</em>                                   <black!>Retrieve prompt specified</black!>
Or if you prefer the long way:
  <em>/prompts get <<prompt name>> [arg]</em>                       <black!>Retrieve prompt specified</black!>

{}

{}"#,
            Self::BASE_COMMAND,
            Self::AVAILABLE_COMMANDS
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptsGetCommand {
    pub orig_input: Option<String>,
    pub params: PromptsGetParam,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptsGetParam {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<String>>,
}

impl Command {
    /// Parse a command string into a Command enum
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();

        // Check if the input starts with a literal backslash followed by a slash
        // This allows users to escape the slash if they actually want to start with one
        if input.starts_with("\\/") {
            return Ok(Self::Ask {
                prompt: input[1..].to_string(), // Remove the backslash but keep the slash
            });
        }

        if let Some(command) = input.strip_prefix("/") {
            let parts: Vec<&str> = command.split_whitespace().collect();

            if parts.is_empty() {
                return Err(anyhow!("Empty command"));
            }

            return Ok(match parts[0].to_lowercase().as_str() {
                "clear" => Self::Clear,
                "help" => Self::Help { help_text: None },
                "compact" => {
                    let mut prompt = None;
                    let show_summary = true;
                    let mut help = false;

                    // Check if "help" is the first subcommand
                    if parts.len() > 1 && parts[1].to_lowercase() == "help" {
                        help = true;
                    } else {
                        let mut remaining_parts = Vec::new();

                        remaining_parts.extend_from_slice(&parts[1..]);

                        // If we have remaining parts after parsing flags, join them as the prompt
                        if !remaining_parts.is_empty() {
                            prompt = Some(remaining_parts.join(" "));
                        }
                    }

                    Self::Compact {
                        prompt,
                        show_summary,
                        help,
                    }
                },
                "acceptall" => {
                    // Deprecated command - set flag to show deprecation message
                    Self::Tools {
                        subcommand: Some(ToolsSubcommand::TrustAll { from_deprecated: true }),
                    }
                },
                "editor" => {
                    if parts.len() > 1 {
                        Self::PromptEditor {
                            initial_text: Some(parts[1..].join(" ")),
                        }
                    } else {
                        Self::PromptEditor { initial_text: None }
                    }
                },
                "issue" => {
                    if parts.len() > 1 {
                        Self::Issue {
                            prompt: Some(parts[1..].join(" ")),
                        }
                    } else {
                        Self::Issue { prompt: None }
                    }
                },
                "q" | "exit" | "quit" => Self::Quit,
                "profile" => {
                    if parts.len() < 2 {
                        return Ok(Self::Profile {
                            subcommand: ProfileSubcommand::Help,
                        });
                    }

                    macro_rules! usage_err {
                        ($usage_str:expr) => {
                            return Err(anyhow!(format!(
                                "Invalid /profile arguments.\n\nUsage:\n  {}",
                                $usage_str
                            )))
                        };
                    }

                    match parts[1].to_lowercase().as_str() {
                        "list" => Self::Profile {
                            subcommand: ProfileSubcommand::List,
                        },
                        "create" => {
                            let name = parts.get(2);
                            match name {
                                Some(name) => Self::Profile {
                                    subcommand: ProfileSubcommand::Create {
                                        name: (*name).to_string(),
                                    },
                                },
                                None => usage_err!(ProfileSubcommand::CREATE_USAGE),
                            }
                        },
                        "delete" => {
                            let name = parts.get(2);
                            match name {
                                Some(name) => Self::Profile {
                                    subcommand: ProfileSubcommand::Delete {
                                        name: (*name).to_string(),
                                    },
                                },
                                None => usage_err!(ProfileSubcommand::DELETE_USAGE),
                            }
                        },
                        "rename" => {
                            let old_name = parts.get(2);
                            let new_name = parts.get(3);
                            match (old_name, new_name) {
                                (Some(old), Some(new)) => Self::Profile {
                                    subcommand: ProfileSubcommand::Rename {
                                        old_name: (*old).to_string(),
                                        new_name: (*new).to_string(),
                                    },
                                },
                                _ => usage_err!(ProfileSubcommand::RENAME_USAGE),
                            }
                        },
                        "set" => {
                            let name = parts.get(2);
                            match name {
                                Some(name) => Self::Profile {
                                    subcommand: ProfileSubcommand::Set {
                                        name: (*name).to_string(),
                                    },
                                },
                                None => usage_err!(ProfileSubcommand::SET_USAGE),
                            }
                        },
                        "help" => Self::Profile {
                            subcommand: ProfileSubcommand::Help,
                        },
                        other => {
                            return Err(anyhow!(ProfileSubcommand::usage_msg(format!(
                                "Unknown subcommand '{}'.",
                                other
                            ))));
                        },
                    }
                },
                "context" => {
                    if parts.len() < 2 {
                        return Ok(Self::Context {
                            subcommand: ContextSubcommand::Help,
                        });
                    }

                    macro_rules! usage_err {
                        ($usage_str:expr) => {
                            return Err(anyhow!(format!(
                                "Invalid /context arguments.\n\nUsage:\n  {}",
                                $usage_str
                            )));
                        };
                    }

                    match parts[1].to_lowercase().as_str() {
                        "show" => {
                            let mut expand = false;
                            for part in &parts[2..] {
                                if *part == "--expand" {
                                    expand = true;
                                } else {
                                    usage_err!(ContextSubcommand::SHOW_USAGE);
                                }
                            }
                            Self::Context {
                                subcommand: ContextSubcommand::Show { expand },
                            }
                        },
                        "add" => {
                            // Parse add command with paths and flags
                            let mut global = false;
                            let mut force = false;
                            let mut paths = Vec::new();

                            let args = match shlex::split(&parts[2..].join(" ")) {
                                Some(args) => args,
                                None => return Err(anyhow!("Failed to parse quoted arguments")),
                            };

                            for arg in &args {
                                if arg == "--global" {
                                    global = true;
                                } else if arg == "--force" || arg == "-f" {
                                    force = true;
                                } else {
                                    paths.push(arg.to_string());
                                }
                            }

                            if paths.is_empty() {
                                usage_err!(ContextSubcommand::ADD_USAGE);
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Add { global, force, paths },
                            }
                        },
                        "rm" => {
                            // Parse rm command with paths and --global flag
                            let mut global = false;
                            let mut paths = Vec::new();
                            let args = match shlex::split(&parts[2..].join(" ")) {
                                Some(args) => args,
                                None => return Err(anyhow!("Failed to parse quoted arguments")),
                            };

                            for arg in &args {
                                if arg == "--global" {
                                    global = true;
                                } else {
                                    paths.push(arg.to_string());
                                }
                            }

                            if paths.is_empty() {
                                usage_err!(ContextSubcommand::REMOVE_USAGE);
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Remove { global, paths },
                            }
                        },
                        "clear" => {
                            // Parse clear command with optional --global flag
                            let mut global = false;

                            for part in &parts[2..] {
                                if *part == "--global" {
                                    global = true;
                                } else {
                                    usage_err!(ContextSubcommand::CLEAR_USAGE);
                                }
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Clear { global },
                            }
                        },
                        "help" => Self::Context {
                            subcommand: ContextSubcommand::Help,
                        },
                        "hooks" => {
                            if parts.get(2).is_none() {
                                return Ok(Self::Context {
                                    subcommand: ContextSubcommand::Hooks { subcommand: None },
                                });
                            };

                            match Self::parse_hooks(&parts) {
                                Ok(command) => command,
                                Err(err) => return Err(anyhow!(ContextSubcommand::hooks_usage_msg(err))),
                            }
                        },
                        other => {
                            return Err(anyhow!(ContextSubcommand::usage_msg(format!(
                                "Unknown subcommand '{}'.",
                                other
                            ))));
                        },
                    }
                },
                "tools" => {
                    if parts.len() < 2 {
                        return Ok(Self::Tools { subcommand: None });
                    }

                    match parts[1].to_lowercase().as_str() {
                        "list" => Self::Tools { subcommand: None },
                        "schema" => Self::Tools {
                            subcommand: Some(ToolsSubcommand::Schema),
                        },
                        "trust" => {
                            let mut tool_names = HashSet::new();
                            for part in &parts[2..] {
                                tool_names.insert((*part).to_string());
                            }

                            // Usage hints should be handled elsewhere
                            Self::Tools {
                                subcommand: Some(ToolsSubcommand::Trust { tool_names }),
                            }
                        },
                        "untrust" => {
                            let mut tool_names = HashSet::new();
                            for part in &parts[2..] {
                                tool_names.insert((*part).to_string());
                            }

                            // Usage hints should be handled elsewhere
                            Self::Tools {
                                subcommand: Some(ToolsSubcommand::Untrust { tool_names }),
                            }
                        },
                        "trustall" => Self::Tools {
                            subcommand: Some(ToolsSubcommand::TrustAll { from_deprecated: false }),
                        },
                        "reset" => {
                            let tool_name = parts.get(2);
                            match tool_name {
                                Some(tool_name) => Self::Tools {
                                    subcommand: Some(ToolsSubcommand::ResetSingle {
                                        tool_name: (*tool_name).to_string(),
                                    }),
                                },
                                None => Self::Tools {
                                    subcommand: Some(ToolsSubcommand::Reset),
                                },
                            }
                        },
                        "help" => Self::Tools {
                            subcommand: Some(ToolsSubcommand::Help),
                        },
                        other => {
                            return Err(anyhow!(ToolsSubcommand::usage_msg(format!(
                                "Unknown subcommand '{}'.",
                                other
                            ))));
                        },
                    }
                },
                "prompts" => {
                    let subcommand = parts.get(1);
                    match subcommand {
                        Some(c) if c.to_lowercase() == "list" => Self::Prompts {
                            subcommand: Some(PromptsSubcommand::List {
                                search_word: parts.get(2).map(|v| (*v).to_string()),
                            }),
                        },
                        Some(c) if c.to_lowercase() == "help" => Self::Prompts {
                            subcommand: Some(PromptsSubcommand::Help),
                        },
                        Some(c) if c.to_lowercase() == "get" => {
                            // Need to reconstruct the input because simple splitting of
                            // white space might not be sufficient
                            let command = parts[2..].join(" ");
                            let get_command = parse_input_to_prompts_get_command(command.as_str())?;
                            let subcommand = Some(PromptsSubcommand::Get { get_command });
                            Self::Prompts { subcommand }
                        },
                        Some(other) => {
                            return Err(anyhow!(PromptsSubcommand::usage_msg(format!(
                                "Unknown subcommand '{}'\n",
                                other
                            ))));
                        },
                        None => Self::Prompts {
                            subcommand: Some(PromptsSubcommand::List {
                                search_word: parts.get(2).map(|v| (*v).to_string()),
                            }),
                        },
                    }
                },
                "usage" => Self::Usage,
                unknown_command => {
                    // If the command starts with a slash but isn't recognized,
                    // return an error instead of treating it as a prompt
                    return Err(anyhow!(format!(
                        "Unknown command: '/{}'. Type '/help' to see available commands.\nTo use a literal slash at the beginning of your message, escape it with a backslash (e.g., '\\//hey' for '/hey').",
                        unknown_command
                    )));
                },
            });
        }

        if let Some(command) = input.strip_prefix('@') {
            let get_command = parse_input_to_prompts_get_command(command)?;
            let subcommand = Some(PromptsSubcommand::Get { get_command });
            return Ok(Self::Prompts { subcommand });
        }

        if let Some(command) = input.strip_prefix("!") {
            return Ok(Self::Execute {
                command: command.to_string(),
            });
        }

        Ok(Self::Ask {
            prompt: input.to_string(),
        })
    }

    // NOTE: Here we use clap to parse the hooks subcommand instead of parsing manually
    // like the rest of the file.
    // Since the hooks subcommand has a lot of options, this makes more sense.
    // Ideally, we parse everything with clap instead of trying to do it manually.
    // TODO: Move this to the Context commands parse function for better encapsulation
    pub fn parse_hooks(parts: &[&str]) -> Result<Self, String> {
        // Skip the first two parts ("/context" and "hooks")
        let args = match shlex::split(&parts[1..].join(" ")) {
            Some(args) => args,
            None => return Err("Failed to parse arguments".to_string()),
        };

        // Parse with Clap
        HooksCommand::try_parse_from(args)
            .map(|hooks_command| Self::Context {
                subcommand: ContextSubcommand::Hooks {
                    subcommand: Some(hooks_command.command),
                },
            })
            .map_err(|e| e.to_string())
    }
}

fn parse_input_to_prompts_get_command(command: &str) -> Result<PromptsGetCommand> {
    let input = shell_words::split(command).map_err(|e| anyhow!("Error splitting command for prompts: {:?}", e))?;
    let mut iter = input.into_iter();
    let prompt_name = iter
        .next()
        .ok_or_else(|| anyhow!("Prompt name needs to be specified"))?;
    let args = iter.collect::<Vec<_>>();
    let params = PromptsGetParam {
        name: prompt_name,
        arguments: { if args.is_empty() { None } else { Some(args) } },
    };
    let orig_input = Some(command.to_string());
    Ok(PromptsGetCommand { orig_input, params })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parse() {
        macro_rules! profile {
            ($subcommand:expr) => {
                Command::Profile {
                    subcommand: $subcommand,
                }
            };
        }
        macro_rules! context {
            ($subcommand:expr) => {
                Command::Context {
                    subcommand: $subcommand,
                }
            };
        }
        macro_rules! compact {
            ($prompt:expr, $show_summary:expr) => {
                Command::Compact {
                    prompt: $prompt,
                    show_summary: $show_summary,
                    help: false,
                }
            };
        }
        let tests = &[
            ("/compact", compact!(None, true)),
            (
                "/compact custom prompt",
                compact!(Some("custom prompt".to_string()), true),
            ),
            ("/profile list", profile!(ProfileSubcommand::List)),
            (
                "/profile create new_profile",
                profile!(ProfileSubcommand::Create {
                    name: "new_profile".to_string(),
                }),
            ),
            (
                "/profile delete p",
                profile!(ProfileSubcommand::Delete { name: "p".to_string() }),
            ),
            (
                "/profile rename old new",
                profile!(ProfileSubcommand::Rename {
                    old_name: "old".to_string(),
                    new_name: "new".to_string(),
                }),
            ),
            (
                "/profile set p",
                profile!(ProfileSubcommand::Set { name: "p".to_string() }),
            ),
            (
                "/profile set p",
                profile!(ProfileSubcommand::Set { name: "p".to_string() }),
            ),
            ("/context show", context!(ContextSubcommand::Show { expand: false })),
            (
                "/context show --expand",
                context!(ContextSubcommand::Show { expand: true }),
            ),
            (
                "/context add p1 p2",
                context!(ContextSubcommand::Add {
                    global: false,
                    force: false,
                    paths: vec!["p1".into(), "p2".into()]
                }),
            ),
            (
                "/context add --global --force p1 p2",
                context!(ContextSubcommand::Add {
                    global: true,
                    force: true,
                    paths: vec!["p1".into(), "p2".into()]
                }),
            ),
            (
                "/context rm p1 p2",
                context!(ContextSubcommand::Remove {
                    global: false,
                    paths: vec!["p1".into(), "p2".into()]
                }),
            ),
            (
                "/context rm --global p1 p2",
                context!(ContextSubcommand::Remove {
                    global: true,
                    paths: vec!["p1".into(), "p2".into()]
                }),
            ),
            ("/context clear", context!(ContextSubcommand::Clear { global: false })),
            (
                "/context clear --global",
                context!(ContextSubcommand::Clear { global: true }),
            ),
            ("/issue", Command::Issue { prompt: None }),
            ("/issue there was an error in the chat", Command::Issue {
                prompt: Some("there was an error in the chat".to_string()),
            }),
            ("/issue \"there was an error in the chat\"", Command::Issue {
                prompt: Some("\"there was an error in the chat\"".to_string()),
            }),
            (
                "/context hooks",
                context!(ContextSubcommand::Hooks { subcommand: None }),
            ),
            (
                "/context hooks add test --trigger per_prompt --command 'echo 1' --global",
                context!(ContextSubcommand::Hooks {
                    subcommand: Some(HooksSubcommand::Add {
                        name: "test".to_string(),
                        global: true,
                        trigger: "per_prompt".to_string(),
                        command: "echo 1".to_string()
                    })
                }),
            ),
            (
                "/context hooks rm test --global",
                context!(ContextSubcommand::Hooks {
                    subcommand: Some(HooksSubcommand::Remove {
                        name: "test".to_string(),
                        global: true
                    })
                }),
            ),
            (
                "/context hooks enable test --global",
                context!(ContextSubcommand::Hooks {
                    subcommand: Some(HooksSubcommand::Enable {
                        name: "test".to_string(),
                        global: true
                    })
                }),
            ),
            (
                "/context hooks disable test",
                context!(ContextSubcommand::Hooks {
                    subcommand: Some(HooksSubcommand::Disable {
                        name: "test".to_string(),
                        global: false
                    })
                }),
            ),
            (
                "/context hooks enable-all --global",
                context!(ContextSubcommand::Hooks {
                    subcommand: Some(HooksSubcommand::EnableAll { global: true })
                }),
            ),
            (
                "/context hooks disable-all",
                context!(ContextSubcommand::Hooks {
                    subcommand: Some(HooksSubcommand::DisableAll { global: false })
                }),
            ),
            (
                "/context hooks help",
                context!(ContextSubcommand::Hooks {
                    subcommand: Some(HooksSubcommand::Help)
                }),
            ),
        ];

        for (input, parsed) in tests {
            let result = Command::parse(input).unwrap_or_else(|_| panic!("Failed to parse command: {}", input));
            assert_eq!(&result, parsed, "{}", input);
        }
    }
}
/// Structure to hold command descriptions
#[derive(Debug, Clone)]
pub struct CommandDescription {
    pub short_description: String,
    pub full_description: String,
    #[allow(dead_code)]
    pub usage: String,
}

impl Command {
    /// Get the appropriate handler for this command variant
    pub fn to_handler(&self) -> &'static dyn CommandHandler {
        match self {
            Command::Help { .. } => &HELP_HANDLER,
            Command::Quit => &QUIT_HANDLER,
            Command::Clear => &CLEAR_HANDLER,
            Command::Context { subcommand } => subcommand.to_handler(),
            Command::Profile { subcommand } => subcommand.to_handler(), // Use the to_handler method on
            // ProfileSubcommand
            Command::Tools { subcommand } => match subcommand {
                Some(sub) => sub.to_handler(), // Use the to_handler method on ToolsSubcommand
                None => &crate::cli::chat::commands::tools::LIST_TOOLS_HANDLER, /* Default to list handler when no
                                                 * subcommand */
            },
            Command::Compact { .. } => &COMPACT_HANDLER,
            Command::PromptEditor { .. } => &EDITOR_HANDLER,
            Command::Usage => &USAGE_HANDLER,
            Command::Issue { .. } => &ISSUE_HANDLER,
            // These commands are not handled through the command system
            Command::Ask { .. } => &HELP_HANDLER,        // Fallback to help handler
            Command::Execute { .. } => &EXECUTE_HANDLER, // Use the dedicated execute handler
            Command::Prompts { subcommand } => match subcommand {
                Some(sub) => sub.to_handler(),
                None => &crate::cli::chat::commands::prompts::LIST_PROMPTS_HANDLER, /* Default to list handler when
                                                                                     * no subcommand */
            },
        }
    }

    /// Parse a command from components
    ///
    /// This method formats a command string from its components and parses it into a Command enum.
    ///
    /// # Arguments
    ///
    /// * `command` - The base command name
    /// * `subcommand` - Optional subcommand
    /// * `args` - Optional arguments
    /// * `flags` - Optional flags
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - The parsed Command enum
    pub fn parse_from_components(
        command: &str,
        subcommand: Option<&String>,
        args: Option<&Vec<String>>,
        flags: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<Self> {
        // Format the command string
        let mut cmd_str = if !command.starts_with('/') {
            format!("/{}", command)
        } else {
            command.to_string()
        };

        // Add subcommand if present
        if let Some(subcommand) = subcommand {
            cmd_str.push_str(&format!(" {}", subcommand));
        }

        // Add arguments if present
        if let Some(args) = args {
            for arg in args {
                cmd_str.push_str(&format!(" {}", arg));
            }
        }

        // Add flags if present
        if let Some(flags) = flags {
            for (flag, value) in flags {
                if value.is_empty() {
                    cmd_str.push_str(&format!(" --{}", flag));
                } else {
                    cmd_str.push_str(&format!(" --{}={}", flag, value));
                }
            }
        }

        // Parse the formatted command string
        Self::parse(&cmd_str)
    }

    /// Execute the command directly with ChatContext
    pub async fn execute<'a>(
        &'a self,
        chat_context: &'a mut crate::cli::chat::ChatContext,
        tool_uses: Option<Vec<crate::cli::chat::QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<crate::cli::chat::ChatState, crate::cli::chat::ChatError> {
        // Get the appropriate handler and delegate to it
        let handler = self.to_handler();

        // Create a CommandContextAdapter from the ChatContext
        let mut adapter = chat_context.command_context_adapter();

        handler
            .execute_command(self, &mut adapter, tool_uses, pending_tool_index)
            .await
    }

    /// Returns a vector of all available commands for dynamic enumeration
    pub fn all_commands() -> Vec<(&'static str, &'static dyn CommandHandler)> {
        vec![
            ("help", &HELP_HANDLER as &dyn CommandHandler),
            ("quit", &QUIT_HANDLER as &dyn CommandHandler),
            ("clear", &CLEAR_HANDLER as &dyn CommandHandler),
            ("context", &CONTEXT_HANDLER as &dyn CommandHandler),
            ("profile", &PROFILE_HANDLER as &dyn CommandHandler),
            ("tools", &TOOLS_HANDLER as &dyn CommandHandler),
            ("compact", &COMPACT_HANDLER as &dyn CommandHandler),
            ("usage", &USAGE_HANDLER as &dyn CommandHandler),
            ("editor", &EDITOR_HANDLER as &dyn CommandHandler),
            ("issue", &ISSUE_HANDLER as &dyn CommandHandler),
        ]
    }

    /// Generate descriptions for all commands for LLM tool descriptions
    ///
    /// This method dynamically iterates through all available commands and collects
    /// their descriptions for use in LLM integration. This ensures that all commands
    /// are properly described and no commands are missed when new ones are added.
    pub fn generate_llm_descriptions() -> std::collections::HashMap<String, CommandDescription> {
        let mut descriptions = std::collections::HashMap::new();

        // Dynamically iterate through all commands
        for (name, handler) in Self::all_commands() {
            descriptions.insert(name.to_string(), CommandDescription {
                short_description: handler.description().to_string(),
                full_description: handler.llm_description(),
                usage: handler.usage().to_string(),
            });
        }

        descriptions
    }
}
