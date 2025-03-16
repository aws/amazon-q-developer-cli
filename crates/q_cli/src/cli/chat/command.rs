use eyre::Result;

#[derive(Debug)]
pub enum Command {
    Ask {
        prompt: String,
    },
    Execute {
        command: String,
    },
    Clear,
    Help,
    AcceptAll,
    Quit,
    #[allow(dead_code)]
    Context {
        subcommand: ContextSubcommand,
    },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ContextSubcommand {
    Show,
    Add {
        global: bool,
        paths: Vec<String>,
    },
    Remove {
        global: bool,
        paths: Vec<String>,
    },
    Profile {
        delete: Option<String>,
        create: Option<String>,
    },
    Switch {
        name: String,
        create: bool,
    },
    Clear {
        global: bool,
    },
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
                "context" => {
                    if parts.len() < 2 {
                        return Err("Missing subcommand for /context. Try /help for available commands.".to_string());
                    }

                    match parts[1].to_lowercase().as_str() {
                        "show" => Self::Context {
                            subcommand: ContextSubcommand::Show,
                        },
                        "add" => {
                            // Parse add command with paths and --global flag
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
                                return Err("No paths specified for /context add".to_string());
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Add { global, paths },
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
                                return Err("No paths specified for /context rm".to_string());
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Remove { global, paths },
                            }
                        },
                        "profile" => {
                            // Parse profile command with optional --create or --delete flags
                            let mut create = None;
                            let mut delete = None;

                            let mut i = 2;
                            while i < parts.len() {
                                match parts.get(i) {
                                    Some(&"--create" | &"-c") => {
                                        if i + 1 < parts.len() {
                                            create = Some(parts[i + 1].to_string());
                                            i += 2;
                                        } else {
                                            return Err("Missing profile name for --create".to_string());
                                        }
                                    },
                                    Some(&"--delete" | &"-d") => {
                                        if i + 1 < parts.len() {
                                            delete = Some(parts[i + 1].to_string());
                                            i += 2;
                                        } else {
                                            return Err("Missing profile name for --delete".to_string());
                                        }
                                    },
                                    Some(part) => {
                                        return Err(format!("Unknown option for /context profile: {}", part));
                                    },
                                    None => break,
                                }
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Profile { create, delete },
                            }
                        },
                        "switch" => {
                            // Parse switch command with profile name and optional --create flag
                            let mut create = false;
                            let mut name = None;

                            for part in &parts[2..] {
                                if *part == "--create" || *part == "-c" {
                                    create = true;
                                } else if name.is_none() {
                                    name = Some((*part).to_string());
                                } else {
                                    return Err("Too many arguments for /context switch".to_string());
                                }
                            }

                            if let Some(name) = name {
                                Self::Context {
                                    subcommand: ContextSubcommand::Switch { name, create },
                                }
                            } else {
                                return Err("Missing profile name for /context switch. Usage: /context switch <profile-name> [--create]".to_string());
                            }
                        },
                        "clear" => {
                            // Parse clear command with optional --global flag
                            let mut global = false;

                            for part in &parts[2..] {
                                if *part == "--global" {
                                    global = true;
                                } else {
                                    return Err(format!("Unknown option for /context clear: {}", part));
                                }
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Clear { global },
                            }
                        },
                        _ => return Err(format!("Unknown context subcommand: {}", parts[1])),
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
    fn test_parse_context_show() {
        let cmd = Command::parse("/context show").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Show,
            } => {},
            _ => panic!("Expected Context Show command"),
        }
    }

