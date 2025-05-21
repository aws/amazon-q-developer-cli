use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use once_cell::sync::Lazy;
use semantic_search_client::SemanticSearchClient;
use semantic_search_client::types::{
    MemoryContext,
    SearchResult,
};
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::warn;

use super::{
    InvokeOutput,
    OutputKind,
};
use crate::platform::Context;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command")]
pub enum Knowledge {
    #[serde(rename = "add")]
    Add(KnowledgeAdd),
    #[serde(rename = "remove")]
    Remove(KnowledgeRemove),
    #[serde(rename = "clear")]
    Clear(KnowledgeClear),
    #[serde(rename = "search")]
    Search(KnowledgeSearch),
    #[serde(rename = "update")]
    Update(KnowledgeUpdate),
    #[serde(rename = "show")]
    Show,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeAdd {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeRemove {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub context_id: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeClear {
    pub confirm: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeSearch {
    pub query: String,
    pub context_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeUpdate {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub context_id: String,
    #[serde(default)]
    pub name: String,
}

impl Knowledge {
    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        match self {
            Knowledge::Add(add) => {
                // Check if value is intended to be a path (doesn't contain newlines)
                if !add.value.contains('\n') {
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(ctx, &add.value);
                    if !path.exists() {
                        eyre::bail!("Path '{}' does not exist", add.value);
                    }
                }
                Ok(())
            },
            Knowledge::Remove(remove) => {
                if remove.name.is_empty() && remove.context_id.is_empty() && remove.path.is_empty() {
                    eyre::bail!("Please provide at least one of: name, context_id, or path");
                }
                // If path is provided, validate it exists
                if !remove.path.is_empty() {
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(ctx, &remove.path);
                    if !path.exists() {
                        warn!(
                            "Path '{}' does not exist, will try to remove by path string match",
                            remove.path
                        );
                    }
                }
                Ok(())
            },
            Knowledge::Update(update) => {
                // Require at least one identifier (context_id or name)
                if update.context_id.is_empty() && update.name.is_empty() && update.path.is_empty() {
                    eyre::bail!("Please provide either context_id or name or path to identify the context to update");
                }

                // Validate the path exists
                if !update.path.is_empty() {
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(ctx, &update.path);
                    if !path.exists() {
                        eyre::bail!("Path '{}' does not exist", update.path);
                    }
                }

                Ok(())
            },
            Knowledge::Clear(clear) => {
                if !clear.confirm {
                    eyre::bail!("Please confirm clearing knowledge base by setting confirm=true");
                }
                Ok(())
            },
            Knowledge::Search(_) => Ok(()),
            Knowledge::Show => Ok(()),
        }
    }

    pub async fn queue_description(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        match self {
            Knowledge::Add(add) => {
                queue!(
                    updates,
                    style::Print("Adding to knowledge base: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(&add.name),
                    style::ResetColor,
                )?;

                // Check if value is a path or text content
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(ctx, &add.value);
                if path.exists() {
                    let path_type = if path.is_dir() { "directory" } else { "file" };
                    queue!(
                        updates,
                        style::Print(format!(" ({}: ", path_type)),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&add.value),
                        style::ResetColor,
                        style::Print(")\n")
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
                            style::Print(")\n")
                        )?;
                    } else {
                        queue!(
                            updates,
                            style::Print(" (text: "),
                            style::SetForegroundColor(Color::Blue),
                            style::Print(&add.value),
                            style::ResetColor,
                            style::Print(")\n")
                        )?;
                    }
                }
            },
            Knowledge::Remove(remove) => {
                if !remove.name.is_empty() {
                    queue!(
                        updates,
                        style::Print("Removing from knowledge base by name: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&remove.name),
                        style::ResetColor,
                    )?;
                } else if !remove.context_id.is_empty() {
                    queue!(
                        updates,
                        style::Print("Removing from knowledge base by ID: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&remove.context_id),
                        style::ResetColor,
                    )?;
                } else if !remove.path.is_empty() {
                    queue!(
                        updates,
                        style::Print("Removing from knowledge base by path: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&remove.path),
                        style::ResetColor,
                    )?;
                } else {
                    queue!(
                        updates,
                        style::Print("Removing from knowledge base: "),
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("No identifier provided"),
                        style::ResetColor,
                    )?;
                }
            },
            Knowledge::Update(update) => {
                queue!(updates, style::Print("Updating knowledge base context"),)?;

                if !update.context_id.is_empty() {
                    queue!(
                        updates,
                        style::Print(" with ID: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&update.context_id),
                        style::ResetColor,
                    )?;
                } else if !update.name.is_empty() {
                    queue!(
                        updates,
                        style::Print(" with name: "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&update.name),
                        style::ResetColor,
                    )?;
                }

                let path = crate::cli::chat::tools::sanitize_path_tool_arg(ctx, &update.path);
                let path_type = if path.is_dir() { "directory" } else { "file" };
                queue!(
                    updates,
                    style::Print(format!(" using new {}: ", path_type)),
                    style::SetForegroundColor(Color::Green),
                    style::Print(&update.path),
                    style::ResetColor,
                )?;
            },
            Knowledge::Clear(_) => {
                queue!(
                    updates,
                    style::Print("Clearing "),
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("all"),
                    style::ResetColor,
                    style::Print(" knowledge base entries"),
                )?;
            },
            Knowledge::Search(search) => {
                queue!(
                    updates,
                    style::Print("Searching knowledge base for: "),
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
            Knowledge::Show => {
                queue!(updates, style::Print("Showing all knowledge base entries"),)?;
            },
        };
        Ok(())
    }

    pub async fn invoke(&self, ctx: &Context, _updates: &mut impl Write) -> Result<InvokeOutput> {
        // Get the knowledge store singleton
        let knowledge_store = KnowledgeStore::get_instance();
        let mut store = knowledge_store.lock().await;

        let result = match self {
            Knowledge::Add(add) => {
                // For path indexing, we'll show a progress message first
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(ctx, &add.value);
                let value_to_use = if path.exists() {
                    path.to_string_lossy().to_string()
                } else {
                    // If it's not a valid path, use the original value (might be text content)
                    add.value.clone()
                };

                match store.add(&add.name, &value_to_use) {
                    Ok(context_id) => format!("Added '{}' to knowledge base with ID: {}", add.name, context_id),
                    Err(e) => format!("Failed to add to knowledge base: {}", e),
                }
            },
            Knowledge::Remove(remove) => {
                if !remove.context_id.is_empty() {
                    // Remove by ID
                    match store.remove_by_id(&remove.context_id) {
                        Ok(_) => format!("Removed context with ID '{}' from knowledge base", remove.context_id),
                        Err(e) => format!("Failed to remove context by ID: {}", e),
                    }
                } else if !remove.name.is_empty() {
                    // Remove by name
                    match store.remove_by_name(&remove.name) {
                        Ok(_) => format!("Removed context with name '{}' from knowledge base", remove.name),
                        Err(e) => format!("Failed to remove context by name: {}", e),
                    }
                } else if !remove.path.is_empty() {
                    // Remove by path
                    let sanitized_path = crate::cli::chat::tools::sanitize_path_tool_arg(ctx, &remove.path);
                    match store.remove_by_path(&sanitized_path.to_string_lossy()) {
                        Ok(_) => format!("Removed context with path '{}' from knowledge base", remove.path),
                        Err(e) => format!("Failed to remove context by path: {}", e),
                    }
                } else {
                    "Error: No identifier provided for removal. Please specify name, context_id, or path.".to_string()
                }
            },
            Knowledge::Update(update) => {
                // Validate that we have a path and at least one identifier
                if update.path.is_empty() {
                    return Ok(InvokeOutput {
                        output: OutputKind::Text(
                            "Error: No path provided for update. Please specify a path to update with.".to_string(),
                        ),
                    });
                }

                // Sanitize the path
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(ctx, &update.path);
                if !path.exists() {
                    return Ok(InvokeOutput {
                        output: OutputKind::Text(format!("Error: Path '{}' does not exist", update.path)),
                    });
                }

                let sanitized_path = path.to_string_lossy().to_string();

                // Choose the appropriate update method based on provided identifiers
                if !update.context_id.is_empty() {
                    // Update by ID
                    match store.update_context_by_id(&update.context_id, &sanitized_path) {
                        Ok(_) => format!(
                            "Updated context with ID '{}' using path '{}'",
                            update.context_id, update.path
                        ),
                        Err(e) => format!("Failed to update context by ID: {}", e),
                    }
                } else if !update.name.is_empty() {
                    // Update by name
                    match store.update_context_by_name(&update.name, &sanitized_path) {
                        Ok(_) => format!(
                            "Updated context with name '{}' using path '{}'",
                            update.name, update.path
                        ),
                        Err(e) => format!("Failed to update context by name: {}", e),
                    }
                } else {
                    // Update by path (if no ID or name provided)
                    match store.update_by_path(&sanitized_path) {
                        Ok(_) => format!("Updated context with path '{}'", update.path),
                        Err(e) => format!("Failed to update context by path: {}", e),
                    }
                }
            },
            Knowledge::Clear(_) => match store.clear() {
                Ok(count) => format!("Cleared {} entries from knowledge base", count),
                Err(e) => format!("Failed to clear knowledge base: {}", e),
            },
            Knowledge::Search(search) => {
                // Only use a spinner for search, not a full progress bar
                let results = store.search(&search.query, search.context_id.as_deref());
                match results {
                    Ok(results) => {
                        if results.is_empty() {
                            "No matching entries found in knowledge base".to_string()
                        } else {
                            let mut output = String::from("Search results:\n");
                            for result in results {
                                if let Some(text) = result.text() {
                                    output.push_str(&format!("- {}\n", text));
                                }
                            }
                            output
                        }
                    },
                    Err(e) => format!("Search failed: {}", e),
                }
            },
            Knowledge::Show => {
                let contexts = store.get_all();
                match contexts {
                    Ok(contexts) => {
                        if contexts.is_empty() {
                            "No knowledge base entries found".to_string()
                        } else {
                            let mut output = String::from("Knowledge base entries:\n");
                            for context in contexts {
                                output.push_str(&format!("- ID: {}\n  Name: {}\n  Description: {}\n  Persistent: {}\n  Created: {}\n  Last Updated: {}\n  Items: {}\n\n",
                                    context.id,
                                    context.name,
                                    context.description,
                                    context.persistent,
                                    context.created_at.format("%Y-%m-%d %H:%M:%S"),
                                    context.updated_at.format("%Y-%m-%d %H:%M:%S"),
                                    context.item_count
                                ));
                            }
                            output
                        }
                    },
                    Err(e) => format!("Failed to get knowledge base entries: {}", e),
                }
            },
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }
}

// Knowledge store implementation using semantic_search_client
pub struct KnowledgeStore {
    client: SemanticSearchClient,
}

impl KnowledgeStore {
    pub(crate) fn new() -> Result<Self> {
        match SemanticSearchClient::new_with_default_dir() {
            Ok(client) => Ok(Self { client }),
            Err(e) => Err(eyre::eyre!("Failed to create semantic search client: {}", e)),
        }
    }

    // Create a test instance with an isolated directory
    pub(crate) fn new_test_instance() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        match SemanticSearchClient::new(temp_dir.path()) {
            Ok(client) => Ok(Self { client }),
            Err(e) => Err(eyre::eyre!("Failed to create test semantic search client: {}", e)),
        }
    }

    // Singleton pattern for knowledge store with test mode support
    pub fn get_instance() -> Arc<Mutex<Self>> {
        static INSTANCE: Lazy<Arc<Mutex<KnowledgeStore>>> = Lazy::new(|| {
            Arc::new(Mutex::new(
                KnowledgeStore::new().expect("Failed to create knowledge store"),
            ))
        });

        // Check if we're running in a test environment
        if cfg!(test) {
            // For tests, create a new isolated instance each time
            Arc::new(Mutex::new(
                KnowledgeStore::new_test_instance().expect("Failed to create test knowledge store"),
            ))
        } else {
            // For normal operation, use the singleton
            INSTANCE.clone()
        }
    }

    pub fn add(&mut self, name: &str, value: &str) -> Result<String, String> {
        let path = PathBuf::from(value);

        if path.exists() {
            // Handle file or directory

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
            let progress_callback = move |status: semantic_search_client::types::ProgressStatus| {
                match status {
                    semantic_search_client::types::ProgressStatus::CountingFiles => {
                        pb.set_message("Counting files...");
                        pb.set_length(100);
                        pb.set_position(0);
                    },
                    semantic_search_client::types::ProgressStatus::StartingIndexing(total) => {
                        pb.set_message("Indexing files...");
                        pb.set_length(total as u64);
                        pb.set_position(0);
                    },
                    semantic_search_client::types::ProgressStatus::Indexing(current, total) => {
                        pb.set_message(format!("Indexing file {} of {}", current, total));
                        pb.set_position(current as u64);
                    },
                    semantic_search_client::types::ProgressStatus::Finalizing => {
                        pb.set_message("Finalizing index...");
                        if let Some(len) = pb.length() {
                            pb.set_position(len - 1);
                        }
                    },
                    semantic_search_client::types::ProgressStatus::Complete => {
                        pb.finish_with_message("Indexing complete!");
                        pb.println("✅ Successfully indexed all files and created knowledge context");
                    },
                    semantic_search_client::types::ProgressStatus::CreatingSemanticContext => {
                        pb.set_message("Creating semantic context...");
                    },
                    semantic_search_client::types::ProgressStatus::GeneratingEmbeddings(current, total) => {
                        pb.set_message(format!("Generating embeddings {} of {}", current, total));
                        pb.set_position(current as u64);
                    },
                    semantic_search_client::types::ProgressStatus::BuildingIndex => {
                        pb.set_message("Building vector index...");
                    },
                };
            };

            self.client
                .add_context_from_path(
                    path,
                    name,
                    &format!("Knowledge context for {}", name),
                    true,
                    Some(progress_callback),
                )
                .map_err(|e| e.to_string())
        } else {
            // Handle text content
            let preview: String = value.chars().take(40).collect();
            self.client
                .add_context_from_text(value, name, &format!("Text knowledge {}...", preview), true)
                .map_err(|e| e.to_string())
        }
    }

    pub fn update_context_by_id(&mut self, context_id: &str, path_str: &str) -> Result<(), String> {
        // First, check if the context exists
        let contexts = self.client.get_contexts();
        let context = contexts.iter().find(|c| c.id == context_id);

        if context.is_none() {
            return Err(format!("Context with ID '{}' not found", context_id));
        }

        let context = context.unwrap();
        let path = PathBuf::from(path_str);

        if !path.exists() {
            return Err(format!("Path '{}' does not exist", path_str));
        }

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
        let progress_callback = move |status: semantic_search_client::types::ProgressStatus| {
            match status {
                semantic_search_client::types::ProgressStatus::CountingFiles => {
                    pb.set_message("Counting files...");
                    pb.set_length(100);
                    pb.set_position(0);
                },
                semantic_search_client::types::ProgressStatus::StartingIndexing(total) => {
                    pb.set_message("Indexing files...");
                    pb.set_length(total as u64);
                    pb.set_position(0);
                },
                semantic_search_client::types::ProgressStatus::Indexing(current, total) => {
                    pb.set_message(format!("Indexing file {} of {}", current, total));
                    pb.set_position(current as u64);
                },
                semantic_search_client::types::ProgressStatus::Finalizing => {
                    pb.set_message("Finalizing index...");
                    if let Some(len) = pb.length() {
                        pb.set_position(len - 1);
                    }
                },
                semantic_search_client::types::ProgressStatus::Complete => {
                    pb.finish_with_message("Update complete!");
                    pb.println("✅ Successfully updated knowledge context");
                },
                semantic_search_client::types::ProgressStatus::CreatingSemanticContext => {
                    pb.set_message("Creating semantic context...");
                },
                semantic_search_client::types::ProgressStatus::GeneratingEmbeddings(current, total) => {
                    pb.set_message(format!("Generating embeddings {} of {}", current, total));
                    pb.set_position(current as u64);
                },
                semantic_search_client::types::ProgressStatus::BuildingIndex => {
                    pb.set_message("Building vector index...");
                },
            };
        };

        // First remove the existing context
        if let Err(e) = self.client.remove_context_by_id(context_id, true) {
            return Err(format!("Failed to remove existing context: {}", e));
        }

        // Then add a new context with the same ID but new content
        // Since we can't directly control the ID generation in add_context_from_path,
        // we'll need to add the context and then update its metadata
        let result =
            self.client
                .add_context_from_path(path, &context.name, &context.description, true, Some(progress_callback));

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to update context: {}", e)),
        }
    }

    pub fn update_context_by_name(&mut self, name: &str, path_str: &str) -> Result<(), String> {
        // Find the context ID by name
        let contexts = self.client.get_contexts();
        let context = contexts.iter().find(|c| c.name == name);

        if let Some(context) = context {
            self.update_context_by_id(&context.id, path_str)
        } else {
            Err(format!("Context with name '{}' not found", name))
        }
    }

    pub fn remove_by_id(&mut self, id: &str) -> Result<(), String> {
        self.client.remove_context_by_id(id, true).map_err(|e| e.to_string())
    }

    pub fn remove_by_name(&mut self, name: &str) -> Result<(), String> {
        self.client
            .remove_context_by_name(name, true)
            .map_err(|e| e.to_string())
    }

    pub fn remove_by_path(&mut self, path: &str) -> Result<(), String> {
        self.client
            .remove_context_by_path(path, true)
            .map_err(|e| e.to_string())
    }

    pub fn update_by_path(&mut self, path_str: &str) -> Result<(), String> {
        // Find contexts that might match this path
        let contexts = self.client.get_contexts();
        let matching_context = contexts.iter().find(|c| {
            if let Some(source_path) = &c.source_path {
                source_path == path_str
            } else {
                false
            }
        });

        if let Some(context) = matching_context {
            // Found a matching context, update it
            self.update_context_by_id(&context.id, path_str)
        } else {
            // No matching context found
            Err(format!("No context found with path '{}'", path_str))
        }
    }

    pub fn clear(&mut self) -> Result<usize, String> {
        let contexts = self.client.get_contexts();
        let count = contexts.len();

        for context in contexts {
            if let Err(e) = self.client.remove_context_by_id(&context.id, true) {
                tracing::warn!("Failed to remove context {}: {}", context.id, e);
            }
        }

        Ok(count)
    }

    pub fn search(&self, query: &str, context_id: Option<&str>) -> Result<Vec<SearchResult>, String> {
        if let Some(id) = context_id {
            self.client.search_context(id, query, None).map_err(|e| e.to_string())
        } else {
            let results = self.client.search_all(query, None).map_err(|e| e.to_string())?;

            // Flatten results from all contexts
            let mut flattened = Vec::new();
            for (_, context_results) in results {
                flattened.extend(context_results);
            }

            // Sort by distance (lower is better)
            flattened.sort_by(|a, b| {
                let a_dist = a.distance;
                let b_dist = b.distance;
                a_dist.partial_cmp(&b_dist).unwrap_or(std::cmp::Ordering::Equal)
            });

            Ok(flattened)
        }
    }

    pub fn get_all(&self) -> Result<Vec<MemoryContext>, String> {
        Ok(self.client.get_contexts())
    }
}
