//! /knowledge command execution — manage knowledge base

use std::path::PathBuf;

use agent::agent_config::ConfigSource;
use agent::tui_commands::CommandResult;
use serde_json::json;

use super::CommandContext;
use crate::util::knowledge_store::{
    AddOptions,
    KnowledgeStore,
};

/// Resolve the current agent's name and config path for KnowledgeStore
fn resolve_agent(ctx: &CommandContext<'_>) -> (Option<String>, Option<PathBuf>) {
    let Some(config) = ctx.agent_configs.iter().find(|c| c.name() == ctx.current_agent_name) else {
        return (None, None);
    };
    let path = match config.source() {
        ConfigSource::Workspace { path } | ConfigSource::Global { path } => Some(path.clone()),
        _ => None,
    };
    (Some(config.name().to_string()), path)
}

pub async fn execute(args: &agent::tui_commands::KnowledgeArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let (agent_name, agent_path) = resolve_agent(ctx);
    let store = match KnowledgeStore::get_async_instance(ctx.os, agent_name.as_deref(), agent_path.as_deref()).await {
        Ok(s) => s,
        Err(e) => return CommandResult::error(format!("Knowledge base unavailable: {e}")),
    };

    let sub = args.subcommand.as_deref().unwrap_or("show");
    let (cmd, rest) = sub.split_once(' ').unwrap_or((sub, ""));
    match cmd {
        "show" => {
            let store = store.lock().await;
            format_show(&store).await
        },
        "add" => {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
                return CommandResult::error("Usage: /knowledge add <name> <path>");
            }
            let mut store = store.lock().await;
            match store.add(parts[0], parts[1], AddOptions::new()).await {
                Ok(_) => CommandResult::success(format!("Indexing '{}' started", parts[0])),
                Err(e) => CommandResult::error(format!("Failed to add: {e}")),
            }
        },
        "remove" | "rm" => {
            let target = rest.trim();
            if target.is_empty() {
                return CommandResult::error("Usage: /knowledge remove <name|path>");
            }
            let mut store = store.lock().await;
            if store.remove_by_path(target).await.is_ok() || store.remove_by_name(target).await.is_ok() {
                CommandResult::success(format!("Removed '{target}'"))
            } else {
                CommandResult::error(format!("Entry not found: {target}"))
            }
        },
        "update" => {
            let path = rest.trim();
            if path.is_empty() {
                return CommandResult::error("Usage: /knowledge update <path>");
            }
            let mut store = store.lock().await;
            match store.update_by_path(path).await {
                Ok(msg) => CommandResult::success(msg),
                Err(e) => CommandResult::error(format!("Failed to update: {e}")),
            }
        },
        "clear" => {
            let mut store = store.lock().await;
            let _ = store.cancel_operation(None).await;
            match store.clear_immediate().await {
                Ok(msg) => CommandResult::success(msg),
                Err(e) => CommandResult::error(e),
            }
        },
        "cancel" => {
            let op_id = if rest.is_empty() { None } else { Some(rest.trim()) };
            let mut store = store.lock().await;
            match store.cancel_operation(op_id).await {
                Ok(msg) => CommandResult::success(msg.replace("✅ ", "")),
                Err(e) => CommandResult::error(e),
            }
        },
        other => CommandResult::error(format!(
            "Unknown subcommand '{other}'. Available: show, add, remove, update, clear, cancel"
        )),
    }
}

async fn format_show(store: &KnowledgeStore) -> CommandResult {
    let contexts = store.get_all().await.unwrap_or_default();
    let status = store.get_status_data().await;

    let entries: Vec<serde_json::Value> = contexts
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "id": &c.id[..8],
                "description": c.description,
                "item_count": c.item_count,
                "path": c.source_path,
            })
        })
        .collect();

    let has_operations = status.as_ref().is_ok_and(|s| !s.operations.is_empty());

    let message = if entries.is_empty() && !has_operations {
        "No knowledge base entries".to_string()
    } else {
        let mut parts = vec![];
        if !entries.is_empty() {
            parts.push(format!(
                "{} entr{}",
                entries.len(),
                if entries.len() == 1 { "y" } else { "ies" }
            ));
        }
        if has_operations {
            parts.push("indexing in progress".to_string());
        }
        parts.join(" · ")
    };

    let mut data = json!({ "entries": entries, "message": message });

    if let Ok(s) = status
        && !s.operations.is_empty()
    {
        data["status"] = json!(format_status_display(&s));
    }

    CommandResult::success_with_data(&message, data)
}

fn format_status_display(status: &semantic_search_client::SystemStatus) -> String {
    let mut out = String::new();
    for op in &status.operations {
        let desc = match &op.operation_type {
            semantic_search_client::OperationType::Indexing { path, .. } => path.clone(),
            semantic_search_client::OperationType::Clearing => op.message.clone(),
        };
        out.push_str(&format!(
            "{} ({}) — {desc}",
            op.operation_type.display_name(),
            &op.short_id
        ));
        if op.is_cancelled {
            out.push_str(" [Cancelled]");
        } else if op.is_failed {
            out.push_str(" [Failed]");
        } else if op.total > 0 {
            let pct = (op.current as f64 / op.total as f64 * 100.0) as u8;
            if let Some(eta) = op.eta {
                out.push_str(&format!(" [{pct}% · ETA: {}s]", eta.as_secs()));
            } else {
                out.push_str(&format!(" [{pct}%]"));
            }
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}
