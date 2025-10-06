use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
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
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::os::Os;
use crate::util::knowledge_store::KnowledgeStore;
use crate::util::tool_permission_checker::is_tool_in_allowlist;

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
        ExperimentManager::is_enabled(os, ExperimentName::Knowledge)
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
                    eyre::bail!(
                        "Please provide either context_id, name, or path to identify the knowledge base entry to update"
                    );
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
            Knowledge::Cancel(_) => Ok(()),
        }
    }

    pub async fn queue_description(&self, os: &Os, updates: &mut impl Write) -> Result<()> {
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
                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &add.value);
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

                let path = crate::cli::chat::tools::sanitize_path_tool_arg(os, &update.path);
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
                queue!(
                    updates,
                    style::Print("Showing all knowledge base entries and background operations"),
                )?;
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

    pub async fn invoke(
        &self,
        os: &Os,
        _updates: &mut impl Write,
        agent: Option<&crate::cli::Agent>,
    ) -> Result<InvokeOutput> {
        let async_knowledge_store = KnowledgeStore::get_async_instance(os, agent)
            .await
            .map_err(|e| eyre::eyre!("Failed to access knowledge base: {}", e))?;
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

                match store
                    .add(
                        &add.name,
                        &value_to_use,
                        crate::util::knowledge_store::AddOptions::with_db_defaults(os),
                    )
                    .await
                {
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
                let results = store.search(&search.query, search.context_id.as_deref()).await;
                match results {
                    Ok(results) => {
                        if results.is_empty() {
                            format!("No matching entries found for query: \"{}\"", search.query)
                        } else {
                            let mut output = format!("Search results for \"{}\":\n\n", search.query);
                            for result in results {
                                if let Some(text) = result.text() {
                                    output.push_str(&format!("{}\n\n", text));
                                }
                            }
                            output
                        }
                    },
                    Err(e) => {
                        format!("Search failed: {}", e)
                    },
                }
            },
            Knowledge::Show => {
                // Get both contexts and status data
                let contexts_result = store.get_all().await;
                let status_result = store.get_status_data().await;

                match (contexts_result, status_result) {
                    (Ok(contexts), Ok(status_data)) => {
                        let mut output = String::new();

                        // Add contexts section
                        if contexts.is_empty() {
                            output.push_str("No knowledge base entries found\n");
                        } else {
                            output.push_str("Knowledge base entries:\n");
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
                        }

                        // Add status section
                        output.push('\n');
                        output.push_str(&Self::format_status_display(&status_data));

                        output
                    },
                    (Ok(contexts), Err(e)) => {
                        // Show contexts but note status error
                        let mut output = String::new();
                        if contexts.is_empty() {
                            output.push_str("No knowledge base entries found\n");
                        } else {
                            output.push_str("Knowledge base entries:\n");
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
                        }
                        output.push_str(&format!("\nStatus unavailable: {}", e));
                        output
                    },
                    (Err(e), Ok(status_data)) => {
                        // Show status but note contexts error
                        let mut output = format!("Contexts unavailable: {}\n\n", e);
                        output.push_str(&Self::format_status_display(&status_data));
                        output
                    },
                    (Err(contexts_err), Err(status_err)) => {
                        format!(
                            "Failed to get contexts: {}\nFailed to get status: {}",
                            contexts_err, status_err
                        )
                    },
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

    pub fn eval_perm(&self, os: &Os, agent: &Agent) -> PermissionEvalResult {
        _ = self;
        _ = os;

        if is_tool_in_allowlist(&agent.allowed_tools, "knowledge", None) {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }
    }

    /// Format status data for display (UI rendering responsibility)
    fn format_status_display(status: &semantic_search_client::SystemStatus) -> String {
        if status.operations.is_empty() {
            return "No active operations".to_string();
        }

        let mut output = String::new();
        for op in &status.operations {
            let operation_desc = op.operation_type.display_name();

            // Main entry line with operation name and ID (like knowledge entries)
            output.push_str(&format!("🔄 {} ({})\n", operation_desc, &op.short_id));

            // Description/path line (indented like knowledge entries)
            // Use actual path from operation type if available, otherwise use message
            let description = match &op.operation_type {
                semantic_search_client::OperationType::Indexing { path, .. } => path.clone(),
                semantic_search_client::OperationType::Clearing => op.message.clone(),
            };
            output.push_str(&format!("   {}\n", description));

            // Status/progress line with ETA if available
            if op.is_cancelled {
                output.push_str("   Cancelled\n");
            } else if op.is_failed {
                output.push_str("   Failed\n");
            } else if op.is_waiting {
                output.push_str("   Waiting\n");
            } else if op.total > 0 {
                let percentage = (op.current as f64 / op.total as f64 * 100.0) as u8;
                if let Some(eta) = op.eta {
                    output.push_str(&format!("   {}% • ETA: {}s\n", percentage, eta.as_secs()));
                } else {
                    output.push_str(&format!("   {}%\n", percentage));
                }
            } else {
                output.push_str("   In progress\n");
            }
        }

        output.trim_end().to_string() // Remove trailing newline
    }
}
