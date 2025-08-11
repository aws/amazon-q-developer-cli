use std::collections::HashSet;
use std::io::Write;
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

use crossterm::style::Stylize;
use crossterm::{
    queue,
    style,
};
use eyre::{
    Result,
    bail,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::InvokeOutput;
use crate::os::Os;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase")]
pub enum TodoInput {
    // Creates a todo list
    Create {
        tasks: Vec<String>,
        task_description: String,
    },

    // Completes tasks corresponding to the provided indices
    // on the currently loaded todo list
    Complete {
        completed_indices: Vec<usize>,
        context_update: String,
        modified_files: Option<Vec<String>>,
        current_id: String,
    },

    // Loads a todo list with the given id
    Load {
        load_id: String,
    },

    // Inserts new tasks into the current todo list
    Add {
        new_tasks: Vec<String>,
        insert_indices: Vec<usize>,
        new_description: Option<String>,
        current_id: String,
    },

    // Removes tasks from the current todo list
    Remove {
        remove_indices: Vec<usize>,
        new_description: Option<String>,
        current_id: String,
    },
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct TodoState {
    pub tasks: Vec<String>,
    pub completed: Vec<bool>,
    pub task_description: String,
    pub context: Vec<String>,
    pub modified_files: Vec<String>,
}

impl TodoState {
    /// Loads a TodoState with the given id
    pub fn load(os: &Os, id: &str) -> Result<Self> {
        match os.database.get_todo(id)? {
            Some(state) => Ok(state),
            None => bail!("No to-do list with id {}", id),
        }
    }

    /// Saves this TodoState with the given id
    pub fn save(&self, os: &Os, id: &str) -> Result<()> {
        os.database.set_todo(id, self)?;
        Ok(())
    }

    /// Displays the TodoState as a to-do list
    pub fn display_list(&self, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("TODO:\n".yellow()))?;
        for (index, (task, completed)) in self.tasks.iter().zip(self.completed.iter()).enumerate() {
            TodoState::queue_next_without_newline(output, task.clone(), *completed)?;
            if index < self.tasks.len() - 1 {
                queue!(output, style::Print("\n"))?;
            }
        }
        Ok(())
    }

    /// Displays a single empty or marked off to-do list task depending on
    /// the completion status
    fn queue_next_without_newline(output: &mut impl Write, task: String, completed: bool) -> Result<()> {
        if completed {
            queue!(
                output,
                style::SetAttribute(style::Attribute::Italic),
                style::SetForegroundColor(style::Color::Green),
                style::Print(" ■ "),
                style::SetForegroundColor(style::Color::DarkGrey),
                style::Print(task),
                style::SetAttribute(style::Attribute::NoItalic),
            )?;
        } else {
            queue!(
                output,
                style::SetForegroundColor(style::Color::Reset),
                style::Print(format!(" ☐ {task}")),
            )?;
        }
        Ok(())
    }

    /// Generates a new unique id be used for new to-do lists
    pub fn generate_new_id() -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();

        format!("{timestamp}")
    }
}

