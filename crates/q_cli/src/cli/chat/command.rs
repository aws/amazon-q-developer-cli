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
    Profile {
        delete: Option<String>,
        create: Option<String>,
        rename: Option<(String, String)>, // (old_name, new_name)
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
                        "show" => {
                            // Parse show command with optional --expand flag
                            let mut expand = false;

                            for part in &parts[2..] {
                                if *part == "--expand" {
                                    expand = true;
                                } else {
                                    return Err(format!("Unknown option for /context show: {}", part));
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
                                return Err("No paths specified for /context add".to_string());
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
                                return Err("No paths specified for /context rm".to_string());
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Remove { global, paths },
                            }
                        },
                        "profile" => {
                            // Parse profile command with optional --create, --delete, or --rename flags
                            let mut create = None;
                            let mut delete = None;
                            let mut rename = None;

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
                                    Some(&"--rename" | &"-r") => {
                                        if i + 2 < parts.len() {
                                            rename = Some((parts[i + 1].to_string(), parts[i + 2].to_string()));
                                            i += 3;
                                        } else {
                                            return Err("Missing profile names for --rename. Usage: --rename <old_name> <new_name>".to_string());
                                        }
                                    },
                                    Some(part) => {
                                        return Err(format!("Unknown option for /context profile: {}", part));
                                    },
                                    None => break,
                                }
                            }

                            // Ensure only one operation is specified
                            let operations = [delete.is_some(), create.is_some(), rename.is_some()];
                            if operations.iter().filter(|&&x| x).count() > 1 {
                                return Err("Only one of --delete, --create, or --rename can be specified".to_string());
                            }

                            Self::Context {
                                subcommand: ContextSubcommand::Profile { create, delete, rename },
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
                subcommand: ContextSubcommand::Show { expand },
            } => {
                assert!(!expand);
            },
            _ => panic!("Expected Context Show command"),
        }
    }

    #[test]
    fn test_parse_context_show_expand() {
        let cmd = Command::parse("/context show --expand").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Show { expand },
            } => {
                assert!(expand);
            },
            _ => panic!("Expected Context Show command with expand flag"),
        }
    }

    #[test]
    fn test_parse_context_add() {
        let cmd = Command::parse("/context add path1 path2").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Add { global, force, paths },
            } => {
                assert!(!global);
                assert!(!force);
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
                subcommand: ContextSubcommand::Add { global, force, paths },
            } => {
                assert!(global);
                assert!(!force);
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
                subcommand: ContextSubcommand::Profile { delete, create, rename },
            } => {
                assert!(delete.is_none());
                assert!(create.is_none());
                assert!(rename.is_none());
            },
            _ => panic!("Expected Context Profile command"),
        }
    }

    #[test]
    fn test_parse_context_profile_create() {
        let cmd = Command::parse("/context profile --create my-profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Profile { delete, create, rename },
            } => {
                assert!(delete.is_none());
                assert_eq!(create, Some("my-profile".to_string()));
                assert!(rename.is_none());
            },
            _ => panic!("Expected Context Profile command with create option"),
        }
    }

    #[test]
    fn test_parse_context_profile_delete() {
        let cmd = Command::parse("/context profile --delete my-profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Profile { delete, create, rename },
            } => {
                assert_eq!(delete, Some("my-profile".to_string()));
                assert!(create.is_none());
                assert!(rename.is_none());
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
    fn test_parse_context_profile_rename() {
        let cmd = Command::parse("/context profile --rename old-profile new-profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Profile { delete, create, rename },
            } => {
                assert!(delete.is_none());
                assert!(create.is_none());
                assert_eq!(rename, Some(("old-profile".to_string(), "new-profile".to_string())));
            },
            _ => panic!("Expected Context Profile command with rename option"),
        }
    }

    #[test]
    fn test_parse_context_profile_rename_short_flag() {
        let cmd = Command::parse("/context profile -r old-profile new-profile").unwrap();
        match cmd {
            Command::Context {
                subcommand: ContextSubcommand::Profile { delete, create, rename },
            } => {
                assert!(delete.is_none());
                assert!(create.is_none());
                assert_eq!(rename, Some(("old-profile".to_string(), "new-profile".to_string())));
            },
            _ => panic!("Expected Context Profile command with rename option"),
        }
    }

    #[test]
    fn test_parse_context_profile_rename_error_missing_names() {
        let result = Command::parse("/context profile --rename");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Missing profile names for --rename. Usage: --rename <old_name> <new_name>"
        );
    }

    #[test]
    fn test_parse_context_profile_rename_error_missing_new_name() {
        let result = Command::parse("/context profile --rename old-profile");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Missing profile names for --rename. Usage: --rename <old_name> <new_name>"
        );
    }

    #[test]
    fn test_parse_context_profile_multiple_operations() {
        let result = Command::parse("/context profile --create new-profile --rename old-profile new-profile");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Only one of --delete, --create, or --rename can be specified"
        );
    }
}

