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
        /// Tuple containing the old profile name and new profile name
        rename: Option<(String, String)>,
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

    /// Helper function to assert that a command parses to a Context command with the expected
    /// subcommand
    fn assert_context_command<F>(input: &str, assertion: F)
    where
        F: FnOnce(ContextSubcommand),
    {
        match Command::parse(input).unwrap() {
            Command::Context { subcommand } => assertion(subcommand),
            cmd => panic!("Expected Context command, got {:?}", cmd),
        }
    }

    /// Helper function to assert that a command parsing results in an error
    fn assert_parse_error(input: &str, expected_error: &str) {
        let result = Command::parse(input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), expected_error);
    }

    /// Helper function to assert that a command parses to a Context Add subcommand with the
    /// expected parameters
    fn assert_context_add_command(input: &str, expected_global: bool, expected_force: bool, expected_paths: Vec<&str>) {
        assert_context_command(input, |subcommand| match subcommand {
            ContextSubcommand::Add { global, force, paths } => {
                assert_eq!(global, expected_global);
                assert_eq!(force, expected_force);
                assert_eq!(paths, expected_paths);
            },
            _ => panic!("Expected Add subcommand"),
        });
    }

    /// Helper function to assert that a command parses to a Context Remove subcommand with the
    /// expected parameters
    fn assert_context_remove_command(input: &str, expected_global: bool, expected_paths: Vec<&str>) {
        assert_context_command(input, |subcommand| match subcommand {
            ContextSubcommand::Remove { global, paths } => {
                assert_eq!(global, expected_global);
                assert_eq!(paths, expected_paths);
            },
            _ => panic!("Expected Remove subcommand"),
        });
    }

    /// Helper function to assert that a command parses to a Context Profile subcommand with the
    /// expected parameters
    fn assert_context_profile_command(
        input: &str,
        expected_delete: Option<&str>,
        expected_create: Option<&str>,
        expected_rename: Option<(&str, &str)>,
    ) {
        assert_context_command(input, |subcommand| match subcommand {
            ContextSubcommand::Profile { delete, create, rename } => {
                assert_eq!(delete, expected_delete.map(String::from));
                assert_eq!(create, expected_create.map(String::from));
                assert_eq!(
                    rename,
                    expected_rename.map(|(old, new)| (old.to_string(), new.to_string()))
                );
            },
            _ => panic!("Expected Profile subcommand"),
        });
    }

    /// Helper function to assert that a command parses to a Context Switch subcommand with the
    /// expected parameters
    fn assert_context_switch_command(input: &str, expected_name: &str, expected_create: bool) {
        assert_context_command(input, |subcommand| match subcommand {
            ContextSubcommand::Switch { name, create } => {
                assert_eq!(name, expected_name);
                assert_eq!(create, expected_create);
            },
            _ => panic!("Expected Switch subcommand"),
        });
    }

    /// Helper function to assert that a command parses to a Context Clear subcommand with the
    /// expected parameters
    fn assert_context_clear_command(input: &str, expected_global: bool) {
        assert_context_command(input, |subcommand| match subcommand {
            ContextSubcommand::Clear { global } => {
                assert_eq!(global, expected_global);
            },
            _ => panic!("Expected Clear subcommand"),
        });
    }

    #[test]
    fn test_parse_context_show() {
        assert_context_command("/context show", |subcommand| match subcommand {
            ContextSubcommand::Show { expand } => {
                assert!(!expand);
            },
            _ => panic!("Expected Show subcommand"),
        });
    }

    #[test]
    fn test_parse_context_show_expand() {
        assert_context_command("/context show --expand", |subcommand| match subcommand {
            ContextSubcommand::Show { expand } => {
                assert!(expand);
            },
            _ => panic!("Expected Show subcommand with expand flag"),
        });
    }

    #[test]
    fn test_parse_context_add() {
        assert_context_add_command("/context add path1 path2", false, false, vec!["path1", "path2"]);
    }

    #[test]
    fn test_parse_context_add_global() {
        assert_context_add_command("/context add --global path1 path2", true, false, vec!["path1", "path2"]);
    }

    #[test]
    fn test_parse_context_add_force() {
        assert_context_add_command("/context add --force path1 path2", false, true, vec!["path1", "path2"]);
    }

    #[test]
    fn test_parse_context_add_global_force() {
        assert_context_add_command("/context add --global --force path1 path2", true, true, vec![
            "path1", "path2",
        ]);
    }

    #[test]
    fn test_parse_context_add_short_force() {
        assert_context_add_command("/context add -f path1 path2", false, true, vec!["path1", "path2"]);
    }

    #[test]
    fn test_parse_context_rm() {
        assert_context_remove_command("/context rm path1 path2", false, vec!["path1", "path2"]);
    }

    #[test]
    fn test_parse_context_rm_global() {
        assert_context_remove_command("/context rm --global path1 path2", true, vec!["path1", "path2"]);
    }

    #[test]
    fn test_parse_context_profile() {
        assert_context_profile_command("/context profile", None, None, None);
    }

    #[test]
    fn test_parse_context_profile_create() {
        assert_context_profile_command("/context profile --create my-profile", None, Some("my-profile"), None);
    }

    #[test]
    fn test_parse_context_profile_delete() {
        assert_context_profile_command("/context profile --delete my-profile", Some("my-profile"), None, None);
    }

    #[test]
    fn test_parse_context_profile_rename() {
        assert_context_profile_command(
            "/context profile --rename old-profile new-profile",
            None,
            None,
            Some(("old-profile", "new-profile")),
        );
    }

    #[test]
    fn test_parse_context_profile_rename_short_flag() {
        assert_context_profile_command(
            "/context profile -r old-profile new-profile",
            None,
            None,
            Some(("old-profile", "new-profile")),
        );
    }

    #[test]
    fn test_parse_context_switch() {
        assert_context_switch_command("/context switch my-profile", "my-profile", false);
    }

    #[test]
    fn test_parse_context_switch_create() {
        assert_context_switch_command("/context switch --create my-profile", "my-profile", true);
    }

    #[test]
    fn test_parse_context_clear() {
        assert_context_clear_command("/context clear", false);
    }

    #[test]
    fn test_parse_context_clear_global() {
        assert_context_clear_command("/context clear --global", true);
    }

    #[test]
    fn test_parse_context_profile_rename_error_missing_names() {
        assert_parse_error(
            "/context profile --rename",
            "Missing profile names for --rename. Usage: --rename <old_name> <new_name>",
        );
    }

    #[test]
    fn test_parse_context_profile_rename_error_missing_new_name() {
        assert_parse_error(
            "/context profile --rename old-profile",
            "Missing profile names for --rename. Usage: --rename <old_name> <new_name>",
        );
    }

    #[test]
    fn test_parse_context_profile_multiple_operations() {
        assert_parse_error(
            "/context profile --create new-profile --rename old-profile new-profile",
            "Only one of --delete, --create, or --rename can be specified",
        );
    }
}
