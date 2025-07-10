use clap::Parser;
use crossterm::execute;
use crossterm::style::{self, Color, Attribute};
use std::path::PathBuf;

use crate::cli::chat::{ChatError, ChatSession, ChatState};
use crate::os::Os;

/// Export the conversation transcript to a markdown file
#[derive(Debug, PartialEq, Parser)]
pub struct ExportArgs {
    #[arg(required = true)]
    pub path: PathBuf,
    #[arg(short, long)]
    pub force: bool,
}

impl ExportArgs {
    pub async fn execute(&self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let transcript = self.get_markdown_transcript(session);
        
        if os.fs.exists(&self.path) && !self.force {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Red),
                style::Print(format!(
                    "\nFile at {} already exists. To overwrite, use -f or --force\n\n",
                    &self.path.display()
                )),
                style::SetAttribute(Attribute::Reset)
            )?;
            
            return Ok(ChatState::PromptUser {
                skip_printing_tools: false,
            });
        }
        
        match os.fs.write(&self.path, transcript).await {
            Ok(_) => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("\n✔ Conversation exported to {}\n\n", self.path.display())),
                    style::SetAttribute(Attribute::Reset)
                )?;
            }
            Err(err) => {
                return Err(ChatError::Custom(
                    format!("Failed to export conversation: {}", err).into(),
                ));
            }
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    }

    fn get_markdown_transcript(&self, session: &ChatSession) -> String {
        let mut markdown = String::from("# Amazon Q Conversation\n\n");
        
        for message in &session.conversation.transcript {
            if message.starts_with("> ") {
                markdown.push_str(&format!("## User\n\n{}\n\n", message));
            } else {
                markdown.push_str(&format!("## Assistant\n\n{}\n\n", message));
            }
        }
        
        markdown
    }
}