impl TodoInput {
    pub async fn invoke(&self, os: &Os, output: &mut impl Write) -> Result<InvokeOutput> {
        let (state, id) = match self {
            TodoInput::Create {
                tasks,
                task_description,
            } => {
                // Create a new todo list with the given tasks and save state to database
                let state = TodoState {
                    tasks: tasks.clone(),
                    completed: vec![false; tasks.len()],
                    task_description: task_description.clone(),
                    context: Vec::new(),
                    modified_files: Vec::new(),
                };
                let new_id = TodoState::generate_new_id();
                state.save(os, &new_id)?;
                state.display_list(output)?;
                (state, new_id)
            },
            TodoInput::Complete {
                completed_indices,
                context_update,
                modified_files,
                current_id: id,
            } => {
                let mut state = TodoState::load(os, id)?;

                for i in completed_indices.iter() {
                    state.completed[*i] = true;
                }

                state.context.push(context_update.clone());

                if let Some(files) = modified_files {
                    state.modified_files.extend_from_slice(files);
                }
                state.save(os, id)?;

                // As tasks are being completed, display only the newly completed tasks
                // and the next. Only display the whole list when all tasks are completed
                let last_completed = completed_indices.iter().max().unwrap();
                if *last_completed == state.tasks.len() - 1 || state.completed.iter().all(|c| *c) {
                    state.display_list(output)?;
                } else {
                    let mut display_list = TodoState {
                        tasks: completed_indices.iter().map(|i| state.tasks[*i].clone()).collect(),
                        ..Default::default()
                    };
                    for _ in 0..completed_indices.len() {
                        display_list.completed.push(true);
                    }

                    // For next state, mark it true/false depending on actual completion state
                    // This only matters when the model skips around tasks
                    display_list.tasks.push(state.tasks[*last_completed + 1].clone());
                    display_list.completed.push(state.completed[*last_completed + 1]);

                    display_list.display_list(output)?;
                }
                (state, id.clone())
            },
            TodoInput::Load { load_id: id } => {
                let state = TodoState::load(os, id)?;
                state.display_list(output)?;
                (state, id.clone())
            },
            TodoInput::Add {
                new_tasks,
                insert_indices,
                new_description,
                current_id: id,
            } => {
                let mut state = TodoState::load(os, id)?;
                for (i, task) in insert_indices.iter().zip(new_tasks.iter()) {
                    state.tasks.insert(*i, task.clone());
                    state.completed.insert(*i, false);
                }
                if let Some(description) = new_description {
                    state.task_description = description.clone();
                }
                state.save(os, id)?;
                state.display_list(output)?;
                (state, id.clone())
            },
            TodoInput::Remove {
                remove_indices,
                new_description,
                current_id: id,
            } => {
                let mut state = TodoState::load(os, id)?;

                // Remove entries in reverse order so indices aren't mismatched
                let mut remove_indices = remove_indices.clone();
                remove_indices.sort();
                for i in remove_indices.iter().rev() {
                    state.tasks.remove(*i);
                    state.completed.remove(*i);
                }
                if let Some(description) = new_description {
                    state.task_description = description.clone();
                }
                state.save(os, id)?;
                state.display_list(output)?;
                (state, id.clone())
            },
        };

        let invoke_output = format!("TODO LIST STATE: {}\n\n ID: {id}", serde_json::to_string(&state)?);
        Ok(InvokeOutput {
            output: super::OutputKind::Text(invoke_output),
        })
    }

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        match self {
            TodoInput::Create {
                tasks,
                task_description,
            } => {
                if tasks.is_empty() {
                    bail!("No tasks were provided");
                } else if tasks.iter().any(|task| task.trim().is_empty()) {
                    bail!("Tasks cannot be empty");
                } else if task_description.is_empty() {
                    bail!("No task description was provided");
                }
            },
            TodoInput::Complete {
                completed_indices,
                context_update,
                current_id,
                ..
            } => {
                let state = TodoState::load(os, current_id)?;
                if completed_indices.is_empty() {
                    bail!("At least one completed index must be provided");
                } else if completed_indices.iter().any(|i| *i >= state.completed.len()) {
                    bail!("Completed index is out of bounds");
                } else if context_update.is_empty() {
                    bail!("No context update was provided");
                }
            },
            TodoInput::Load { load_id: id } => {
                let state = TodoState::load(os, id)?;
                if state.tasks.is_empty() {
                    bail!("Loaded todo list is empty");
                }
            },
            TodoInput::Add {
                new_tasks,
                insert_indices,
                new_description,
                current_id: id,
            } => {
                let state = TodoState::load(os, id)?;
                if new_tasks.iter().any(|task| task.trim().is_empty()) {
                    bail!("New tasks cannot be empty");
                } else if has_duplicates(insert_indices) {
                    bail!("Insertion indices must be unique")
                } else if new_tasks.len() != insert_indices.len() {
                    bail!("Must provide an index for every new task");
                } else if insert_indices.iter().any(|i| *i > state.tasks.len()) {
                    bail!("Index is out of bounds");
                } else if new_description.is_some() && new_description.as_ref().unwrap().trim().is_empty() {
                    bail!("New description cannot be empty");
                }
            },
            TodoInput::Remove {
                remove_indices,
                new_description,
                current_id: id,
            } => {
                let state = TodoState::load(os, id)?;
                if has_duplicates(remove_indices) {
                    bail!("Removal indices must be unique")
                } else if remove_indices.iter().any(|i| *i > state.tasks.len()) {
                    bail!("Index is out of bounds");
                } else if new_description.is_some() && new_description.as_ref().unwrap().trim().is_empty() {
                    bail!("New description cannot be empty");
                }
            },
        }
        Ok(())
    }
}

/// Generated by Q
fn has_duplicates<T>(vec: &[T]) -> bool
where
    T: std::hash::Hash + Eq,
{
    let mut seen = HashSet::with_capacity(vec.len());
    vec.iter().any(|item| !seen.insert(item))
}
