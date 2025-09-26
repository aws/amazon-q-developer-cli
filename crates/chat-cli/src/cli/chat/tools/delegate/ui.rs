use std::io::{
    Write,
    stdin,
    stdout,
};

use crossterm::execute;
use crossterm::style::{
    Color,
    Print,
    SetForegroundColor,
};
use eyre::Result;

use crate::cli::chat::tools::delegate::types::AgentConfig;

pub fn display_agent_info(agent: &str, task: &str, config: &AgentConfig) -> Result<()> {
    let short_desc = truncate_description(config.description.as_deref().unwrap_or("No description"));

    execute!(
        stdout(),
        Print(format!("Agent: {}\n", agent)),
        Print(format!("Description: {}\n", short_desc)),
        Print(format!("Task: {}\n", task)),
    )?;

    if !config.allowed_tools.is_empty() {
        let tools: Vec<&str> = config.allowed_tools.iter().map(|s| s.as_str()).collect();
        execute!(stdout(), Print(format!("Tools: {}\n", tools.join(", "))))?;
    }

    // Add appropriate security warning based on agent permissions
    execute!(
        stdout(),
        Print("\n"),
        SetForegroundColor(Color::Yellow),
        Print("! This task will run with the agent's specific tool permissions.\n\n"),
        SetForegroundColor(Color::Reset),
    )?;

    Ok(())
}

pub fn truncate_description(desc: &str) -> &str {
    if let Some(pos) = desc.find('.') {
        &desc[..pos + 1]
    } else if desc.len() > 60 {
        &desc[..57]
    } else {
        desc
    }
}

pub fn display_default_agent_warning() -> Result<()> {
    execute!(
        stdout(),
        Print("\n"),
        SetForegroundColor(Color::Yellow),
        Print(
            "! This task will run with trust-all permissions and can execute commands or consume system/cloud resources.\n\n"
        ),
        SetForegroundColor(Color::Reset),
    )?;
    Ok(())
}

pub fn get_user_confirmation() -> Result<bool> {
    execute!(
        stdout(),
        SetForegroundColor(Color::Yellow),
        Print("Continue? [y/N]: "),
        SetForegroundColor(Color::Reset),
    )?;

    stdout().flush()?;

    let mut input = String::new();
    stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input == "y" || input == "yes" {
        println!();
        Ok(true)
    } else {
        Ok(false)
    }
}
