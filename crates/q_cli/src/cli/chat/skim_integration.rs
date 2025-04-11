use std::io::{Write, BufReader};
use std::path::Path;
use eyre::{Result, eyre};
use tempfile::NamedTempFile;
use skim::prelude::*;
use crossterm::{
    terminal::{Clear, ClearType},
    cursor::{MoveTo},
    execute,
};
use std::io::stdout;

/// Represents a command with its description
#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub command: String,
    pub description: String,
}

/// Get all available commands with their descriptions
pub fn get_available_commands() -> Vec<CommandInfo> {
    vec![
        CommandInfo { command: "/help".to_string(), description: "Show the help dialogue".to_string() },
        CommandInfo { command: "/clear".to_string(), description: "Clear the conversation history".to_string() },
        CommandInfo { command: "/quit".to_string(), description: "Quit the application".to_string() },
        CommandInfo { command: "/issue".to_string(), description: "Report an issue or make a feature request".to_string() },
        CommandInfo { command: "/editor".to_string(), description: "Open editor to compose a prompt".to_string() },
        CommandInfo { command: "/compact".to_string(), description: "Summarize conversation to free up context space".to_string() },
        CommandInfo { command: "/compact --summary".to_string(), description: "Summarize and display the summary".to_string() },
        CommandInfo { command: "/compact help".to_string(), description: "Show help for the compact command".to_string() },
        
        // Tools commands
        CommandInfo { command: "/tools".to_string(), description: "View and manage tools and permissions".to_string() },
        CommandInfo { command: "/tools help".to_string(), description: "Show an explanation for the tools command".to_string() },
        CommandInfo { command: "/tools trust".to_string(), description: "Trust a specific tool for the session".to_string() },
        CommandInfo { command: "/tools untrust".to_string(), description: "Revert a tool to per-request confirmation".to_string() },
        CommandInfo { command: "/tools trustall".to_string(), description: "Trust all tools (no confirmation required)".to_string() },
        CommandInfo { command: "/tools reset".to_string(), description: "Reset all tools to default permission levels".to_string() },
        
        // Profile commands
        CommandInfo { command: "/profile".to_string(), description: "Manage profiles".to_string() },
        CommandInfo { command: "/profile help".to_string(), description: "Show profile help".to_string() },
        CommandInfo { command: "/profile list".to_string(), description: "List profiles".to_string() },
        CommandInfo { command: "/profile create".to_string(), description: "Create a new profile".to_string() },
        CommandInfo { command: "/profile delete".to_string(), description: "Delete a profile".to_string() },
        CommandInfo { command: "/profile rename".to_string(), description: "Rename a profile".to_string() },
        CommandInfo { command: "/profile set".to_string(), description: "Set the current profile".to_string() },
        
        // Context commands
        CommandInfo { command: "/context".to_string(), description: "Manage context files for the chat session".to_string() },
        CommandInfo { command: "/context help".to_string(), description: "Show context help".to_string() },
        CommandInfo { command: "/context show".to_string(), description: "Display current context rules configuration".to_string() },
        CommandInfo { command: "/context show --expand".to_string(), description: "Display context rules with file contents".to_string() },
        CommandInfo { command: "/context add".to_string(), description: "Add file(s) to context".to_string() },
        CommandInfo { command: "/context add --global".to_string(), description: "Add file(s) to global context".to_string() },
        CommandInfo { command: "/context add --force".to_string(), description: "Add file(s) to context, ignoring size limits".to_string() },
        CommandInfo { command: "/context rm".to_string(), description: "Remove file(s) from context".to_string() },
        CommandInfo { command: "/context rm --global".to_string(), description: "Remove file(s) from global context".to_string() },
        CommandInfo { command: "/context clear".to_string(), description: "Clear all files from current context".to_string() },
        CommandInfo { command: "/context clear --global".to_string(), description: "Clear all files from global context".to_string() },
    ]
}

/// Format commands for skim display
fn format_commands_for_skim(commands: &[CommandInfo]) -> Vec<String> {
    commands
        .iter()
        .map(|cmd| format!("{:<30} {}", cmd.command, cmd.description))
        .collect()
}

/// Clear the terminal screen after skim closes
fn clear_screen_after_skim() -> Result<()> {
    // Use crossterm to clear the screen and reset cursor position
    execute!(
        stdout(),
        Clear(ClearType::All),
        MoveTo(0, 0)
    ).map_err(|e| eyre!("Failed to clear screen: {}", e))?;
    
    Ok(())
}

