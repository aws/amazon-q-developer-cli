use std::io::{Write, BufReader};
use eyre::{Result, eyre};
use tempfile::NamedTempFile;
use skim::prelude::*;
use crossterm::{
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
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
    // Import the COMMANDS array directly from prompt.rs
    // This is the single source of truth for available commands
    let commands_array = super::prompt::COMMANDS;
    
    // Create CommandInfo objects from the COMMANDS array
    let mut commands = Vec::new();
    for &cmd in commands_array {
        commands.push(CommandInfo {
            command: cmd.to_string(),
            description: "".to_string(), // Empty description since we're just focusing on commands
        });
    }
    
    commands
}

/// Format commands for skim display
fn format_commands_for_skim(commands: &[CommandInfo]) -> Vec<String> {
    commands
        .iter()
        .map(|cmd| format!("{:<30} {}", cmd.command, cmd.description))
        .collect()
}

/// Enter alternate screen mode to prevent skim output from persisting in terminal history
fn enter_alternate_screen() -> Result<()> {
    execute!(
        stdout(),
        EnterAlternateScreen
    ).map_err(|e| eyre!("Failed to enter alternate screen: {}", e))?;
    
    Ok(())
}

/// Leave alternate screen mode and restore the terminal
fn leave_alternate_screen() -> Result<()> {
    execute!(
        stdout(),
        LeaveAlternateScreen
    ).map_err(|e| eyre!("Failed to leave alternate screen: {}", e))?;
    
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
    
    // Enter alternate screen to prevent skim output from persisting in terminal history
    enter_alternate_screen()?;
    
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
    
    // Leave alternate screen
    leave_alternate_screen()?;
    
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
    
    // Create item reader
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(std::io::Cursor::new(files));
    
    // Enter alternate screen to prevent skim output from persisting in terminal history
    enter_alternate_screen()?;
    
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
    
    // Leave alternate screen
    leave_alternate_screen()?;
    
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

// Removed context file retrieval functions as they were not working correctly

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
                
                // Create item reader
                let item_reader = SkimItemReader::default();
                let items = item_reader.of_bufread(std::io::Cursor::new(tools.join("\n")));
                
                // Enter alternate screen to prevent skim output from persisting in terminal history
                enter_alternate_screen()?;
                
                // Run skim
                let selected_tool = Skim::run_with(&options, Some(items))
                    .map(|out| {
                        if out.is_abort || out.selected_items.is_empty() {
                            None
                        } else {
                            Some(out.selected_items[0].output().to_string())
                        }
                    })
                    .unwrap_or(None);
                
                // Leave alternate screen
                leave_alternate_screen()?;
                
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
