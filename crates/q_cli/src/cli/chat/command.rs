use std::io::Write;

use crossterm::style::Color;
use crossterm::{
    queue,
    style,
};
use eyre::Result;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Ask { prompt: String },
    Execute { command: String },
    Clear,
    Help,
    Issue { prompt: Option<String> },
    Quit,
    Profile { subcommand: ProfileSubcommand },
    Context { subcommand: ContextSubcommand },
    PromptEditor { initial_text: Option<String> },
    Tools { subcommand: Option<ToolsSubcommand> },
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
                                 <black!>--global: Remove global rules</black!>"};
    const CLEAR_USAGE: &str = "/context clear [--global]";
    const REMOVE_USAGE: &str = "/context rm [--global] <path1> [path2...]";
    const SHOW_USAGE: &str = "/context show [--expand]";

    fn usage_msg(header: impl AsRef<str>) -> String {
        format!("{}\n\n{}", header.as_ref(), Self::AVAILABLE_COMMANDS)
    }

    pub fn help_text() -> String {
        color_print::cformat!(
            r#"
<magenta,em>(Beta) Context Rule Management</magenta,em>

Context rules determine which files are included in your Amazon Q session. 
The files matched by these rules provide Amazon Q with additional information 
about your project or environment. Adding relevant files helps Q generate 
more accurate and helpful responses.

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolsSubcommand {
    Trust { tool_name: String },
    Untrust { tool_name: String },
    TrustAll,
    Reset,
    Help,
}

impl ToolsSubcommand {
    const AVAILABLE_COMMANDS: &str = color_print::cstr! {"<cyan!>Available subcommands</cyan!>
  <em>help</em>                           <black!>Show an explanation for the tools command</black!>
  <em>trust <<tool name>></em>              <black!>Trust a specific tool for the session</black!>
  <em>untrust <<tool name>></em>            <black!>Revert a tool to per-request confirmation</black!>
  <em>trustall</em>                       <black!>Trust all tools (equivalent to deprecated /acceptall)</black!>
  <em>reset</em>                          <black!>Reset all tools to default permission levels</black!>"};
    const BASE_COMMAND: &str = color_print::cstr! {"<cyan!>Usage: /tools [SUBCOMMAND]</cyan!>

<cyan!>Description</cyan!>
  Show the current set of tools and their permission setting.
  The permission setting states when user confirmation is required. Trusted tools never require confirmation.
  Alternatively, specify a subcommand to modify the tool permissions."};
    const TRUST_USAGE: &str = "/tools trust <tool name>";
    const UNTRUST_USAGE: &str = "/tools untrust <tool name>";

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

        if let Some(command) = input.strip_prefix("/") {
            let parts: Vec<&str> = command.split_whitespace().collect();

            if parts.is_empty() {
                return Err("Empty command".to_string());
            }

            return Ok(match parts[0].to_lowercase().as_str() {
                "clear" => Self::Clear,
                "help" => Self::Help,
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
                        return Err(ProfileSubcommand::usage_msg("Missing subcommand for /profile."));
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

                            for part in &parts[2..] {
                                if *part == "--global" {
                                    global = true;
                                } else if *part == "--force" || *part == "-f" {
                                    force = true;
                                } else {
                                    paths.push((*part).to_string());
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

                            for part in &parts[2..] {
                                if *part == "--global" {
                                    global = true;
                                } else {
                                    paths.push((*part).to_string());
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
                        other => {
                            return Err(ContextSubcommand::usage_msg(format!("Unknown subcommand '{}'.", other)));
                        },
                    }
                },
                "tools" => {
                    if parts.len() < 2 {
                        return Ok(Self::Tools { subcommand: None });
                    }

                    macro_rules! usage_err {
                        ($subcommand:expr, $usage_str:expr) => {
                            return Err(format!(
                                "Invalid /tools {} arguments.\n\nUsage:\n  {}",
                                $subcommand, $usage_str
                            ))
                        };
                    }

                    match parts[1].to_lowercase().as_str() {
                        "trust" => {
                            let tool_name = parts.get(2);
                            match tool_name {
                                Some(tool_name) => Self::Tools {
                                    subcommand: Some(ToolsSubcommand::Trust {
                                        tool_name: (*tool_name).to_string(),
                                    }),
                                },
                                None => usage_err!("trust", ToolsSubcommand::TRUST_USAGE),
                            }
                        },
                        "untrust" => {
                            let tool_name = parts.get(2);
                            match tool_name {
                                Some(tool_name) => Self::Tools {
                                    subcommand: Some(ToolsSubcommand::Untrust {
                                        tool_name: (*tool_name).to_string(),
                                    }),
                                },
                                None => usage_err!("untrust", ToolsSubcommand::UNTRUST_USAGE),
                            }
                        },
                        "trustall" => Self::Tools {
                            subcommand: Some(ToolsSubcommand::TrustAll),
                        },
                        "reset" => Self::Tools {
                            subcommand: Some(ToolsSubcommand::Reset),
                        },
                        "help" => Self::Tools {
                            subcommand: Some(ToolsSubcommand::Help),
                        },
                        other => {
                            return Err(ToolsSubcommand::usage_msg(format!("Unknown subcommand '{}'.", other)));
                        },
                    }
                },
                _ => {
                    return Ok(Self::Ask {
                        prompt: input.to_string(),
                    });
                },
            });
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
        let tests = &[
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
