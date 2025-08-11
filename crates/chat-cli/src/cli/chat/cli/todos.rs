use clap::Subcommand;
use crossterm::execute;
use crossterm::style::{
    self,
    Stylize,
};
use dialoguer::FuzzySelect;
use eyre::Result;

use crate::cli::chat::tools::todo::TodoState;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

#[derive(Debug, PartialEq, Subcommand)]
pub enum TodoSubcommand {
    /// Delete all completed to-do lists
    ClearFinished,

    /// Resume a selected to-do list
    Resume,

    /// View a to-do list
    View,

    /// Delete a to-do list
    Delete,
}

/// Used for displaying completed and in-progress todo lists
pub struct TodoDisplayEntry {
    pub num_completed: usize,
    pub num_tasks: usize,
    pub description: String,
    pub id: String,
}

impl std::fmt::Display for TodoDisplayEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.num_completed == self.num_tasks {
            write!(f, "{} {}", "✓".green().bold(), self.description.clone(),)
        } else {
            write!(
                f,
                "{} {} ({}/{})",
                "✗".red().bold(),
                self.description.clone(),
                self.num_completed,
                self.num_tasks
            )
        }
    }
}

impl TodoSubcommand {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self {
            Self::ClearFinished => {
                let entries = match os.database.get_all_todos() {
                    Ok(e) => e,
                    Err(e) => return Err(ChatError::Custom(format!("Could not get all to-do lists: {e}").into())),
                };
                let mut cleared_one = false;

                for (id, value) in entries.iter() {
                    let todo_status = match value.as_str() {
                        Some(s) => match serde_json::from_str::<TodoState>(s) {
                            Ok(state) => state,

                            // FIX: Silent fail
                            Err(_) => continue,
                        },
                        None => continue,
                    };
                    if todo_status.completed.iter().all(|b| *b) {
                        match os.database.delete_todo(id) {
                            Ok(_) => cleared_one = true,
                            Err(e) => {
                                return Err(ChatError::Custom(format!("Could not delete to-do list: {e}").into()));
                            },
                        };
                    }
                }
                if cleared_one {
                    execute!(
                        session.stderr,
                        style::Print("✔ Cleared finished to-do lists!\n".green())
                    )?;
                } else {
                    execute!(session.stderr, style::Print("No finished to-do lists to clear!\n"))?;
                }
            },
            Self::Resume => match Self::get_descriptions_and_statuses(os) {
                Ok(entries) => {
                    if entries.is_empty() {
                        execute!(session.stderr, style::Print("No to-do lists to resume!\n"),)?;
                    } else if let Some(index) = fuzzy_select_todos(&entries, "Select a to-do list to resume:") {
                        if index < entries.len() {
                            execute!(
                                session.stderr,
                                style::Print(format!(
                                    "{} {}",
                                    "⟳ Resuming:".magenta(),
                                    entries[index].description.clone()
                                ))
                            )?;
                            return session.resume_todo_request(os, &entries[index].id).await;
                        }
                    }
                },
                Err(e) => return Err(ChatError::Custom(format!("Could not show to-do lists: {e}").into())),
            },
            Self::View => match Self::get_descriptions_and_statuses(os) {
                Ok(entries) => {
                    if entries.is_empty() {
                        execute!(session.stderr, style::Print("No to-do lists to view!\n"))?;
                    } else if let Some(index) = fuzzy_select_todos(&entries, "Select a to-do list to view:") {
                        if index < entries.len() {
                            let list = TodoState::load(os, &entries[index].id).map_err(|e| {
                                ChatError::Custom(format!("Could not load current to-do list: {e}").into())
                            })?;
                            execute!(
                                session.stderr,
                                style::Print(format!(
                                    "{} {}\n\n",
                                    "Viewing:".magenta(),
                                    entries[index].description.clone()
                                ))
                            )?;
                            if list.display_list(&mut session.stderr).is_err() {
                                return Err(ChatError::Custom("Could not display the selected to-do list".into()));
                            }
                            execute!(session.stderr, style::Print("\n"),)?;
                        }
                    }
                },
                Err(_) => return Err(ChatError::Custom("Could not show to-do lists".into())),
            },
            Self::Delete => match Self::get_descriptions_and_statuses(os) {
                Ok(entries) => {
                    if entries.is_empty() {
                        execute!(session.stderr, style::Print("No to-do lists to delete!\n"))?;
                    } else if let Some(index) = fuzzy_select_todos(&entries, "Select a to-do list to delete:") {
                        if index < entries.len() {
                            os.database.delete_todo(&entries[index].id).map_err(|e| {
                                ChatError::Custom(format!("Could not delete the selected to-do list: {e}").into())
                            })?;
                            execute!(
                                session.stderr,
                                style::Print("✔ Deleted to-do list: ".green()),
                                style::Print(format!("{}\n", entries[index].description.clone().dark_grey()))
                            )?;
                        }
                    }
                },
                Err(_) => return Err(ChatError::Custom("Could not show to-do lists".into())),
            },
        }
        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    /// Convert all to-do list state entries to displayable entries
    fn get_descriptions_and_statuses(os: &Os) -> Result<Vec<TodoDisplayEntry>> {
        let mut out = Vec::new();
        let entries = os.database.get_all_todos()?;
        for (id, value) in entries.iter() {
            let temp_struct = match value.as_str() {
                Some(s) => match serde_json::from_str::<TodoState>(s) {
                    Ok(state) => state,
                    Err(_) => continue,
                },
                None => continue,
            };

            out.push(TodoDisplayEntry {
                num_completed: temp_struct.completed.iter().filter(|b| **b).count(),
                num_tasks: temp_struct.completed.len(),
                description: temp_struct.task_description,
                id: id.clone(),
            });
        }
        Ok(out)
    }
}

fn fuzzy_select_todos(entries: &[TodoDisplayEntry], prompt_str: &str) -> Option<usize> {
    FuzzySelect::new()
        .with_prompt(prompt_str)
        .items(entries)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}
