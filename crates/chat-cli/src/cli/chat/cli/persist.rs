use clap::Subcommand;
use crossterm::execute;
use crossterm::style::{
    self,
    Attribute,
    Color,
};

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
pub enum PersistSubcommand {
    /// Save the current conversation
    Save {
        path: String,
        #[arg(short, long)]
        force: bool,
    },
    /// Load a previous conversation
    Load { path: String },
}

impl PersistSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        macro_rules! tri {
            ($v:expr, $name:expr, $path:expr) => {
                match $v {
                    Ok(v) => v,
                    Err(err) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("\nFailed to {} {}: {}\n\n", $name, $path, &err)),
                            style::SetAttribute(Attribute::Reset)
                        )?;

                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    },
                }
            };
        }

        match self {
            Self::Save { path, force } => {
                let contents = tri!(serde_json::to_string_pretty(&session.conversation), "export to", &path);
                if os.fs.exists(&path) && !force {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!(
                            "\nFile at {} already exists. To overwrite, use -f or --force\n\n",
                            &path
                        )),
                        style::SetAttribute(Attribute::Reset)
                    )?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }
                tri!(os.fs.write(&path, contents).await, "export to", &path);

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("\n✔ Exported conversation state to {}\n\n", &path)),
                    style::SetAttribute(Attribute::Reset)
                )?;
            },
            Self::Load { path: _ } => {
                // For profile operations that need a profile name, show profile selector
                // As part of the agent implementation, we are disabling the ability to
                // switch profile after a session has started.
                // TODO: perhaps revive this after we have a decision on profile switching
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(
                        "Conversation loading has been disabled. To load a conversation. Quit and restart q chat."
                    ),
                    style::SetAttribute(Attribute::Reset)
                )?;
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
