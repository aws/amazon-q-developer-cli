use std::io::Write;
use std::collections::HashSet;
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

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

// Demo prompts:
// Create a Python package layout with a blank main file and a blank utilities file. Start by making
// a todo list. Design your own super simple programming task with 4 steps. Make a todo list for the
// task, and begin executing those steps. Implement a basic input to Python type converter where the
// user can input either a string or integer and it gets converted to the corresponding Python
// object.

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command")]
pub enum TodoInput {
    #[serde(rename = "create")]
    Create {
        tasks: Vec<String>,
        task_description: String,
    },

    #[serde(rename = "complete")]
    Complete {
        completed_indices: Vec<usize>,
        context_update: String,
        modified_files: Option<Vec<String>>,
    },

    #[serde(rename = "load")]
    Load { id: String },

    #[serde(rename = "add")]
    Add {
        new_tasks: Vec<String>,
        insert_indices: Vec<usize>,
        new_description: Option<String>,
    },

    #[serde(rename = "remove")]
    Remove {
        remove_indices: Vec<usize>,
        new_description: Option<String>,
    }
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
        queue!(output, style::Print("TODO:\n"),)?;
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
                // style::SetAttribute(style::Attribute::CrossedOut),
                style::Print(task),
                // style::SetAttribute(style::Attribute::NotCrossedOut),
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

    pub fn get_current_todo_id(os: &Os) -> Result<Option<String>> {
        Ok(os.database.get_current_todo_id()?)
    }

    pub fn set_current_todo_id(os: &Os, id: &str) -> Result<()> {
        os.database.set_current_todo_id(id)?;
        Ok(())
    }

    /// Generates a new unique filename to be used for new to-do lists
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
        let state = match self {
            TodoInput::Create {
                tasks,
                task_description,
            } => {
                let state = TodoState {
                    tasks: tasks.clone(),
                    completed: vec![false; tasks.len()],
                    task_description: task_description.clone(),
                    context: Vec::new(),
                    modified_files: Vec::new(),
                };
                let new_id = TodoState::generate_new_id();
                state.save(os, &new_id)?;
                TodoState::set_current_todo_id(os, &new_id)?;
                state
            },
            TodoInput::Complete {
                completed_indices,
                context_update,
                modified_files,
            } => {
                let current_id = match TodoState::get_current_todo_id(os)? {
                    Some(id) => id,
                    None => bail!("No to-do list currently loaded"),
                };
                let mut state = TodoState::load(os, &current_id)?;

                for i in completed_indices.iter() {
                    state.completed[*i] = true;
                }

                state.context.push(context_update.clone());

                if let Some(files) = modified_files {
                    state.modified_files.extend_from_slice(files);
                }
                state.save(os, &current_id)?;
                state
            },
            TodoInput::Load { id } => {
                TodoState::set_current_todo_id(os, id)?;
                TodoState::load(os, id)?
            },
            TodoInput::Add { new_tasks, insert_indices, new_description } => {
                let current_id = match TodoState::get_current_todo_id(os)? {
                    Some(id) => id,
                    None => bail!("No to-do list currently loaded"),
                };
                let mut state = TodoState::load(os, &current_id)?;
                for (i, task) in insert_indices.iter().zip(new_tasks.iter()) {
                    state.tasks.insert(*i, task.clone());
                    state.completed.insert(*i, false);
                }
                if let Some(description) = new_description {
                    state.task_description = description.clone();
                }
                state.save(os, &current_id)?;
                state
            },
            TodoInput::Remove { remove_indices, new_description } => {
                let current_id = match TodoState::get_current_todo_id(os)? {
                    Some(id) => id,
                    None => bail!("No to-do list currently loaded"),
                };
                let mut state = TodoState::load(os, &current_id)?;
                for i in remove_indices.iter() {
                    state.tasks.remove(*i);
                    state.completed.remove(*i);
                }
                if let Some(description) = new_description {
                    state.task_description = description.clone();
                }
                state.save(os, &current_id)?;
                state
            }
        };
        state.display_list(output)?;
        output.flush()?;

        Ok(Default::default())
    }

    pub fn queue_description(&self, _os: &Os, _output: &mut impl Write) -> Result<()> {
        Ok(())
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
                ..
            } => {
                let current_id = match TodoState::get_current_todo_id(os)? {
                    Some(id) => id,
                    None => bail!("No todo list is currently loaded"),
                };
                let state = TodoState::load(os, &current_id)?;
                if completed_indices.iter().any(|i| *i >= state.completed.len()) {
                    bail!("Completed index is out of bounds");
                } else if context_update.is_empty() {
                    bail!("No context update was provided");
                }
            },
            TodoInput::Load { id } => {
                let state = TodoState::load(os, id)?;
                if state.tasks.is_empty() {
                    bail!("Loaded todo list is empty");
                }
            },
            TodoInput::Add { new_tasks, insert_indices, new_description } => {
                let current_id = match TodoState::get_current_todo_id(os)? {
                    Some(id) => id,
                    None => bail!("No todo list is currently loaded"),
                };
                let state = TodoState::load(os, &current_id)?;
                if new_tasks.iter().any(|task| task.trim().is_empty()) {
                    bail!("New tasks cannot be empty");
                } else if has_duplicates(&insert_indices) {
                    bail!("Insertion indices must be unique")
                } else if new_tasks.len() != insert_indices.len() {
                    bail!("Must provide an index for every new task");
                } else if insert_indices.iter().any(|i| *i > state.tasks.len()) {
                    bail!("Index is out of bounds");
                } else if new_description.is_some() && new_description.as_ref().unwrap().trim().is_empty() {
                    bail!("New description cannot be empty");
                }
            },
            TodoInput::Remove { remove_indices, new_description } => {
                let current_id = match TodoState::get_current_todo_id(os)? {
                    Some(id) => id,
                    None => bail!("No todo list is currently loaded"),
                };
                let state = TodoState::load(os, &current_id)?;
                if has_duplicates(&remove_indices) {
                    bail!("Removal indices must be unique")
                } else if remove_indices.iter().any(|i| *i > state.tasks.len()) {
                    bail!("Index is out of bounds");
                } else if new_description.is_some() && new_description.as_ref().unwrap().trim().is_empty() {
                    bail!("New description cannot be empty");
                }
            }
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