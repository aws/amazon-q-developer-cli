use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use fig_os_shim::Context;
use memory_bank_client::MemoryBankClient;
use once_cell::sync::Lazy;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::{
    InvokeOutput,
    OutputKind,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command")]
pub enum Memory {
    Add(MemoryAdd),
    Remove(MemoryRemove),
    Clear(MemoryClear),
    Search(MemorySearch),
    Show,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryAdd {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryRemove {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub context_id: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryClear {
    pub confirm: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemorySearch {
    pub query: String,
    pub context_id: Option<String>,
}

impl Memory {
    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        match self {
            Memory::Add(_) => Ok(()),
            Memory::Remove(remove) => {
                if remove.name.is_empty() && remove.context_id.is_empty() && remove.path.is_empty() {
                    eyre::bail!("Please provide at least one of: name, context_id, or path");
                }
                Ok(())
            },
            Memory::Clear(clear) => {
                if !clear.confirm {
                    eyre::bail!("Please confirm clearing memory by setting confirm=true");
                }
                Ok(())
            },
            Memory::Search(_) => Ok(()),
            Memory::Show => Ok(()),
        }
    }

    pub async fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        match self {
            Memory::Add(add) => {
                queue!(
                    updates,
                    style::Print("Adding to memory: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(&add.name),
                    style::ResetColor,
                )?;

                // Check if value is a path or text content
                let path = PathBuf::from(&add.value);
                if path.exists() {
                    queue!(
                        updates,
                        style::Print(" (directory: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&add.value),
                        style::ResetColor,
                        style::Print(")")
                    )?;
                } else {
                    let preview: String = add.value.chars().take(20).collect();
                    if add.value.len() > 20 {
                        queue!(
                            updates,
                            style::Print(" (text: "),
                            style::SetForegroundColor(Color::Blue),
                            style::Print(format!("{}...", preview)),
                            style::ResetColor,
                            style::Print(")")
                        )?;
                    } else {
                        queue!(
                            updates,
                            style::Print(" (text: "),
                            style::SetForegroundColor(Color::Blue),
                            style::Print(&add.value),
                            style::ResetColor,
                            style::Print(")")
                        )?;
                    }
                }
            },
            Memory::Remove(remove) => {
                if !remove.name.is_empty() {
                    queue!(
                        updates,
                        style::Print("Removing from memory by name: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&remove.name),
                        style::ResetColor,
                    )?;
                } else if !remove.context_id.is_empty() {
                    queue!(
                        updates,
                        style::Print("Removing from memory by ID: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&remove.context_id),
                        style::ResetColor,
                    )?;
                } else if !remove.path.is_empty() {
                    queue!(
                        updates,
                        style::Print("Removing from memory by path: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&remove.path),
                        style::ResetColor,
                    )?;
                } else {
                    queue!(
                        updates,
                        style::Print("Removing from memory: "),
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("No identifier provided"),
                        style::ResetColor,
                    )?;
                }
            },
            Memory::Clear(_) => {
                queue!(
                    updates,
                    style::Print("Clearing "),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("all"),
                    style::ResetColor,
                    style::Print(" memory entries"),
                )?;
            },
            Memory::Search(search) => {
                queue!(
                    updates,
                    style::Print("Searching memory for: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(&search.query),
                    style::ResetColor,
                )?;

                if let Some(context_id) = &search.context_id {
                    queue!(
                        updates,
                        style::Print(" in context: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(context_id),
                        style::ResetColor,
                    )?;
                } else {
                    queue!(updates, style::Print(" across all contexts"),)?;
                }
            },
            Memory::Show => {
                queue!(updates, style::Print("Showing all memory entries"),)?;
            },
        };
        Ok(())
    }

    pub async fn invoke(&self, _ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        // Get the memory store singleton
        let memory_store = MemoryStore::get_instance();
        let mut store = memory_store.lock().await;

        let result = match self {
            Memory::Add(add) => {
                // For path indexing, we'll show a progress message first
                let path = PathBuf::from(&add.value);
                if path.exists() {
                    if path.is_dir() {
                        writeln!(updates, "Indexing directory: {}...", add.value)?;
                    } else {
                        writeln!(updates, "Indexing file: {}...", add.value)?;
                    }
                    writeln!(updates, "This may take a moment depending on the size.")?;
                }

                store.add(&add.name, &add.value);
                format!("Added '{}' to memory", add.name)
            },
            Memory::Remove(remove) => {
                if !remove.context_id.is_empty() {
                    // Remove by ID
                    if store.remove_by_id(&remove.context_id) {
                        format!("Removed context with ID '{}' from memory", remove.context_id)
                    } else {
                        format!("Context with ID '{}' not found in memory", remove.context_id)
                    }
                } else if !remove.name.is_empty() {
                    // Remove by name
                    if store.remove_by_name(&remove.name) {
                        format!("Removed context with name '{}' from memory", remove.name)
                    } else {
                        format!("Context with name '{}' not found in memory", remove.name)
                    }
                } else if !remove.path.is_empty() {
                    // Remove by path
                    if store.remove_by_path(&remove.path) {
                        format!("Removed context with path '{}' from memory", remove.path)
                    } else {
                        format!("Context with path '{}' not found in memory", remove.path)
                    }
                } else {
                    "Error: No identifier provided for removal. Please specify name, context_id, or path.".to_string()
                }
            },
            Memory::Clear(_) => {
                let count = store.clear();
                format!("Cleared {} entries from memory", count)
            },
            Memory::Search(search) => {
                let results = store.search(&search.query, search.context_id.as_deref());
                if results.is_empty() {
                    "No matching entries found in memory".to_string()
                } else {
                    let mut output = String::from("Search results:\n");
                    for (name, value) in results {
                        output.push_str(&format!("- {}: {}\n", name, value));
                    }
                    output
                }
            },
            Memory::Show => {
                let contexts = store.get_all();
                if contexts.is_empty() {
                    "No memory entries found".to_string()
                } else {
                    let mut output = String::from("Memory entries:\n");
                    for context in contexts {
                        output.push_str(&format!("- ID: {}\n  Name: {}\n  Description: {}\n  Persistent: {}\n  Created: {}\n  Last Updated: {}\n\n",
                            context.id,
                            context.name,
                            context.description,
                            context.persistent,
                            context.created_at.format("%Y-%m-%d %H:%M:%S"),
                            context.updated_at.format("%Y-%m-%d %H:%M:%S")
                        ));
                    }
                    output
                }
            },
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }
}

// Simple in-memory store implementation
pub struct MemoryStore {
    memory_bank_client: MemoryBankClient,
}

impl MemoryStore {
    pub(crate) fn new() -> Self {
        let expanded_path = shellexpand::tilde("~/.aws/amazonq/memory_bank");
        let global_path = PathBuf::from(expanded_path.as_ref());
        Self {
            memory_bank_client: MemoryBankClient::new(global_path).unwrap(),
        }
    }

    // Create a test instance with an isolated directory
    pub(crate) fn new_test_instance() -> Self {
        // Create a temporary directory for tests
        let temp_dir = std::env::temp_dir()
            .join("memory_bank_test")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&temp_dir).unwrap();
        Self {
            memory_bank_client: MemoryBankClient::new(temp_dir).unwrap(),
        }
    }

    // Singleton pattern for memory store with test mode support
    pub fn get_instance() -> Arc<Mutex<Self>> {
        static INSTANCE: Lazy<Arc<Mutex<MemoryStore>>> = Lazy::new(|| Arc::new(Mutex::new(MemoryStore::new())));

        // Check if we're running in a test environment
        if cfg!(test) {
            // For tests, create a new isolated instance each time
            Arc::new(Mutex::new(MemoryStore::new_test_instance()))
        } else {
            // For normal operation, use the singleton
            INSTANCE.clone()
        }
    }

    pub fn add(&mut self, key: &str, value: &str) {
        let path = PathBuf::from(value);
        if path.exists() {
            // Clone the path before moving it
            let path_clone = path.clone();
            let path_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");

            // Create a progress bar
            let pb = indicatif::ProgressBar::new(100);
            pb.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg} {pos}/{len} ({eta})")
                    .unwrap()
                    .progress_chars("#>-"),
            );

            // Enable steady tick to ensure the progress bar updates regularly
            pb.enable_steady_tick(std::time::Duration::from_millis(100));

            // Use a progress callback
            let _ = self.memory_bank_client.add_context_from_path(
                path_clone,
                key,
                path_name,
                true,
                Some(move |status| match status {
                    memory_bank_client::types::ProgressStatus::CountingFiles => {
                        pb.set_message("Counting files...");
                        pb.set_length(100);
                        pb.set_position(0);
                    },
                    memory_bank_client::types::ProgressStatus::StartingIndexing(total) => {
                        pb.set_message("Indexing files...");
                        pb.set_length(total as u64);
                        pb.set_position(0);
                    },
                    memory_bank_client::types::ProgressStatus::Indexing(current, total) => {
                        pb.set_message(format!("Indexing file {} of {}", current, total));
                        pb.set_position(current as u64);
                    },
                    memory_bank_client::types::ProgressStatus::Finalizing => {
                        pb.set_message("Finalizing index...");
                        if let Some(len) = pb.length() {
                            pb.set_position(len - 1);
                        }
                    },
                    memory_bank_client::types::ProgressStatus::Complete => {
                        pb.finish_with_message("Indexing complete!");
                        pb.println("✅ Successfully indexed all files and created memory context");
                    },
                    memory_bank_client::types::ProgressStatus::CreatingSemanticContext => {
                        pb.set_message("Creating semantic context...");
                    },
                    memory_bank_client::types::ProgressStatus::GeneratingEmbeddings(current, total) => {
                        pb.set_message(format!("Generating embeddings {} of {}", current, total));
                        pb.set_position(current as u64);
                    },
                    memory_bank_client::types::ProgressStatus::BuildingIndex => {
                        pb.set_message("Building vector index...");
                    },
                }),
            );
        } else {
            let preview: String = value.chars().take(40).collect();
            let _ =
                self.memory_bank_client
                    .add_context_from_text(key, value, &format!("Text memory {}...", preview), true);
        }
    }

    pub fn remove_by_id(&mut self, id: &str) -> bool {
        self.memory_bank_client.remove_context_by_id(id, true).is_ok()
    }

    pub fn remove_by_name(&mut self, name: &str) -> bool {
        self.memory_bank_client.remove_context_by_name(name, true).is_ok()
    }

    pub fn remove_by_path(&mut self, path: &str) -> bool {
        self.memory_bank_client.remove_context_by_path(path, true).is_ok()
    }

    pub fn clear(&mut self) -> usize {
        let contexts = self.memory_bank_client.get_contexts();

        let count = contexts.len();

        for context in contexts {
            let _ = self.memory_bank_client.remove_context(&context.id, true);
        }

        count
    }

    pub fn search(&self, query: &str, context_id: Option<&str>) -> Vec<(String, String)> {
        match context_id {
            // If context_id is provided, search only in that specific context
            Some(id) => match self.memory_bank_client.search_context(id, query, 5) {
                Ok(search_results) => search_results
                    .into_iter()
                    .map(|result| {
                        let content = result.text().unwrap_or("[No text content]").to_string();
                        (id.to_string(), content)
                    })
                    .collect(),
                Err(_) => Vec::new(),
            },
            // If no context_id is provided, search across all contexts
            None => match self.memory_bank_client.search_all(query, 5) {
                Ok(results) => results
                    .into_iter()
                    .flat_map(|(context_id, search_results)| {
                        search_results.into_iter().map(move |result| {
                            let content = result.text().unwrap_or("[No text content]").to_string();
                            (context_id.clone(), content)
                        })
                    })
                    .collect(),
                Err(_) => Vec::new(),
            },
        }
    }

    pub fn get_all(&self) -> Vec<memory_bank_client::MemoryContext> {
        self.memory_bank_client.get_contexts()
    }
}
