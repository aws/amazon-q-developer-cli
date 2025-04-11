use std::io::{BufReader, Cursor, Write, stdout};

use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use eyre::{Result, eyre};
use skim::prelude::*;
use tempfile::NamedTempFile;

/// Load tool names from the tool_index.json file
fn load_tool_names() -> Result<Vec<String>> {
    let tool_specs = super::load_tools()?;
    let tool_names: Vec<String> = tool_specs.values().map(|spec| spec.name.clone()).collect();
    Ok(tool_names)
}

pub fn get_available_commands() -> Vec<String> {
    // Import the COMMANDS array directly from prompt.rs
    // This is the single source of truth for available commands
    let commands_array = super::prompt::COMMANDS;

    let mut commands = Vec::new();
    for &cmd in commands_array {
        commands.push(cmd.to_string());
    }

    commands
}

/// Format commands for skim display
/// Create a standard set of skim options with consistent styling
fn create_skim_options(prompt: &str, multi: bool) -> Result<SkimOptions<'_>> {
    SkimOptionsBuilder::default()
        .height(Some("100%"))
        .prompt(Some(prompt))
        .reverse(true)
        .multi(multi)
        .color(Some("fg:252,bg:234,hl:67,fg+:252,bg+:235,hl+:81"))
        .color(Some("info:144,prompt:161,spinner:135,pointer:135,marker:118"))
        .build()
        .map_err(|e| eyre!("Failed to build skim options: {}", e))
}

/// Run skim with the given options and items in an alternate screen
/// This helper function handles entering/exiting the alternate screen and running skim
fn run_skim_with_options(options: &SkimOptions<'_>, items: SkimItemReceiver) -> Result<Option<Vec<Arc<dyn SkimItem>>>> {
    // Enter alternate screen to prevent skim output from persisting in terminal history
    execute!(stdout(), EnterAlternateScreen).map_err(|e| eyre!("Failed to enter alternate screen: {}", e))?;

    let selected_items = Skim::run_with(options, Some(items))
        .map(|out| if out.is_abort { None } else { Some(out.selected_items) })
        .unwrap_or(None);

    execute!(stdout(), LeaveAlternateScreen).map_err(|e| eyre!("Failed to leave alternate screen: {}", e))?;

    Ok(selected_items)
}

/// Extract string selections from skim items
fn extract_selections(items: Vec<Arc<dyn SkimItem>>) -> Vec<String> {
    items.iter().map(|item| item.output().to_string()).collect()
}

/// Launch skim with the given items and return the selected item
pub fn launch_skim_selector(items: &[String], prompt: &str, multi: bool) -> Result<Option<Vec<String>>> {
    let mut temp_file_for_skim_input = NamedTempFile::new()?;
    temp_file_for_skim_input.write_all(items.join("\n").as_bytes())?;

    let options = create_skim_options(prompt, multi)?;
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(BufReader::new(std::fs::File::open(temp_file_for_skim_input.path())?));

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
    // Create skim options with appropriate settings
    let options = create_skim_options("Select files: ", true)?;

    // Create a command that will be executed by skim
    // This avoids loading all files into memory at once
    let find_cmd = "find . -type f -not -path '*/\\.*'";

    // Create a command collector that will execute the find command
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(BufReader::new(
        std::process::Command::new("sh")
            .args(&["-c", find_cmd])
            .stdout(std::process::Stdio::piped())
            .spawn()?
            .stdout
            .ok_or_else(|| eyre!("Failed to get stdout from command"))?,
    ));

    // Run skim with the command output as a stream
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

    match launch_skim_selector(&commands, "Select command: ", false)? {
        Some(selections) if !selections.is_empty() => {
            let selected_command = &selections[0];

            match CommandType::from_str(selected_command) {
                Some(CommandType::ContextAdd(cmd)) => {
                    // For context add commands, we need to select files
                    match select_files_with_skim()? {
                        Some(files) if !files.is_empty() => {
                            // Construct the full command with selected files
                            let mut cmd = cmd.to_string();
                            for file in files {
                                cmd.push_str(&format!(" {}", file));
                            }
                            Ok(Some(cmd))
                        },
                        _ => Ok(Some(selected_command.clone())), /* User cancelled file selection, return just the
                                                                  * command */
                    }
                },
                Some(CommandType::Tools(_)) => {
                    // For tools trust/untrust, we need to select a tool
                    // Load tool names from the tool_index.json file
                    let tools = load_tool_names()?;

                    // Create skim options for tool selection
                    let options = create_skim_options("Select tool: ", false)?;

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
                        None => Ok(Some(selected_command.clone())), /* User cancelled tool selection, return just the
                                                                     * command */
                    }
                },
                Some(CommandType::Profile(_)) => {
                    // For profile operations, we'd need to prompt for the name
                    // For now, just return the command and let the user type the name
                    Ok(Some(selected_command.clone()))
                },
                None => {
                    // Command doesn't need additional parameters
                    Ok(Some(selected_command.clone()))
                },
            }
        },
        _ => Ok(None), // User cancelled command selection
    }
}

#[derive(PartialEq)]
enum CommandType {
    ContextAdd(String),
    Tools(&'static str),
    Profile(&'static str),
}

impl CommandType {
    fn from_str(cmd: &str) -> Option<CommandType> {
        if cmd.starts_with("/context add") {
            Some(CommandType::ContextAdd(cmd.to_string()))
        } else {
            match cmd {
                "/tools trust" => Some(CommandType::Tools("trust")),
                "/tools untrust" => Some(CommandType::Tools("untrust")),
                "/profile set" => Some(CommandType::Profile("set")),
                "/profile delete" => Some(CommandType::Profile("delete")),
                "/profile rename" => Some(CommandType::Profile("rename")),
                "/profile create" => Some(CommandType::Profile("create")),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Test to verify that all hardcoded command strings in select_command
    /// are present in the COMMANDS array from prompt.rs
    #[test]
    fn test_hardcoded_commands_in_commands_array() {
        // Get the set of available commands from prompt.rs
        let available_commands: HashSet<String> = get_available_commands().iter().map(|cmd| cmd.clone()).collect();

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

            // This should assert that all the commands we assert are present in the match statement of
            // select_command()
            assert!(
                CommandType::from_str(cmd).is_some(),
                "Command '{}' cannot be parsed into a CommandType",
                cmd
            );
        }
    }
}
