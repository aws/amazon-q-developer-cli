use eyre::Result;

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Ask { prompt: String },
    Execute { command: String },
    Clear,
    Help,
    AcceptAll,
    Quit,
    Profile { subcommand: ProfileSubcommand },
    Context { subcommand: ContextSubcommand },
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
<magenta,em>Profile Management</magenta,em>

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
  <em>show [--expand]</em>                <black!>Display current context configuration</black!>
                                 <black!>Use --expand to list all matched files</black!>

  <em>add [--global] [--force] <<paths...>></em>
                                 <black!>Add file(s) to context</black!>
                                 <black!>--global: Add to global context (available in all profiles)</black!>
                                 <black!>--force: Add files even if they exceed size limits</black!>

  <em>rm [--global] <<paths...>></em>       <black!>Remove file(s) from context</black!>
                                 <black!>--global: Remove from global context</black!>

  <em>clear [--global]</em>               <black!>Clear all files from current context</black!>
                                 <black!>--global: Clear global context</black!>"};
    const CLEAR_USAGE: &str = "/context clear [--global]";
    const REMOVE_USAGE: &str = "/context rm [--global] <path1> [path2...]";
    const SHOW_USAGE: &str = "/context show [--expand]";

    fn usage_msg(header: impl AsRef<str>) -> String {
        format!("{}\n\n{}", header.as_ref(), Self::AVAILABLE_COMMANDS)
    }

    pub fn help_text() -> String {
        color_print::cformat!(
            r#"
<magenta,em>Context Management</magenta,em>

Context files provide Amazon Q with additional information about your project or environment.
Adding relevant files to your context helps Amazon Q provide more accurate and helpful responses.

{}

<cyan!>Notes</cyan!>
• You can add specific files or use glob patterns (e.g., "*.py", "src/**/*.js")
• Context files are associated with the current profile
• Global context files are available across all profiles
• Context is preserved between chat sessions
"#,
            Self::AVAILABLE_COMMANDS
        )
    }
}

impl Command {
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();

        if let Some(command) = input.strip_prefix("/") {
            let parts: Vec<&str> = command.split_whitespace().collect();

            if parts.is_empty() {
                return Err("Empty command".to_string());
            }

            return Ok(match parts[0].to_lowercase().as_str() {
                "clear" => Self::Clear,
                "help" => Self::Help,
                "acceptall" => Self::AcceptAll,
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
                        return Err(ContextSubcommand::usage_msg("Missing subcommand for /context."));
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
                            // Parse show command with optional --expand flag
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
                _ => return Err(format!("Unknown command: {}", input)),
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
        ];

        for (input, parsed) in tests {
            assert_eq!(&Command::parse(input).unwrap(), parsed, "{}", input);
        }
    }
}
