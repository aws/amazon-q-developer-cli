use std::collections::HashSet;
use std::io::Write;

use clap::{
    Parser,
    Subcommand,
};
use crossterm::style::Color;
use crossterm::{
    queue,
    style,
};
use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Ask {
        prompt: String,
    },
    Execute {
        command: String,
    },
    Clear,
    Help,
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
    TrustAll,
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
    // Check if input is a common single-word command that should use slash prefix
    fn check_common_command(input: &str) -> Option<String> {
        let input_lower = input.trim().to_lowercase();
        match input_lower.as_str() {
            "exit" | "quit" | "q" | "exit()" => {
                Some("Did you mean to use the command '/quit' to exit? Type '/quit' to exit.".to_string())
            },
            "clear" | "cls" => Some(
                "Did you mean to use the command '/clear' to clear the conversation? Type '/clear' to clear."
                    .to_string(),
            ),
            "help" | "?" => Some(
                "Did you mean to use the command '/help' for help? Type '/help' to see available commands.".to_string(),
            ),
            _ => None,
        }
    }

    pub fn parse(input: &str, output: &mut impl Write) -> Result<Self, String> {
        let input = input.trim();

        // Check for common single-word commands without slash prefix
        if let Some(suggestion) = Self::check_common_command(input) {
            return Err(suggestion);
        }

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
                return Err("Empty command".to_string());
            }

            return Ok(match parts[0].to_lowercase().as_str() {
                "clear" => Self::Clear,
                "help" => Self::Help,
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
                    let _ = queue!(
                        output,
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("\n/acceptall is deprecated. Use /tools instead.\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    );

                    Self::Tools {
                        subcommand: Some(ToolsSubcommand::TrustAll),
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
                            return Err(format!(
                                "Invalid /profile arguments.\n\nUsage:\n  {}",
                                $usage_str
                            ))
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
                            return Err(ProfileSubcommand::usage_msg(format!("Unknown subcommand '{}'.", other)));
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
                            return Err(format!(
                                "Invalid /context arguments.\n\nUsage:\n  {}",
                                $usage_str
                            ))
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
                                None => return Err("Failed to parse quoted arguments".to_string()),
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
                                None => return Err("Failed to parse quoted arguments".to_string()),
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
                                Err(err) => return Err(ContextSubcommand::hooks_usage_msg(err)),
                            }
                        },
                        other => {
                            return Err(ContextSubcommand::usage_msg(format!("Unknown subcommand '{}'.", other)));
                        },
                    }
                },
                "tools" => {
                    if parts.len() < 2 {
                        return Ok(Self::Tools { subcommand: None });
                    }

                    match parts[1].to_lowercase().as_str() {
                        "schema" => Self::Tools {
                            subcommand: Some(ToolsSubcommand::Schema),
                        },
                        "trust" => {
                            let mut tool_names = HashSet::new();
                            for part in &parts[2..] {
                                tool_names.insert((*part).to_string());
                            }

                            if tool_names.is_empty() {
                                let _ = queue!(
                                    output,
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print("\nPlease use"),
                                    style::SetForegroundColor(Color::DarkGreen),
                                    style::Print(" /tools trust <tool1> <tool2>"),
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print(" to trust tools.\n\n"),
                                    style::Print("Use "),
                                    style::SetForegroundColor(Color::DarkGreen),
                                    style::Print("/tools"),
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print(" to see all available tools.\n\n"),
                                    style::SetForegroundColor(Color::Reset),
                                );
                            }

                            Self::Tools {
                                subcommand: Some(ToolsSubcommand::Trust { tool_names }),
                            }
                        },
                        "untrust" => {
                            let mut tool_names = HashSet::new();
                            for part in &parts[2..] {
                                tool_names.insert((*part).to_string());
                            }

                            if tool_names.is_empty() {
                                let _ = queue!(
                                    output,
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print("\nPlease use"),
                                    style::SetForegroundColor(Color::DarkGreen),
                                    style::Print(" /tools untrust <tool1> <tool2>"),
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print(" to untrust tools.\n\n"),
                                    style::Print("Use "),
                                    style::SetForegroundColor(Color::DarkGreen),
                                    style::Print("/tools"),
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print(" to see all available tools.\n\n"),
                                    style::SetForegroundColor(Color::Reset),
                                );
                            }

                            Self::Tools {
                                subcommand: Some(ToolsSubcommand::Untrust { tool_names }),
                            }
                        },
                        "trustall" => Self::Tools {
                            subcommand: Some(ToolsSubcommand::TrustAll),
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
                            return Err(ToolsSubcommand::usage_msg(format!("Unknown subcommand '{}'.", other)));
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
                            return Err(PromptsSubcommand::usage_msg(format!(
                                "Unknown subcommand '{}'\n",
                                other
                            )));
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
                    let looks_like_path = {
                        let after_slash_command_str = parts[1..].join(" ");
                        unknown_command.contains('/')
                            || unknown_command.contains('.')
                            || unknown_command.contains('\\')
                            || after_slash_command_str.contains('/')
                            || after_slash_command_str.contains('.')
                            || after_slash_command_str.contains('\\')
                    };
                    if looks_like_path {
                        return Ok(Self::Ask {
                            prompt: command.to_string(),
                        });
                    }

                    return Err(format!(
                        "Unknown command: '/{}'. Type '/help' to see available commands.\nTo use a literal slash at the beginning of your message, escape it with a backslash (e.g., '\\//hey' for '/hey').",
                        unknown_command
                    ));
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
    fn parse_hooks(parts: &[&str]) -> Result<Self, String> {
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

fn parse_input_to_prompts_get_command(command: &str) -> Result<PromptsGetCommand, String> {
    let input = shell_words::split(command).map_err(|e| format!("Error splitting command for prompts: {:?}", e))?;
    let mut iter = input.into_iter();
    let prompt_name = iter.next().ok_or("Prompt name needs to be specified")?;
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
        let mut stdout = std::io::stdout();

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
            assert_eq!(&Command::parse(input, &mut stdout).unwrap(), parsed, "{}", input);
        }
    }

    #[test]
    fn test_common_command_suggestions() {
        let mut stdout = std::io::stdout();
        let test_cases = vec![
            (
                "exit",
                "Did you mean to use the command '/quit' to exit? Type '/quit' to exit.",
            ),
            (
                "quit",
                "Did you mean to use the command '/quit' to exit? Type '/quit' to exit.",
            ),
            (
                "q",
                "Did you mean to use the command '/quit' to exit? Type '/quit' to exit.",
            ),
            (
                "clear",
                "Did you mean to use the command '/clear' to clear the conversation? Type '/clear' to clear.",
            ),
            (
                "cls",
                "Did you mean to use the command '/clear' to clear the conversation? Type '/clear' to clear.",
            ),
            (
                "help",
                "Did you mean to use the command '/help' for help? Type '/help' to see available commands.",
            ),
            (
                "?",
                "Did you mean to use the command '/help' for help? Type '/help' to see available commands.",
            ),
        ];

        for (input, expected_message) in test_cases {
            let result = Command::parse(input, &mut stdout);
            assert!(result.is_err(), "Expected error for input: {}", input);
            assert_eq!(result.unwrap_err(), expected_message);
        }
    }
}
