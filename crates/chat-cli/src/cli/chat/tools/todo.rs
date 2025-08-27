use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
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
    OptionExt,
    Report,
    Result,
    bail,
    eyre,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::InvokeOutput;
use crate::os::Os;

// Local directory to store todo lists
const TODO_LIST_DIR: &str = ".amazonq/cli-todo-lists/";

/// Contains all state to be serialized and deserialized into a todo list
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct TodoListState {
    pub tasks: Vec<String>,
    pub completed: Vec<bool>,
    pub description: String,
    pub context: Vec<String>,
    pub modified_files: Vec<String>,
    pub id: String,
}

impl TodoListState {
    /// Creates a local directory to store todo lists
    pub async fn init_dir(os: &Os) -> Result<()> {
        os.fs.create_dir_all(os.env.current_dir()?.join(TODO_LIST_DIR)).await?;
        Ok(())
    }

    /// Loads a TodoListState with the given id
    pub async fn load(os: &Os, id: &str) -> Result<Self> {
        let state_str = os
            .fs
            .read_to_string(id_to_path(os, id)?)
            .await
            .map_err(|e| eyre!("Could not load todo list: {e}"))?;
        serde_json::from_str::<Self>(&state_str).map_err(|e| eyre!("Could not deserialize todo list: {e}"))
    }

    /// Saves this TodoListState with the given id
    pub async fn save(&self, os: &Os, id: &str) -> Result<()> {
        Self::init_dir(os).await?;
        let path = id_to_path(os, id)?;
        Self::init_dir(os).await?;
        if !os.fs.exists(&path) {
            os.fs.create_new(&path).await?;
        }
        os.fs.write(path, serde_json::to_string(self)?).await?;
        Ok(())
    }

    /// Displays the TodoListState as a to-do list
    pub fn display_list(&self, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("TODO:\n".yellow()))?;
        for (index, (task, completed)) in self.tasks.iter().zip(self.completed.iter()).enumerate() {
            queue_next_without_newline(output, task.clone(), *completed)?;
            if index < self.tasks.len() - 1 {
                queue!(output, style::Print("\n"))?;
            }
        }
        Ok(())
    }
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
pub fn generate_new_todo_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();

    format!("{timestamp}")
}

/// Converts a todo list id to an absolute path in the cwd
pub fn id_to_path(os: &Os, id: &str) -> Result<PathBuf> {
    Ok(os.env.current_dir()?.join(TODO_LIST_DIR).join(format!("{id}.json")))
}

/// Gets all todo lists from the local directory
pub async fn get_all_todos(os: &Os) -> Result<(Vec<TodoListState>, Vec<Report>)> {
    let todo_list_dir = os.env.current_dir()?.join(TODO_LIST_DIR);
    let mut read_dir_output = os.fs.read_dir(todo_list_dir).await?;

    let mut todos = Vec::new();
    let mut errors = Vec::new();

    while let Some(entry) = read_dir_output.next_entry().await? {
        match TodoListState::load(
            os,
            &entry
                .path()
                .with_extension("")
                .file_name()
                .ok_or_eyre("Path is not a file")?
                .to_string_lossy(),
        )
        .await
        {
            Ok(todo) => todos.push(todo),
            Err(e) => errors.push(e),
        };
    }

    Ok((todos, errors))
}

/// Deletes a todo list
pub async fn delete_todo(os: &Os, id: &str) -> Result<()> {
    os.fs.remove_file(id_to_path(os, id)?).await?;
    Ok(())
}

