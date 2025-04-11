use std::io::{Write, BufReader, Cursor};
use eyre::{Result, eyre};
use tempfile::NamedTempFile;
use skim::prelude::*;
use crossterm::{
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use std::io::stdout;
use std::path::Path;
use std::collections::HashMap;
use serde_json::Value;
#[cfg(test)]
use std::collections::HashSet;

/// Load tool names from the tool_index.json file
fn load_tool_names() -> Result<Vec<String>> {
    // Path to the tool_index.json file
    let tool_index_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/cli/chat/tools/tool_index.json");
    
    // Read the file content
    let file_content = std::fs::read_to_string(tool_index_path)?;
    
    // Parse the JSON
    let tool_index: HashMap<String, Value> = serde_json::from_str(&file_content)?;
    
    // Extract tool names
    let tool_names: Vec<String> = tool_index.keys().cloned().collect();
    
    Ok(tool_names)
}

/// Represents a command
#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub command: String,
}

/// Get all available commands
pub fn get_available_commands() -> Vec<CommandInfo> {
    // Import the COMMANDS array directly from prompt.rs
    // This is the single source of truth for available commands
    let commands_array = super::prompt::COMMANDS;
    
    // Create CommandInfo objects from the COMMANDS array
    let mut commands = Vec::new();
    for &cmd in commands_array {
        commands.push(CommandInfo {
            command: cmd.to_string(),
        });
    }
    
    commands
}

/// Format commands for skim display
fn format_commands_for_skim(commands: &[CommandInfo]) -> Vec<String> {
    commands
        .iter()
        .map(|cmd| cmd.command.clone())
        .collect()
}

/// Run skim with the given options and items in an alternate screen
/// This helper function handles entering/exiting the alternate screen and running skim
fn run_skim_with_options(options: &SkimOptions<'_>, items: SkimItemReceiver) -> Result<Option<Vec<Arc<dyn SkimItem>>>> {
    // Enter alternate screen to prevent skim output from persisting in terminal history
    execute!(stdout(), EnterAlternateScreen)
        .map_err(|e| eyre!("Failed to enter alternate screen: {}", e))?;
    
    // Run skim
    let selected_items = Skim::run_with(options, Some(items))
        .map(|out| {
            if out.is_abort {
                None
            } else {
                Some(out.selected_items)
            }
        })
        .unwrap_or(None);
    
    // Leave alternate screen
    execute!(stdout(), LeaveAlternateScreen)
        .map_err(|e| eyre!("Failed to leave alternate screen: {}", e))?;
    
    Ok(selected_items)
}

/// Extract string selections from skim items
fn extract_selections(items: Vec<Arc<dyn SkimItem>>) -> Vec<String> {
    items.iter()
        .map(|item| item.output().to_string())
        .collect()
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
    
    // Run skim and get selected items
    match run_skim_with_options(&options, items)? {
        Some(items) if !items.is_empty() => {
            let selections = extract_selections(items);
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
    let items = item_reader.of_bufread(Cursor::new(files));
    
    // Run skim and get selected items
    match run_skim_with_options(&options, items)? {
        Some(items) if !items.is_empty() => {
            let selections = extract_selections(items);
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
            if selected_command.starts_with("/context add") {
                // For context add commands, we need to select files
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
                // Load tool names from the tool_index.json file
                let tools = load_tool_names()?;
                
                // Create skim options for tool selection
                let options = SkimOptionsBuilder::default()
                    .height(Some("40%"))
                    .reverse(true)
                    .prompt(Some("Select tool: "))
                    .build()
                    .map_err(|e| eyre!("Failed to build skim options: {}", e))?;
                
                // Create item reader
                let item_reader = SkimItemReader::default();
                let items = item_reader.of_bufread(Cursor::new(tools.join("\n")));
                
                // Run skim and get selected tool
                let selected_tool = match run_skim_with_options(&options, items)? {
                    Some(items) if !items.is_empty() => Some(items[0].output().to_string()),
                    _ => None,
                };
                
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
#[cfg(test)]
mod tests {
    use super::*;

    /// Test to verify that all hardcoded command strings in select_command
    /// are present in the COMMANDS array from prompt.rs
    #[test]
    fn test_hardcoded_commands_in_commands_array() {
        // Get the set of available commands from prompt.rs
        let available_commands: HashSet<String> = get_available_commands()
            .iter()
            .map(|cmd| cmd.command.clone())
            .collect();

        // List of hardcoded commands used in select_command
        let hardcoded_commands = vec![
            "/context add",
            "/context add --global",
            "/tools trust",
            "/tools untrust",
            "/profile set",
            "/profile delete",
            "/profile rename",
            "/profile create",
        ];

        // Check that each hardcoded command is in the COMMANDS array
        for cmd in hardcoded_commands {
            assert!(
                available_commands.contains(cmd),
                "Command '{}' is used in select_command but not defined in COMMANDS array",
                cmd
            );
        }
    }
}
