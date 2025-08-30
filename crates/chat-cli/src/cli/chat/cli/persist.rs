use clap::Subcommand;
use crossterm::execute;
use crossterm::style::{
    self,
    Attribute,
    Color,
};

use crate::cli::ConversationState;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

/// Commands for persisting and loading conversation state
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
pub enum PersistSubcommand {
    /// Save the current conversation
    Save {
        /// Path where the conversation will be saved
        path: String,
        #[arg(short, long)]
        /// Force overwrite if file already exists
        force: bool,
    },
    /// Load a previous conversation
    Load {
        /// Path to the conversation file to load
        path: String,
    },
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
            Self::Load { path } => {
                // Try the original path first
                let original_result = os.fs.read_to_string(&path).await;

                // If the original path fails and doesn't end with .json, try with .json appended
                let contents = if original_result.is_err() && !path.ends_with(".json") {
                    let json_path = format!("{}.json", path);
                    match os.fs.read_to_string(&json_path).await {
                        Ok(content) => content,
                        Err(_) => {
                            // If both paths fail, return the original error for better user experience
                            tri!(original_result, "import from", &path)
                        },
                    }
                } else {
                    tri!(original_result, "import from", &path)
                };

                let mut new_state: ConversationState = tri!(serde_json::from_str(&contents), "import from", &path);
                std::mem::swap(&mut new_state.tool_manager, &mut session.conversation.tool_manager);
                std::mem::swap(&mut new_state.mcp_enabled, &mut session.conversation.mcp_enabled);
                std::mem::swap(&mut new_state.model_info, &mut session.conversation.model_info);
                std::mem::swap(
                    &mut new_state.context_manager,
                    &mut session.conversation.context_manager,
                );
                std::mem::swap(&mut new_state.agents, &mut session.conversation.agents);
                session.conversation = new_state;

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("\n✔ Imported conversation state from {}\n\n", &path)),
                    style::SetAttribute(Attribute::Reset)
                )?;
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}