/// Contains the command definitions that allow the model to create,
/// modify, and mark todo list tasks as complete
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase")]
pub enum TodoList {
    // Creates a todo list
    Create {
        tasks: Vec<String>,
        todo_list_description: String,
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

impl TodoList {
    pub async fn invoke(&self, os: &Os, output: &mut impl Write) -> Result<InvokeOutput> {
        let (state, id) = match self {
            TodoList::Create {
                tasks,
                todo_list_description: task_description,
            } => {
                let new_id = generate_new_todo_id();

                // Create a new todo list with the given tasks and save state
                let state = TodoListState {
                    tasks: tasks.clone(),
                    completed: vec![false; tasks.len()],
                    description: task_description.clone(),
                    context: Vec::new(),
                    modified_files: Vec::new(),
                    id: new_id.clone(),
                };
                state.save(os, &new_id).await?;
                state.display_list(output)?;
                (state, new_id)
            },
            TodoList::Complete {
                completed_indices,
                context_update,
                modified_files,
                current_id: id,
            } => {
                let mut state = TodoListState::load(os, id).await?;

                for i in completed_indices.iter() {
                    state.completed[*i] = true;
                }

                state.context.push(context_update.clone());

                if let Some(files) = modified_files {
                    state.modified_files.extend_from_slice(files);
                }
                state.save(os, id).await?;

                // As tasks are being completed, display only the newly completed tasks
                // and the next. Only display the whole list when all tasks are completed
                let last_completed = completed_indices.iter().max().unwrap();
                if *last_completed == state.tasks.len() - 1 || state.completed.iter().all(|c| *c) {
                    state.display_list(output)?;
                } else {
                    let mut display_list = TodoListState {
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
            TodoList::Load { load_id: id } => {
                let state = TodoListState::load(os, id).await?;
                state.display_list(output)?;
                (state, id.clone())
            },
            TodoList::Add {
                new_tasks,
                insert_indices,
                new_description,
                current_id: id,
            } => {
                let mut state = TodoListState::load(os, id).await?;
                for (i, task) in insert_indices.iter().zip(new_tasks.iter()) {
                    state.tasks.insert(*i, task.clone());
                    state.completed.insert(*i, false);
                }
                if let Some(description) = new_description {
                    state.description = description.clone();
                }
                state.save(os, id).await?;
                state.display_list(output)?;
                (state, id.clone())
            },
            TodoList::Remove {
                remove_indices,
                new_description,
                current_id: id,
            } => {
                let mut state = TodoListState::load(os, id).await?;

                // Remove entries in reverse order so indices aren't mismatched
                let mut remove_indices = remove_indices.clone();
                remove_indices.sort();
                for i in remove_indices.iter().rev() {
                    state.tasks.remove(*i);
                    state.completed.remove(*i);
                }
                if let Some(description) = new_description {
                    state.description = description.clone();
                }
                state.save(os, id).await?;
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
            TodoList::Create {
                tasks,
                todo_list_description: task_description,
            } => {
                if tasks.is_empty() {
                    bail!("No tasks were provided");
                } else if tasks.iter().any(|task| task.trim().is_empty()) {
                    bail!("Tasks cannot be empty");
                } else if task_description.is_empty() {
                    bail!("No task description was provided");
                }
            },
            TodoList::Complete {
                completed_indices,
                context_update,
                current_id,
                ..
            } => {
                let state = TodoListState::load(os, current_id).await?;
                if completed_indices.is_empty() {
                    bail!("At least one completed index must be provided");
                } else if context_update.is_empty() {
                    bail!("No context update was provided");
                }
                for i in completed_indices.iter() {
                    if *i >= state.tasks.len() {
                        bail!("Index {i} is out of bounds for length {}, ", state.tasks.len());
                    }
                }
            },
            TodoList::Load { load_id: id } => {
                let state = TodoListState::load(os, id).await?;
                if state.tasks.is_empty() {
                    bail!("Loaded todo list is empty");
                }
            },
            TodoList::Add {
                new_tasks,
                insert_indices,
                new_description,
                current_id: id,
            } => {
                let state = TodoListState::load(os, id).await?;
                if new_tasks.iter().any(|task| task.trim().is_empty()) {
                    bail!("New tasks cannot be empty");
                } else if has_duplicates(insert_indices) {
                    bail!("Insertion indices must be unique")
                } else if new_tasks.len() != insert_indices.len() {
                    bail!("Must provide an index for every new task");
                } else if new_description.is_some() && new_description.as_ref().unwrap().trim().is_empty() {
                    bail!("New description cannot be empty");
                }
                for i in insert_indices.iter() {
                    if *i > state.tasks.len() {
                        bail!("Index {i} is out of bounds for length {}, ", state.tasks.len());
                    }
                }
            },
            TodoList::Remove {
                remove_indices,
                new_description,
                current_id: id,
            } => {
                let state = TodoListState::load(os, id).await?;
                if has_duplicates(remove_indices) {
                    bail!("Removal indices must be unique")
                } else if new_description.is_some() && new_description.as_ref().unwrap().trim().is_empty() {
                    bail!("New description cannot be empty");
                }
                for i in remove_indices.iter() {
                    if *i >= state.tasks.len() {
                        bail!("Index {i} is out of bounds for length {}, ", state.tasks.len());
                    }
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
