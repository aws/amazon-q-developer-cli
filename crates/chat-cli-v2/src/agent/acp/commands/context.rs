//! /context command execution

use agent::agent_loop::types::{
    ContentBlock,
    Role,
    ToolResultContentBlock,
};
use agent::tui_commands::{
    CommandResult,
    ContextArgs,
};
use agent::types::AgentSnapshot;
use agent::util::steering::{
    is_steering_file,
    should_include_steering_file,
};
use serde_json::json;

use super::CommandContext;

const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 200_000;

pub async fn execute(args: &ContextArgs, ctx: &CommandContext<'_>) -> CommandResult {
    // Default behavior - show context usage
    let model = ctx.rts_state.model_id().unwrap_or_else(|| "default".to_string());
    let backend_usage = ctx.rts_state.context_usage_percentage();
    let context_window = ctx
        .rts_state
        .model_info()
        .map_or(DEFAULT_CONTEXT_WINDOW_TOKENS, |m| m.context_window_tokens);

    let snapshot = match ctx.agent.create_snapshot().await {
        Ok(s) => s,
        Err(e) => return CommandResult::error(format!("Failed to get context breakdown: {}", e)),
    };

    let (breakdown, estimated_usage) = calculate_context_breakdown(&snapshot, backend_usage, context_window);

    // Use backend usage if available, otherwise use our estimate
    let context_usage = backend_usage.unwrap_or(estimated_usage);

    CommandResult::success_with_data(
        format!("Context breakdown - {}% used", context_usage as u32),
        json!({
            "model": model,
            "contextUsagePercentage": context_usage,
            "verbose": args.verbose,
            "breakdown": breakdown
        }),
    )
}

// ============================================================================
// Breakdown calculation
// ============================================================================

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextBreakdown {
    context_files: CategoryBreakdown,
    tools: CategoryBreakdown,
    kiro_responses: CategoryBreakdown,
    your_prompts: CategoryBreakdown,
    session_files: CategoryBreakdown,
}

#[derive(serde::Serialize)]
struct CategoryBreakdown {
    tokens: usize,
    #[serde(rename = "percent")]
    percentage: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    items: Vec<BreakdownItem>,
}

#[derive(serde::Serialize)]
pub struct BreakdownItem {
    name: String,
    tokens: usize,
    matched: bool,
    percent: f32,
}

fn calculate_context_breakdown(
    snapshot: &AgentSnapshot,
    _context_usage_percent: Option<f32>,
    context_window_tokens: usize,
) -> (ContextBreakdown, f32) {
    let mut sizes = calculate_component_sizes(snapshot);

    let total_tokens = sizes.context_files + sizes.tools + sizes.kiro + sizes.user;

    // Calculate our estimate
    let estimated_usage = (total_tokens as f32 / context_window_tokens as f32) * 100.0;

    let context_window_tokens_f = context_window_tokens as f32;

    // Calculate percent for each context file item
    for item in &mut sizes.context_file_items {
        item.percent = (item.tokens as f32 / context_window_tokens_f) * 100.0;
    }

    let breakdown = ContextBreakdown {
        context_files: CategoryBreakdown {
            tokens: sizes.context_files,
            percentage: (sizes.context_files as f32 / context_window_tokens_f) * 100.0,
            items: sizes.context_file_items,
        },
        tools: CategoryBreakdown {
            tokens: sizes.tools,
            percentage: (sizes.tools as f32 / context_window_tokens_f) * 100.0,
            items: vec![],
        },
        kiro_responses: CategoryBreakdown {
            tokens: sizes.kiro,
            percentage: (sizes.kiro as f32 / context_window_tokens_f) * 100.0,
            items: vec![],
        },
        your_prompts: CategoryBreakdown {
            tokens: sizes.user,
            percentage: (sizes.user as f32 / context_window_tokens_f) * 100.0,
            items: vec![],
        },
        session_files: CategoryBreakdown {
            tokens: 0,
            percentage: 0.0,
            items: vec![],
        },
    };

    (breakdown, estimated_usage)
}

// ============================================================================
// Size calculation utilities
// ============================================================================

/// Component sizes in tokens for context breakdown
pub struct ComponentSizes {
    pub context_files: usize,
    pub context_file_items: Vec<BreakdownItem>,
    pub tools: usize,
    pub kiro: usize,
    pub user: usize,
}

