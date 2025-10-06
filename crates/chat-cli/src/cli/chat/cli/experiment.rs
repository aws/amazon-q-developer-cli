// ABOUTME: Implements the /experiment slash command for toggling experimental features
// ABOUTME: Provides interactive selection interface similar to /model command

use clap::Args;
use crossterm::style::{
    self,
    Color,
};
use crossterm::{
    execute,
    queue,
};
use dialoguer::Select;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::cli::experiment::experiment_manager::ExperimentManager;
use crate::os::Os;

#[derive(Debug, PartialEq, Args)]
pub struct ExperimentArgs;
impl ExperimentArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        Ok(select_experiment(os, session).await?.unwrap_or(ChatState::PromptUser {
            skip_printing_tools: false,
        }))
    }
}

async fn select_experiment(os: &mut Os, session: &mut ChatSession) -> Result<Option<ChatState>, ChatError> {
    // Get current experiment status
    let mut experiment_labels = Vec::new();
    let mut current_states = Vec::new();
    let experiments = ExperimentManager::get_experiments();

    for experiment in &experiments {
        let is_enabled = ExperimentManager::is_enabled(os, experiment.experiment_name);

        current_states.push(is_enabled);

        let status_indicator = if is_enabled {
            format!("{}", style::Stylize::green("[ON] "))
        } else {
            format!("{}", style::Stylize::grey("[OFF]"))
        };

        // Handle multi-line descriptions with proper indentation
        let description = experiment.description.replace('\n', &format!("\n{}", " ".repeat(34)));

        let label = format!(
            "{:<25} {:<6} {}",
            experiment.experiment_name.as_str(),
            status_indicator,
            style::Stylize::dark_grey(description)
        );
        experiment_labels.push(label);
    }

    // Show disclaimer before selection
    queue!(
        session.stderr,
        style::SetForegroundColor(Color::Yellow),
        style::Print("⚠ Experimental features may be changed or removed at any time\n\n"),
        style::ResetColor,
    )?;

    let selection: Option<_> = match Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt("Select an experiment to toggle")
        .items(&experiment_labels)
        .default(0)
        .interact_on_opt(&dialoguer::console::Term::stdout())
    {
        Ok(sel) => {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::style::SetForegroundColor(crossterm::style::Color::Magenta)
            );
            sel
        },
        // Ctrl‑C -> Err(Interrupted)
        Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => {
            // Move to beginning of line and clear everything from warning message down
            queue!(
                session.stderr,
                crossterm::cursor::MoveToColumn(0),
                crossterm::cursor::MoveUp(experiment_labels.len() as u16 + 3),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown),
            )?;
            return Ok(None);
        },
        Err(e) => return Err(ChatError::Custom(format!("Failed to choose experiment: {e}").into())),
    };

    queue!(session.stderr, style::ResetColor)?;

    if let Some(index) = selection {
        // Clear the dialoguer selection line and disclaimer
        queue!(
            session.stderr,
            crossterm::cursor::MoveUp(3), // Move up past selection + 2 disclaimer lines
            crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown),
        )?;

        // Skip if user selected disclaimer or empty line (last 2 items)
        if index >= experiments.len() {
            return Ok(Some(ChatState::PromptUser {
                skip_printing_tools: false,
            }));
        }

        let experiment = &experiments[index];
        let current_state = current_states[index];
        let new_state = !current_state;

        // Update the setting using ExperimentManager
        ExperimentManager::set_enabled(os, experiment.experiment_name, new_state, session).await?;

        let status_text = if new_state { "enabled" } else { "disabled" };

        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetForegroundColor(Color::Green),
            style::Print(format!(
                " {} experiment {}\n\n",
                experiment.experiment_name.as_str(),
                status_text
            )),
            style::ResetColor,
            style::SetForegroundColor(Color::Reset),
            style::SetBackgroundColor(Color::Reset),
        )?;
    } else {
        // ESC was pressed - clear the warning message
        queue!(
            session.stderr,
            crossterm::cursor::MoveUp(3), // Move up past selection + 2 disclaimer lines
            crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown),
        )?;
    }

    execute!(session.stderr, style::ResetColor)?;

    Ok(Some(ChatState::PromptUser {
        skip_printing_tools: false,
    }))
}