/// Launch skim with the given items and return the selected item
pub fn launch_skim_selector(items: &[String], prompt: &str, multi: bool) -> Result<Option<Vec<String>>> {
    // Create a temporary file for skim input
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(items.join("\n").as_bytes())?;
    
    // Build skim options
    let options = SkimOptionsBuilder::default()
        .height(Some("40%"))
        .prompt(Some(prompt))
        .reverse(true)
        .multi(multi)
        .color(Some("fg:252,bg:234,hl:67,fg+:252,bg+:235,hl+:81"))
        .color(Some("info:144,prompt:161,spinner:135,pointer:135,marker:118"))
        .build()
        .map_err(|e| eyre!("Failed to build skim options: {}", e))?;
    
    // Create item reader
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(BufReader::new(std::fs::File::open(temp_file.path())?));
    
    // Run skim
    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| {
            if out.is_abort {
                None
            } else {
                Some(out.selected_items)
            }
        })
        .unwrap_or(None);
    
    // Clear the screen after skim closes
    clear_screen_after_skim()?;
    
    // Parse the output
    match selected_items {
        Some(items) if !items.is_empty() => {
            let selections: Vec<String> = items
                .iter()
                .map(|item| {
                    // Extract the command part (everything before the description)
                    let line = item.output();
                    let parts: Vec<&str> = line.splitn(2, "  ").collect();
                    parts[0].trim().to_string()
                })
                .collect();
            
            Ok(Some(selections))
        },
        _ => Ok(None), // User cancelled or no selection
    }
}

/// Select files using skim
pub fn select_files_with_skim() -> Result<Option<Vec<String>>> {
    // Use find to get a list of files
    let find_output = std::process::Command::new("find")
        .args(&[".", "-type", "f", "-not", "-path", "*/\\.*"])
        .output()?;
    
    if !find_output.status.success() {
        return Err(eyre!("Failed to list files"));
    }
    
    let files = String::from_utf8(find_output.stdout)?;
    let file_list: Vec<String> = files.lines().map(|s| s.to_string()).collect();
    
    if file_list.is_empty() {
        return Ok(None);
    }
    
    // Create skim options
    let options = SkimOptionsBuilder::default()
        .height(Some("40%"))
        .multi(true)
        .reverse(true)
        .prompt(Some("Select files: "))
        .build()
        .map_err(|e| eyre!("Failed to build skim options: {}", e))?;
    
    // Create item reader and run skim
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(std::io::Cursor::new(files));
    
    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| {
            if out.is_abort {
                None
            } else {
                Some(out.selected_items)
            }
        })
        .unwrap_or(None);
    
    // Clear the screen after skim closes
    clear_screen_after_skim()?;
    
    // Parse the output
    match selected_items {
        Some(items) if !items.is_empty() => {
            let selections: Vec<String> = items
                .iter()
                .map(|item| item.output().to_string())
                .collect();
            
            Ok(Some(selections))
        },
        _ => Ok(None), // User cancelled or no selection
    }
}

/// Get the current context files from the context config file
pub fn get_context_files(global: bool) -> Result<Vec<String>> {
    // Get the context config file path
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let config_dir = Path::new(&home_dir).join(".q").join("context");
    
    let config_file = if global {
        config_dir.join("global.json")
    } else {
        // Get the current profile
        let profile_file = config_dir.join("current_profile");
        let profile = if profile_file.exists() {
            std::fs::read_to_string(profile_file)?
        } else {
            "default".to_string()
        };
        
        config_dir.join(format!("{}.json", profile))
    };
    
    // Check if the config file exists
    if !config_file.exists() {
        return Ok(Vec::new());
    }
    
    // Read the config file
    let config_content = std::fs::read_to_string(config_file)?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;
    
    // Extract the paths
    let paths = match config.get("paths") {
        Some(serde_json::Value::Array(paths)) => {
            paths.iter()
                .filter_map(|p| p.as_str().map(|s| s.to_string()))
                .collect()
        },
        _ => Vec::new(),
    };
    
    Ok(paths)
}

/// Select context files to remove using skim
pub fn select_context_files_to_remove(global: bool) -> Result<Option<Vec<String>>> {
    // Get the current context files
    let context_files = get_context_files(global)?;
    
    if context_files.is_empty() {
        return Ok(None);
    }
    
    // Set prompt based on context type
    let prompt = if global {
        "Select global context files to remove: "
    } else {
        "Select context files to remove: "
    };
    
    // Create skim options
    let options = SkimOptionsBuilder::default()
        .height(Some("40%"))
        .multi(true)
        .reverse(true)
        .prompt(Some(prompt))
        .build()
        .map_err(|e| eyre!("Failed to build skim options: {}", e))?;
    
    // Create item reader and run skim
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(std::io::Cursor::new(context_files.join("\n")));
    
    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| {
            if out.is_abort {
                None
            } else {
                Some(out.selected_items)
            }
        })
        .unwrap_or(None);
    
    // Clear the screen after skim closes
    clear_screen_after_skim()?;
    
    // Parse the output
    match selected_items {
        Some(items) if !items.is_empty() => {
            let selections: Vec<String> = items
                .iter()
                .map(|item| item.output().to_string())
                .collect();
            
            Ok(Some(selections))
        },
        _ => Ok(None), // User cancelled or no selection
    }
}