pub fn calculate_component_sizes(snapshot: &AgentSnapshot) -> ComponentSizes {
    let (context_files, context_file_items) = calculate_context_files_tokens(snapshot);
    ComponentSizes {
        context_files,
        context_file_items,
        tools: calculate_tools_tokens(snapshot),
        kiro: calculate_message_tokens(snapshot, Role::Assistant),
        user: calculate_message_tokens(snapshot, Role::User),
    }
}

fn calculate_context_files_tokens(snapshot: &AgentSnapshot) -> (usize, Vec<BreakdownItem>) {
    let resources = snapshot.agent_config.resources();
    if resources.is_empty() {
        return (0, vec![]);
    }

    let mut items = Vec::new();
    let mut total = 0;

    for r in resources {
        let path_str = r.as_ref();
        let path = path_str.strip_prefix("file://").unwrap_or(path_str);

        // Expand ~ in paths
        let expanded_path = if path.starts_with('~') {
            if let Ok(home_dir) = std::env::var("HOME") {
                path.replacen('~', &home_dir, 1)
            } else {
                path.to_string()
            }
        } else {
            path.to_string()
        };

        // Handle glob patterns
        if expanded_path.contains('*') || expanded_path.contains('?') || expanded_path.contains('[') {
            match glob::glob(&expanded_path) {
                Ok(entries) => {
                    for file_path in entries.flatten() {
                        if file_path.is_file() {
                            let path_str = file_path.to_string_lossy().to_string();
                            let (tokens, matched) = calculate_file_tokens(&path_str);
                            if matched {
                                total += tokens;
                                items.push(BreakdownItem {
                                    name: path_str,
                                    tokens,
                                    matched,
                                    percent: 0.0,
                                });
                            }
                        }
                    }
                },
                Err(_) => {
                    // Invalid glob pattern - add as unmatched
                    items.push(BreakdownItem {
                        name: expanded_path,
                        tokens: 0,
                        matched: false,
                        percent: 0.0,
                    });
                },
            }
        } else {
            // Regular file path
            let (tokens, matched) = calculate_file_tokens(&expanded_path);
            total += tokens;
            items.push(BreakdownItem {
                name: expanded_path,
                tokens,
                matched,
                percent: 0.0,
            });
        }
    }
    (total, items)
}

fn calculate_file_tokens(path: &str) -> (usize, bool) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            // Apply steering file filtering
            if is_steering_file(path) {
                if should_include_steering_file(&content) {
                    (content.len() / 4, true)
                } else {
                    (0, false) // Excluded by frontmatter
                }
            } else {
                (content.len() / 4, true)
            }
        },
        Err(_) => (0, false),
    }
}

fn calculate_tools_tokens(snapshot: &AgentSnapshot) -> usize {
    // Use actual tool specs - no fallback estimates
    if !snapshot.tool_specs.is_empty() {
        let specs_json = serde_json::to_string(&snapshot.tool_specs).unwrap_or_default();
        return specs_json.len() / 4;
    }
    0
}

fn calculate_message_tokens(snapshot: &AgentSnapshot, role: Role) -> usize {
    snapshot
        .conversation_state
        .cached_messages()
        .unwrap_or(&[])
        .iter()
        .filter(|msg| msg.role == role)
        .map(|msg| msg.content.iter().map(estimate_content_size).sum::<usize>() / 4)
        .sum()
}

fn estimate_content_size(content: &ContentBlock) -> usize {
    match content {
        ContentBlock::Text(t) => t.len(),
        ContentBlock::Image(_) => 1000, // rough estimate
        ContentBlock::ToolUse(t) => serde_json::to_string(&t.input).unwrap_or_default().len(),
        ContentBlock::ToolResult(t) => t
            .content
            .iter()
            .map(|c| match c {
                ToolResultContentBlock::Text(s) => s.len(),
                ToolResultContentBlock::Json(v) => v.to_string().len(),
                ToolResultContentBlock::Image(_) => 1000,
            })
            .sum(),
    }
}

#[cfg(test)]
mod tests {
    use agent::tui_commands::ContextArgs;

    #[test]
    fn test_context_args_default() {
        let args = ContextArgs::default();
        assert!(!args.verbose);
    }
}
