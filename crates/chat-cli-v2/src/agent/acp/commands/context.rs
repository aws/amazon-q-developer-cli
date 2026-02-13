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
use serde_json::json;

use super::CommandContext;

const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 200_000;

pub async fn execute(args: &ContextArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let model = ctx.rts_state.model_id().unwrap_or_else(|| "default".to_string());
    let backend_usage = ctx.rts_state.context_usage_percentage();
    let context_window = ctx
        .rts_state
        .model_info()
        .map(|m| m.context_window_tokens)
        .unwrap_or(DEFAULT_CONTEXT_WINDOW_TOKENS);

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
struct BreakdownItem {
    name: String,
    tokens: usize,
}

fn calculate_context_breakdown(
    snapshot: &AgentSnapshot,
    context_usage_percent: Option<f32>,
    context_window_tokens: usize,
) -> (ContextBreakdown, f32) {
    let sizes = calculate_component_sizes(snapshot);
    let total_tokens = sizes.context_files + sizes.tools + sizes.kiro + sizes.user;

    // Calculate our estimate
    let estimated_usage = (total_tokens as f32 / context_window_tokens as f32) * 100.0;

    // Use backend percentage if available, otherwise use estimate
    let used_percent = context_usage_percent.unwrap_or(estimated_usage);

    // Each category's percentage of the TOTAL context window
    let total_f = total_tokens.max(1) as f32;

    let breakdown = ContextBreakdown {
        context_files: CategoryBreakdown {
            tokens: sizes.context_files,
            percentage: (sizes.context_files as f32 / total_f) * used_percent,
            items: vec![],
        },
        tools: CategoryBreakdown {
            tokens: sizes.tools,
            percentage: (sizes.tools as f32 / total_f) * used_percent,
            items: vec![],
        },
        kiro_responses: CategoryBreakdown {
            tokens: sizes.kiro,
            percentage: (sizes.kiro as f32 / total_f) * used_percent,
            items: vec![],
        },
        your_prompts: CategoryBreakdown {
            tokens: sizes.user,
            percentage: (sizes.user as f32 / total_f) * used_percent,
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
    pub tools: usize,
    pub kiro: usize,
    pub user: usize,
}

pub fn calculate_component_sizes(snapshot: &AgentSnapshot) -> ComponentSizes {
    ComponentSizes {
        context_files: calculate_context_files_tokens(snapshot),
        tools: calculate_tools_tokens(snapshot),
        kiro: calculate_message_tokens(snapshot, Role::Assistant),
        user: calculate_message_tokens(snapshot, Role::User),
    }
}

fn calculate_context_files_tokens(snapshot: &AgentSnapshot) -> usize {
    let resources = snapshot.agent_config.resources();
    if resources.is_empty() {
        return 0;
    }

    resources
        .iter()
        .map(|r| {
            let path_str = r.as_ref();
            let path = path_str.strip_prefix("file://").unwrap_or(path_str);
            std::fs::metadata(path).map(|m| m.len() as usize / 4).unwrap_or(0)
        })
        .sum()
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