#[test]
fn test_parse_context_profile_rename() {
    let cmd = Command::parse("/context profile --rename old-profile new-profile").unwrap();
    match cmd {
        Command::Context {
            subcommand: ContextSubcommand::Profile { delete, create, rename },
        } => {
            assert!(delete.is_none());
            assert!(create.is_none());
            assert_eq!(rename, Some(("old-profile".to_string(), "new-profile".to_string())));
        },
        _ => panic!("Expected Context Profile command with rename option"),
    }
}

#[test]
fn test_parse_context_profile_rename_short_flag() {
    let cmd = Command::parse("/context profile -r old-profile new-profile").unwrap();
    match cmd {
        Command::Context {
            subcommand: ContextSubcommand::Profile { delete, create, rename },
        } => {
            assert!(delete.is_none());
            assert!(create.is_none());
            assert_eq!(rename, Some(("old-profile".to_string(), "new-profile".to_string())));
        },
        _ => panic!("Expected Context Profile command with rename option"),
    }
}

#[test]
fn test_parse_context_profile_rename_error_missing_names() {
    let result = Command::parse("/context profile --rename");
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Missing profile names for --rename. Usage: --rename <old_name> <new_name>"
    );
}

#[test]
fn test_parse_context_profile_rename_error_missing_new_name() {
    let result = Command::parse("/context profile --rename old-profile");
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Missing profile names for --rename. Usage: --rename <old_name> <new_name>"
    );
}

#[test]
fn test_parse_context_profile_multiple_operations() {
    let result = Command::parse("/context profile --create new-profile --rename old-profile new-profile");
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Only one of --delete, --create, or --rename can be specified"
    );
}
#[test]
fn test_parse_context_add_force() {
    let cmd = Command::parse("/context add --force path1 path2").unwrap();
    match cmd {
        Command::Context {
            subcommand: ContextSubcommand::Add { global, force, paths },
        } => {
            assert!(!global);
            assert!(force);
            assert_eq!(paths, vec!["path1", "path2"]);
        },
        _ => panic!("Expected Context Add command with force flag"),
    }
}

#[test]
fn test_parse_context_add_global_force() {
    let cmd = Command::parse("/context add --global --force path1 path2").unwrap();
    match cmd {
        Command::Context {
            subcommand: ContextSubcommand::Add { global, force, paths },
        } => {
            assert!(global);
            assert!(force);
            assert_eq!(paths, vec!["path1", "path2"]);
        },
        _ => panic!("Expected Context Add command with global and force flags"),
    }
}

#[test]
fn test_parse_context_add_short_force() {
    let cmd = Command::parse("/context add -f path1 path2").unwrap();
    match cmd {
        Command::Context {
            subcommand: ContextSubcommand::Add { global, force, paths },
        } => {
            assert!(!global);
            assert!(force);
            assert_eq!(paths, vec!["path1", "path2"]);
        },
        _ => panic!("Expected Context Add command with short force flag"),
    }
}
