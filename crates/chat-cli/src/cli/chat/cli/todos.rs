use clap::Subcommand;
use crossterm::execute;
use crossterm::style::{
    self,
    Stylize,
};
use dialoguer::Select;
use eyre::Result;

use crate::cli::chat::tools::todo::{
    TodoList,
    TodoListState,
    delete_todo,
    get_all_todos,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

/// Defines subcommands that allow users to view and manage todo lists
#[derive(Debug, PartialEq, Subcommand)]
pub enum TodoSubcommand {
    /// Delete all completed to-do lists
    ClearFinished,

    /// Resume a selected to-do list
    Resume,

    /// View a to-do list
    View,

    /// Delete a to-do list
    Delete {
        #[arg(long, short)]
        all: bool,
    },
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
        // Check if todo lists are enabled
        if !TodoList::is_enabled(os) {
            execute!(
                session.stderr,
                style::SetForegroundColor(style::Color::Red),
                style::Print("Todo lists are disabled. Enable them with: q settings chat.enableTodoList true\n"),
                style::SetForegroundColor(style::Color::Reset)
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }
        TodoListState::init_dir(os)
            .await
            .map_err(|e| ChatError::Custom(format!("Could not create todos directory: {e}").into()))?;
        match self {
            Self::ClearFinished => {
                let (todos, errors) = match get_all_todos(os).await {
                    Ok(res) => res,
                    Err(e) => return Err(ChatError::Custom(format!("Could not get to-do lists: {e}").into())),
                };
                let mut cleared_one = false;

                for todo_status in todos.iter() {
                    if todo_status.tasks.iter().all(|b| b.completed) {
                        match delete_todo(os, &todo_status.id).await {
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
                if !errors.is_empty() {
                    execute!(
                        session.stderr,
                        style::Print(format!("* Failed to get {} todo list(s)\n", errors.len()).dark_grey())
                    )?;
                }
            },
            Self::Resume => match Self::get_descriptions_and_statuses(os).await {
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
            Self::View => match Self::get_descriptions_and_statuses(os).await {
                Ok(entries) => {
                    if entries.is_empty() {
                        execute!(session.stderr, style::Print("No to-do lists to view!\n"))?;
                    } else if let Some(index) = fuzzy_select_todos(&entries, "Select a to-do list to view:") {
                        if index < entries.len() {
                            let list = TodoListState::load(os, &entries[index].id).await.map_err(|e| {
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
                Err(e) => return Err(ChatError::Custom(format!("Could not show to-do lists: {e}").into())),
            },
            Self::Delete { all } => match Self::get_descriptions_and_statuses(os).await {
                Ok(entries) => {
                    if entries.is_empty() {
                        execute!(session.stderr, style::Print("No to-do lists to delete!\n"))?;
                    } else if all {
                        for entry in entries {
                            delete_todo(os, &entry.id)
                                .await
                                .map_err(|_e| ChatError::Custom("Could not delete all to-do lists".into()))?;
                        }
                        execute!(session.stderr, style::Print("✔ Deleted all to-do lists!\n".green()),)?;
                    } else if let Some(index) = fuzzy_select_todos(&entries, "Select a to-do list to delete:") {
                        if index < entries.len() {
                            delete_todo(os, &entries[index].id).await.map_err(|e| {
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
                Err(e) => return Err(ChatError::Custom(format!("Could not show to-do lists: {e}").into())),
            },
        }
        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    /// Convert all to-do list state entries to displayable entries
    async fn get_descriptions_and_statuses(os: &Os) -> Result<Vec<TodoDisplayEntry>> {
        let mut out = Vec::new();
        let (todos, _) = get_all_todos(os).await?;
        for todo in todos.iter() {
            out.push(TodoDisplayEntry {
                num_completed: todo.tasks.iter().filter(|t| t.completed).count(),
                num_tasks: todo.tasks.len(),
                description: todo.description.clone(),
                id: todo.id.clone(),
            });
        }
        Ok(out)
    }
}

fn fuzzy_select_todos(entries: &[TodoDisplayEntry], prompt_str: &str) -> Option<usize> {
    Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt(prompt_str)
        .items(entries)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}