    #[test]
    fn test_parse_context_add() {
        let cmd = Command::parse("/context add path1 path2").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Add { global, paths },
            } => {
                assert!(!global);
                assert_eq!(paths, vec!["path1", "path2"]);
            },
            _ => panic!("Expected Context Add command"),
        }
    }

    #[test]
    fn test_parse_context_add_global() {
        let cmd = Command::parse("/context add --global path1 path2").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Add { global, paths },
            } => {
                assert!(global);
                assert_eq!(paths, vec!["path1", "path2"]);
            },
            _ => panic!("Expected Context Add command with global flag"),
        }
    }

    #[test]
    fn test_parse_context_rm() {
        let cmd = Command::parse("/context rm path1 path2").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Remove { global, paths },
            } => {
                assert!(!global);
                assert_eq!(paths, vec!["path1", "path2"]);
            },
            _ => panic!("Expected Context Remove command"),
        }
    }

    #[test]
    fn test_parse_context_rm_global() {
        let cmd = Command::parse("/context rm --global path1 path2").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Remove { global, paths },
            } => {
                assert!(global);
                assert_eq!(paths, vec!["path1", "path2"]);
            },
            _ => panic!("Expected Context Remove command with global flag"),
        }
    }

    #[test]
    fn test_parse_context_profile() {
        let cmd = Command::parse("/context profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Profile { delete, create },
            } => {
                assert!(delete.is_none());
                assert!(create.is_none());
            },
            _ => panic!("Expected Context Profile command"),
        }
    }

    #[test]
    fn test_parse_context_profile_create() {
        let cmd = Command::parse("/context profile --create my-profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Profile { delete, create },
            } => {
                assert!(delete.is_none());
                assert_eq!(create, Some("my-profile".to_string()));
            },
            _ => panic!("Expected Context Profile command with create option"),
        }
    }

    #[test]
    fn test_parse_context_profile_delete() {
        let cmd = Command::parse("/context profile --delete my-profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Profile { delete, create },
            } => {
                assert_eq!(delete, Some("my-profile".to_string()));
                assert!(create.is_none());
            },
            _ => panic!("Expected Context Profile command with delete option"),
        }
    }

    #[test]
    fn test_parse_context_switch() {
        let cmd = Command::parse("/context switch my-profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Switch { name, create },
            } => {
                assert_eq!(name, "my-profile");
                assert!(!create);
            },
            _ => panic!("Expected Context Switch command"),
        }
    }

    #[test]
    fn test_parse_context_switch_create() {
        let cmd = Command::parse("/context switch --create my-profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Switch { name, create },
            } => {
                assert_eq!(name, "my-profile");
                assert!(create);
            },
            _ => panic!("Expected Context Switch command with create flag"),
        }
    }

    #[test]
    fn test_parse_context_clear() {
        let cmd = Command::parse("/context clear").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Clear { global },
            } => {
                assert!(!global);
            },
            _ => panic!("Expected Context Clear command"),
        }
    }

    #[test]
    fn test_parse_context_clear_global() {
        let cmd = Command::parse("/context clear --global").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Clear { global },
            } => {
                assert!(global);
            },
            _ => panic!("Expected Context Clear command with global flag"),
        }
    }

    #[test]
    fn test_parse_context_error_no_subcommand() {
        let result = Command::parse("/context");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Missing subcommand for /context. Try /help for available commands."
        );
    }

    #[test]
    fn test_parse_context_error_unknown_subcommand() {
        let result = Command::parse("/context unknown");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Unknown context subcommand: unknown");
    }

    #[test]
    fn test_parse_context_add_error_no_paths() {
        let result = Command::parse("/context add");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No paths specified for /context add");
    }

    #[test]
    fn test_parse_context_add_error_only_global_flag() {
        let result = Command::parse("/context add --global");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No paths specified for /context add");
    }

    #[test]
    fn test_parse_context_rm_error_no_paths() {
        let result = Command::parse("/context rm");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No paths specified for /context rm");
    }

    #[test]
    fn test_parse_context_profile_create_error_no_name() {
        let result = Command::parse("/context profile --create");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Missing profile name for --create");
    }

    #[test]
    fn test_parse_context_profile_delete_error_no_name() {
        let result = Command::parse("/context profile --delete");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Missing profile name for --delete");
    }

    #[test]
    fn test_parse_context_switch_error_no_name() {
        let result = Command::parse("/context switch");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Missing profile name for /context switch. Usage: /context switch <profile-name> [--create]"
        );
    }

    #[test]
    fn test_parse_context_switch_error_only_create_flag() {
        let result = Command::parse("/context switch --create");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Missing profile name for /context switch. Usage: /context switch <profile-name> [--create]"
        );
    }
}