/// Launch the command selector and handle the selected command
pub fn select_command() -> Result<Option<String>> {
    let commands = get_available_commands();
    let formatted_commands = format_commands_for_skim(&commands);
    
    match launch_skim_selector(&formatted_commands, "Select command: ", false)? {
        Some(selections) if !selections.is_empty() => {
            let selected_command = &selections[0];
            
            // Check if the command needs parameters
            if selected_command == "/context add" {
                // For context add, we need to select files
                match select_files_with_skim()? {
                    Some(files) if !files.is_empty() => {
                        // Construct the full command with selected files
                        let mut cmd = selected_command.clone();
                        for file in files {
                            cmd.push_str(&format!(" {}", file));
                        }
                        Ok(Some(cmd))
                    },
                    _ => Ok(Some(selected_command.clone())), // User cancelled file selection, return just the command
                }
            } else if selected_command == "/context add --global" || selected_command == "/context add --force" {
                // For context add with flags, we need to select files
                match select_files_with_skim()? {
                    Some(files) if !files.is_empty() => {
                        // Construct the full command with selected files
                        let mut cmd = selected_command.clone();
                        for file in files {
                            cmd.push_str(&format!(" {}", file));
                        }
                        Ok(Some(cmd))
                    },
                    _ => Ok(Some(selected_command.clone())), // User cancelled file selection, return just the command
                }
            } else if selected_command == "/context rm" {
                // For context rm, we need to select context files to remove
                match select_context_files_to_remove(false)? {
                    Some(files) if !files.is_empty() => {
                        // Construct the full command with selected files
                        let mut cmd = selected_command.clone();
                        for file in files {
                            cmd.push_str(&format!(" {}", file));
                        }
                        Ok(Some(cmd))
                    },
                    _ => Ok(Some(selected_command.clone())), // User cancelled file selection, return just the command
                }
            } else if selected_command == "/context rm --global" {
                // For context rm --global, we need to select global context files to remove
                match select_context_files_to_remove(true)? {
                    Some(files) if !files.is_empty() => {
                        // Construct the full command with selected files
                        let mut cmd = selected_command.clone();
                        for file in files {
                            cmd.push_str(&format!(" {}", file));
                        }
                        Ok(Some(cmd))
                    },
                    _ => Ok(Some(selected_command.clone())), // User cancelled file selection, return just the command
                }
            } else if selected_command == "/tools trust" || selected_command == "/tools untrust" {
                // For tools trust/untrust, we need to select a tool
                let tools = vec![
                    "fs_read".to_string(),
                    "fs_write".to_string(),
                    "execute_bash".to_string(),
                    "use_aws".to_string(),
                    "report_issue".to_string(),
                ];
                
                // Create skim options for tool selection
                let options = SkimOptionsBuilder::default()
                    .height(Some("40%"))
                    .reverse(true)
                    .prompt(Some("Select tool: "))
                    .build()
                    .map_err(|e| eyre!("Failed to build skim options: {}", e))?;
                
                // Create item reader and run skim
                let item_reader = SkimItemReader::default();
                let items = item_reader.of_bufread(std::io::Cursor::new(tools.join("\n")));
                
                let selected_tool = Skim::run_with(&options, Some(items))
                    .map(|out| {
                        if out.is_abort || out.selected_items.is_empty() {
                            None
                        } else {
                            Some(out.selected_items[0].output().to_string())
                        }
                    })
                    .unwrap_or(None);
                
                // Clear the screen after skim closes
                clear_screen_after_skim()?;
                
                match selected_tool {
                    Some(tool) => Ok(Some(format!("{} {}", selected_command, tool))),
                    None => Ok(Some(selected_command.clone())), // User cancelled tool selection, return just the command
                }
            } else if selected_command == "/profile set" || selected_command == "/profile delete" || 
                      selected_command == "/profile rename" || selected_command == "/profile create" {
                // For profile operations, we'd need to prompt for the name
                // For now, just return the command and let the user type the name
                Ok(Some(selected_command.clone()))
            } else {
                // Command doesn't need additional parameters
                Ok(Some(selected_command.clone()))
            }
        },
        _ => Ok(None), // User cancelled command selection
    }
}
