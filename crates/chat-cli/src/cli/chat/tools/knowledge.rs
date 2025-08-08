use std::io::Write;

use crossterm::queue;
use crossterm::style;
use eyre::Result;
use serde::Deserialize;
use tracing::warn;

use super::{
    InvokeOutput,
    OutputKind,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::colors::ColorManager;
use crate::database::settings::{Setting, Settings};
use crate::os::Os;
use crate::util::knowledge_store::KnowledgeStore;
use crate::{with_success, with_info, with_warning, with_color};

/// The Knowledge tool allows storing and retrieving information across chat sessions.
/// It provides semantic search capabilities for files, directories, and text content.
///
/// This feature can be enabled/disabled via settings:
/// `q settings chat.enableKnowledge true`
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command", rename_all = "lowercase")]
pub enum Knowledge {
    Add(KnowledgeAdd),
    Remove(KnowledgeRemove),
    Clear(KnowledgeClear),
    Search(KnowledgeSearch),
    Update(KnowledgeUpdate),
    Show,
    /// Show background operation status
    Status,
    /// Cancel a background operation
    Cancel(KnowledgeCancel),
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

#[derive(Debug, Clone, Deserialize)]
pub struct KnowledgeCancel {
    /// Operation ID to cancel, or "all" to cancel all operations
    pub operation_id: String,
}

impl Knowledge {
    /// Checks if the knowledge feature is enabled in settings
    pub fn is_enabled(os: &Os) -> bool {
        os.database
            .settings
            .get_bool(Setting::EnabledKnowledge)
            .unwrap_or(false)
    }

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        match self {
            Knowledge::Add(add) => {
                // Check if value is intended to be a path (doesn't contain newlines)
                if !add.value.contains('\n') {
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &add.value);
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
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &remove.path);
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
                    let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &update.path);
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
            Knowledge::Status => Ok(()),
            Knowledge::Cancel(_) => Ok(()),
        }
    }

    pub async fn queue_description(&self, os: &Os, updates: &mut impl Write) -> Result<()> {
        let settings = Settings::default();
        let color_manager = ColorManager::from_settings(&settings);

        match self {
            Knowledge::Add(add) => {
                queue!(updates, style::Print("Adding to knowledge base: "))?;
                with_success!(updates, &color_manager, "{}", &add.name)?;

                // Check if value is a path or text content
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &add.value);
                if path.exists() {
                    let path_type = if path.is_dir() { "directory" } else { "file" };
                    queue!(updates, style::Print(format!(" ({}: ", path_type)))?;
                    with_success!(updates, &color_manager, "{}", &add.value)?;
                    queue!(updates, style::Print(")\n"))?;
                } else {
                    let preview: String = add.value.chars().take(20).collect();
                    if add.value.len() > 20 {
                        queue!(updates, style::Print(" (text: "))?;
                        with_info!(updates, &color_manager, "{}...", preview)?;
                        queue!(updates, style::Print(")\n"))?;
                    } else {
                        queue!(updates, style::Print(" (text: "))?;
                        with_info!(updates, &color_manager, "{}", &add.value)?;
                        queue!(updates, style::Print(")\n"))?;
                    }
                }
            },
            Knowledge::Remove(remove) => {
                if !remove.name.is_empty() {
                    queue!(updates, style::Print("Removing from knowledge base by name: "))?;
                    with_success!(updates, &color_manager, "{}", &remove.name)?;
                } else if !remove.context_id.is_empty() {
                    queue!(updates, style::Print("Removing from knowledge base by ID: "))?;
                    with_success!(updates, &color_manager, "{}", &remove.context_id)?;
                } else if !remove.path.is_empty() {
                    queue!(updates, style::Print("Removing from knowledge base by path: "))?;
                    with_success!(updates, &color_manager, "{}", &remove.path)?;
                } else {
                    queue!(updates, style::Print("Removing from knowledge base: "))?;
                    with_warning!(updates, &color_manager, "No identifier provided")?;
                }
            },
            Knowledge::Update(update) => {
                queue!(updates, style::Print("Updating knowledge base context"),)?;

                if !update.context_id.is_empty() {
                    queue!(
                        updates,
                        style::Print(" with ID: "),
                    )?;
                    with_success!(updates, &color_manager, "{}", &update.context_id)?;
                } else if !update.name.is_empty() {
                    queue!(updates, style::Print(" with name: "))?;
                    with_success!(updates, &color_manager, "{}", &update.name)?;
                }

                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &update.path);
                let path_type = if path.is_dir() { "directory" } else { "file" };
                queue!(updates, style::Print(format!(" using new {}: ", path_type)))?;
                with_success!(updates, &color_manager, "{}", &update.path)?;
                queue!(updates, style::Print("\n"))?;
            },
            Knowledge::Clear(_) => {
                queue!(updates, style::Print("Clearing "))?;
                with_warning!(updates, &color_manager, "all")?;
                queue!(updates, style::Print(" knowledge base entries"))?;
            },
            Knowledge::Search(search) => {
                queue!(updates, style::Print("Searching knowledge base for: "))?;
                with_success!(updates, &color_manager, "{}", &search.query)?;

                if let Some(context_id) = &search.context_id {
                    queue!(updates, style::Print(" in context: "))?;
                    with_success!(updates, &color_manager, "{}", context_id)?;
                } else {
                    queue!(updates, style::Print(" across all contexts"))?;
                }
            },
            Knowledge::Show => {
                queue!(updates, style::Print("Showing all knowledge base entries"),)?;
            },
            Knowledge::Status => {
                queue!(updates, style::Print("Checking background operation status"),)?;
            },
            Knowledge::Cancel(cancel) => {
                queue!(
                    updates,
                    style::Print(&format!("Cancelling operation: {}", cancel.operation_id)),
                )?;
            },
        };
        Ok(())
    }

    pub async fn invoke(&self, os: &Os, _updates: &mut impl Write) -> Result<InvokeOutput> {
        // Get the async knowledge store singleton
        let async_knowledge_store = KnowledgeStore::get_async_instance().await;
        let mut store = async_knowledge_store.lock().await;

        let result = match self {
            Knowledge::Add(add) => {
                // For path indexing, we'll show a progress message first
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &add.value);
                let value_to_use = if path.exists() {
                    path.to_string_lossy().to_string()
                } else {
                    // If it's not a valid path, use the original value (might be text content)
                    add.value.clone()
                };

                match store.add(&add.name, &value_to_use).await {
                    Ok(context_id) => format!(
                        "Added '{}' to knowledge base with ID: {}. Track active jobs in '/knowledge status' with provided id.",
                        add.name, context_id
                    ),
                    Err(e) => format!("Failed to add to knowledge base: {}", e),
                }
            },
            Knowledge::Remove(remove) => {
                if !remove.context_id.is_empty() {
                    // Remove by ID
                    match store.remove_by_id(&remove.context_id).await {
                        Ok(_) => format!("Removed context with ID '{}' from knowledge base", remove.context_id),
                        Err(e) => format!("Failed to remove context by ID: {}", e),
                    }
                } else if !remove.name.is_empty() {
                    // Remove by name
                    match store.remove_by_name(&remove.name).await {
                        Ok(_) => format!("Removed context with name '{}' from knowledge base", remove.name),
                        Err(e) => format!("Failed to remove context by name: {}", e),
                    }
                } else if !remove.path.is_empty() {
                    // Remove by path
                    let sanitized_path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &remove.path);
                    match store.remove_by_path(sanitized_path.to_string_lossy().as_ref()).await {
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
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &update.path);
                if !path.exists() {
                    return Ok(InvokeOutput {
                        output: OutputKind::Text(format!("Error: Path '{}' does not exist", update.path)),
                    });
                }

                let sanitized_path = path.to_string_lossy().to_string();

                // Choose the appropriate update method based on provided identifiers
                if !update.context_id.is_empty() {
                    // Update by ID
                    match store.update_context_by_id(&update.context_id, &sanitized_path).await {
                        Ok(_) => format!(
                            "Updated context with ID '{}' using path '{}'.  Track active jobs in '/knowledge status' with provided id.",
                            update.context_id, update.path
                        ),
                        Err(e) => format!("Failed to update context by ID: {}", e),
                    }
                } else if !update.name.is_empty() {
                    // Update by name
                    match store.update_context_by_name(&update.name, &sanitized_path).await {
                        Ok(_) => format!(
                            "Updated context with name '{}' using path '{}'. Track active jobs in '/knowledge status' with provided id.",
                            update.name, update.path
                        ),
                        Err(e) => format!("Failed to update context by name: {}", e),
                    }
                } else {
                    // Update by path (if no ID or name provided)
                    match store.update_by_path(&sanitized_path).await {
                        Ok(_) => format!(
                            "Updated context with path '{}'. Track active jobs in '/knowledge status' with provided id.",
                            update.path
                        ),
                        Err(e) => format!("Failed to update context by path: {}", e),
                    }
                }
            },
            Knowledge::Clear(_) => store
                .clear()
                .await
                .unwrap_or_else(|e| format!("Failed to clear knowledge base: {}", e)),
            Knowledge::Search(search) => {
                // Only use a spinner for search, not a full progress bar
                let results = store.search(&search.query, search.context_id.as_deref()).await;
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
                let contexts = store.get_all().await;
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
            Knowledge::Status => {
                match store.get_status_data().await {
                    Ok(status_data) => {
                        // Format the status data for display (same logic as knowledge command)
                        Self::format_status_display(&status_data)
                    },
                    Err(e) => format!("Failed to get status: {}", e),
                }
            },
            Knowledge::Cancel(cancel) => store
                .cancel_operation(Some(&cancel.operation_id))
                .await
                .unwrap_or_else(|e| format!("Failed to cancel operation: {}", e)),
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    pub fn eval_perm(&self, agent: &Agent) -> PermissionEvalResult {
        _ = self;
        if agent.allowed_tools.contains("knowledge") {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }
    }

    /// Format status data for display (UI rendering responsibility)
    fn format_status_display(status: &semantic_search_client::SystemStatus) -> String {
        let mut status_lines = Vec::new();

        // Show context summary
        status_lines.push(format!(
            "Total contexts: {} ({} persistent, {} volatile)",
            status.total_contexts, status.persistent_contexts, status.volatile_contexts
        ));

        if status.operations.is_empty() {
            status_lines.push("No active operations".to_string());
            return status_lines.join("\n");
        }

        status_lines.push("Active Operations:".to_string());
        status_lines.push(format!(
            "Queue Status: {} active, {} waiting (max {} concurrent)",
            status.active_count, status.waiting_count, status.max_concurrent
        ));

        for op in &status.operations {
            let formatted_operation = Self::format_operation_display(op);
            status_lines.push(formatted_operation);
        }

        status_lines.join("\n")
    }

    /// Format a single operation for display (LLM-friendly data format)
    fn format_operation_display(op: &semantic_search_client::OperationStatus) -> String {
        let elapsed = op.started_at.elapsed().unwrap_or_default();

        let status_info = if op.is_cancelled {
            "Status: Cancelled".to_string()
        } else if op.is_failed {
            format!("Status: Failed - {}", op.message)
        } else if op.is_waiting {
            format!("Status: Waiting - {}", op.message)
        } else if op.total > 0 {
            let percentage = (op.current as f64 / op.total as f64 * 100.0) as u8;
            format!(
                "Status: In Progress - {}% ({}/{}) - {}",
                percentage, op.current, op.total, op.message
            )
        } else {
            format!("Status: In Progress - {}", op.message)
        };

        let operation_desc = op.operation_type.display_name();

        // Format with conditional elapsed time and ETA
        if op.is_cancelled || op.is_failed {
            format!(
                "Operation ID: {} | Type: {} | {}",
                op.short_id, operation_desc, status_info
            )
        } else {
            let mut time_info = format!("Elapsed: {}s", elapsed.as_secs());

            if let Some(eta) = op.eta {
                time_info.push_str(&format!(" | ETA: {}s", eta.as_secs()));
            }

            format!(
                "Operation ID: {} | Type: {} | {} | {}",
                op.short_id, operation_desc, status_info, time_info
            )
        }
    }
}
