use clap::Subcommand;
use crossterm::style::{
    self,
    Stylize,
};
use crossterm::{
    cursor,
    execute,
    queue,
    terminal,
};
use dialoguer::FuzzySelect;
use eyre::Result;
use spinners::{
    Spinner,
    Spinners,
};

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

    /// Display current to-do list
    Show,
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
                    Err(_) => return Err(ChatError::Custom("Could not get all to-do lists".into())),
                };
                let mut cleared_one = false;
                for (id, value) in entries.iter() {
                    let temp_struct = match value.as_str() {
                        Some(s) => match serde_json::from_str::<TodoState>(s) {
                            Ok(state) => state,
                            Err(_) => continue,
                        },
                        None => continue,
                    };
                    if temp_struct.completed.iter().all(|b| *b) {
                        match os.database.delete_todo(id) {
                            Ok(_) => cleared_one = true,
                            Err(_) => return Err(ChatError::Custom("Could not delete to-do list".into())),
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
                            // Create spinner for long wait
                            // Can't use with_spinner because of mutable references?? bchm
                            execute!(session.stderr, cursor::Hide)?;
                            let spinner = if session.interactive {
                                Some(Spinner::new(
                                    Spinners::Dots,
                                    format!("{} {}", "Resuming:".magenta(), entries[index].description.clone()),
                                ))
                            } else {
                                None
                            };

                            let todo_result = session.resume_todo(os, &entries[index].id).await;

                            // Remove spinner
                            if let Some(mut s) = spinner {
                                s.stop();
                                queue!(
                                    session.stderr,
                                    terminal::Clear(terminal::ClearType::CurrentLine),
                                    cursor::MoveToColumn(0),
                                    style::Print(format!(
                                        "{} {}\n",
                                        "⟳ Resuming:".magenta(),
                                        entries[index].description.clone()
                                    )),
                                    cursor::Show,
                                    style::SetForegroundColor(style::Color::Reset)
                                )?;
                            }

                            if let Err(e) = todo_result {
                                return Err(ChatError::Custom(format!("Could not resume todo list: {e}").into()));
                            }
                        }
                    }
                },
                Err(_) => return Err(ChatError::Custom("Could not show to-do lists".into())),
            },
            Self::View => match Self::get_descriptions_and_statuses(os) {
                Ok(entries) => {
                    if entries.is_empty() {
                        execute!(session.stderr, style::Print("No to-do lists to view!\n"))?;
                    } else if let Some(index) = fuzzy_select_todos(&entries, "Select a to-do list to view:") {
                        if index < entries.len() {
                            let list = match TodoState::load(os, &entries[index].id) {
                                Ok(list) => list,
                                Err(_) => {
                                    return Err(ChatError::Custom("Could not load the selected to-do list".into()));
                                },
                            };
                            execute!(
                                session.stderr,
                                style::Print(format!(
                                    "{} {}\n",
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
                            match os.database.delete_todo(&entries[index].id) {
                                Ok(_) => {},
                                Err(_) => {
                                    return Err(ChatError::Custom("Could not delete the selected to-do list".into()));
                                },
                            };
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
            Self::Show => {
                if let Some(id) = TodoState::get_current_todo_id(os).unwrap_or(None) {
                    let state = match TodoState::load(os, &id) {
                        Ok(s) => s,
                        Err(_) => {
                            return Err(ChatError::Custom("Could not load current to-do list".into()));
                        },
                    };
                    match state.display_list(&mut session.stderr) {
                        Ok(_) => execute!(session.stderr, style::Print("\n"))?,
                        Err(_) => {
                            return Err(ChatError::Custom("Could not display current to-do list".into()));
                        },
                    };
                } else {
                    execute!(session.stderr, style::Print("No to-do list currently loaded\n"))?;
                }
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
