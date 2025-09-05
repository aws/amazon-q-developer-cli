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

use crate::cli::chat::conversation::format_tool_spec;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;

/// Represents an experimental feature that can be toggled
#[derive(Debug, Clone)]
struct Experiment {
    name: &'static str,
    description: &'static str,
    setting_key: Setting,
}

static AVAILABLE_EXPERIMENTS: &[Experiment] = &[
    Experiment {
        name: "Knowledge",
        description: "Enables persistent context storage and retrieval across chat sessions (/knowledge)",
        setting_key: Setting::EnabledKnowledge,
    },
    Experiment {
        name: "Thinking",
        description: "Enables complex reasoning with step-by-step thought processes",
        setting_key: Setting::EnabledThinking,
    },
    Experiment {
        name: "Tangent Mode",
        description: "Enables entering into a temporary mode for sending isolated conversations (/tangent)",
        setting_key: Setting::EnabledTangentMode,
    },
    Experiment {
        name: "Todo Lists",
        description: "Enables Q to create todo lists that can be viewed and managed using /todos",
        setting_key: Setting::EnabledTodoList,
    },
];

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

    for experiment in AVAILABLE_EXPERIMENTS {
        let is_enabled = os.database.settings.get_bool(experiment.setting_key).unwrap_or(false);

        current_states.push(is_enabled);
        // Create clean single-line format: "Knowledge    [ON]   - Description"
        let status_indicator = if is_enabled {
            style::Stylize::green("[ON] ")
        } else {
            style::Stylize::grey("[OFF]")
        };
        let label = format!(
            "{:<18} {} - {}",
            experiment.name,
            status_indicator,
            style::Stylize::dark_grey(experiment.description)
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
        if index >= AVAILABLE_EXPERIMENTS.len() {
            return Ok(Some(ChatState::PromptUser {
                skip_printing_tools: false,
            }));
        }

        let experiment = &AVAILABLE_EXPERIMENTS[index];
        let current_state = current_states[index];
        let new_state = !current_state;

        // Update the setting
        os.database
            .settings
            .set(experiment.setting_key, new_state)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to update experiment setting: {e}").into()))?;

        // Reload tools to reflect the experiment change
        let tools = session
            .conversation
            .tool_manager
            .load_tools(os, &mut session.stderr)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to update tool spec: {e}").into()))?;

        session.conversation.tools = format_tool_spec(tools);

        let status_text = if new_state { "enabled" } else { "disabled" };

        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetForegroundColor(Color::Green),
            style::Print(format!(" {} experiment {}\n\n", experiment.name, status_text)),
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
