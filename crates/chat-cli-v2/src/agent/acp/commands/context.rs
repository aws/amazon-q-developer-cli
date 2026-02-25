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

pub const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 200_000;

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
    backend_usage_percent: Option<f32>,
    context_window_tokens: usize,
) -> (ContextBreakdown, f32) {
    let mut sizes = calculate_component_sizes(snapshot);

    let total_tokens = sizes.context_files + sizes.tools + sizes.kiro + sizes.user;

    // Calculate our estimate
    let estimated_usage = (total_tokens as f32 / context_window_tokens as f32) * 100.0;
    let cw = context_window_tokens as f32;

    let context_files_pct = (sizes.context_files as f32 / cw) * 100.0;
    let tools_pct = (sizes.tools as f32 / cw) * 100.0;
    let kiro_pct = (sizes.kiro as f32 / cw) * 100.0;
    let user_pct = (sizes.user as f32 / cw) * 100.0;

    // Calculate percentages - use backend-scaled if available, otherwise use estimates
    let (context_files_final, tools_final, kiro_final, user_final) = if let Some(backend_pct) = backend_usage_percent {
        // Adjust components: keep context_files/tools stable, fill remaining to kiro+user
        adjust_component_percentages(context_files_pct, tools_pct, kiro_pct, user_pct, backend_pct)
    } else {
        // Fall back to estimates when backend value not available
        (context_files_pct, tools_pct, kiro_pct, user_pct)
    };

    for item in &mut sizes.context_file_items {
        item.percent = (item.tokens as f32 / cw) * 100.0;
    }

    let breakdown = ContextBreakdown {
        context_files: CategoryBreakdown {
            tokens: sizes.context_files,
            percentage: context_files_final,
            items: sizes.context_file_items,
        },
        tools: CategoryBreakdown {
            tokens: sizes.tools,
            percentage: tools_final,
            items: vec![],
        },
        kiro_responses: CategoryBreakdown {
            tokens: sizes.kiro,
            percentage: kiro_final,
            items: vec![],
        },
        your_prompts: CategoryBreakdown {
            tokens: sizes.user,
            percentage: user_final,
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

/// Adjust component percentages to match backend total.
///
/// Strategy: Always keep context_files + tools stable (they tokenize predictably)
/// and fill the remaining percentage to kiro_responses + your_prompts (where char-based
/// estimates are unreliable, especially for repetitive text).
///
/// This works whether estimates are too high or too low compared to backend.
///
/// # Arguments
/// * `context_files_pct` - Estimated context file percentage
/// * `tools_pct` - Estimated tool percentage
/// * `kiro_pct` - Estimated kiro response percentage
/// * `user_pct` - Estimated user prompt percentage
/// * `backend_total_pct` - Accurate total percentage from backend
///
/// # Returns
/// Adjusted component percentages that sum to `backend_total_pct`
fn adjust_component_percentages(
    context_files_pct: f32,
    tools_pct: f32,
    kiro_pct: f32,
    user_pct: f32,
    backend_total_pct: f32,
) -> (f32, f32, f32, f32) {
    // Strategy: Always keep context_files + tools stable (they tokenize predictably)
    // Fill remaining to kiro + user (where char-based estimates are unreliable)
    let stable_total = context_files_pct + tools_pct;

    if stable_total == 0.0 && backend_total_pct == 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let remaining = backend_total_pct - stable_total;

    // Edge case: stable components exceed backend (shouldn't happen in practice)
    if remaining < 0.0 {
        let scale_factor = if stable_total > 0.0 {
            backend_total_pct / stable_total
        } else {
            0.0
        };
        return (context_files_pct * scale_factor, tools_pct * scale_factor, 0.0, 0.0);
    }

    // Distribute remaining to kiro + user, preserving their ratio
    let variable_estimate = kiro_pct + user_pct;
    let (kiro_final, user_final) = if variable_estimate > 0.0 {
        let kiro_ratio = kiro_pct / variable_estimate;
        (remaining * kiro_ratio, remaining * (1.0 - kiro_ratio))
    } else {
        (0.0, remaining)
    };

    (context_files_pct, tools_pct, kiro_final, user_final)
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

    use super::*;
    #[test]
    fn test_context_args_default() {
        let args = ContextArgs::default();
        assert!(!args.verbose);
    }

    #[test]
    fn test_adjust_component_gap_filling() {
        // Backend says 80%, estimates say 50% (underestimate)
        // Context: 5%, Tools: 15%, Kiro: 20%, User: 10% = 50%
        // Stable: 20%, Remaining: 60% distributed to kiro+user (2:1 ratio)
        let (context, tools, kiro, user) = adjust_component_percentages(5.0, 15.0, 20.0, 10.0, 80.0);

        // Context and tools should stay the same
        assert!((context - 5.0).abs() < 0.01, "Context should be ~5%, got {}", context);
        assert!((tools - 15.0).abs() < 0.01, "Tools should be ~15%, got {}", tools);

        // Remaining 60% distributed 2:1 (kiro:user) = 40% + 20%
        assert!((kiro - 40.0).abs() < 0.1, "Expected ~40%, got {}", kiro);
        assert!((user - 20.0).abs() < 0.1, "Expected ~20%, got {}", user);

        // Sum should equal backend total
        assert!((context + tools + kiro + user - 80.0).abs() < 0.001);
    }

    #[test]
    fn test_adjust_component_overestimate() {
        // Backend says 40%, estimates say 80% (overestimate from repetitive text)
        // Context: 10%, Tools: 30%, Kiro: 20%, User: 20% = 80%
        // Stable: 40%, Remaining: 0% means context+tools fill it all
        let (context, tools, kiro, user) = adjust_component_percentages(10.0, 30.0, 20.0, 20.0, 40.0);

        assert!((context - 10.0).abs() < 0.01, "Context should be ~10%, got {}", context);
        assert!((tools - 30.0).abs() < 0.01, "Tools should be ~30%, got {}", tools);

        // Remaining is 0%, so kiro+user get nothing
        assert!((kiro - 0.0).abs() < 0.1);
        assert!((user - 0.0).abs() < 0.1);

        assert!((context + tools + kiro + user - 40.0).abs() < 0.001);
    }

    #[test]
    fn test_adjust_component_zero_variable() {
        // Edge case: no kiro or user messages, remaining goes to user
        let (context, tools, kiro, user) = adjust_component_percentages(5.0, 10.0, 0.0, 0.0, 60.0);

        assert!((context - 5.0).abs() < 0.01, "Context should be ~5%, got {}", context);
        assert!((tools - 10.0).abs() < 0.01, "Tools should be ~10%, got {}", tools);
        // Remaining (45%) goes to user
        assert!((kiro - 0.0).abs() < 0.01, "Kiro should be ~0%, got {}", kiro);
        assert!((user - 45.0).abs() < 0.1, "User should get the remaining, got {}", user);

        assert!((context + tools + kiro + user - 60.0).abs() < 0.001);
    }

    #[test]
    fn test_adjust_component_zero_all() {
        // Edge case: all zeros
        let (context, tools, kiro, user) = adjust_component_percentages(0.0, 0.0, 0.0, 0.0, 0.0);

        assert_eq!(context, 0.0);
        assert_eq!(tools, 0.0);
        assert_eq!(kiro, 0.0);
        assert_eq!(user, 0.0);
    }

    #[test]
    fn test_adjust_component_realistic_underestimate() {
        // Real scenario: estimates are low, backend is high
        // Estimates: context 0.1%, tools 18.9%, kiro 0%, user 0% = 19%
        // Backend: 74.5%
        // Remaining 55.5% should go to user (kiro is 0)
        let (context, tools, kiro, user) = adjust_component_percentages(0.1, 18.9, 0.0, 0.0, 74.5);

        assert!(
            (context - 0.1).abs() < 0.1,
            "Context should stay ~0.1%, got {}",
            context
        );
        assert!((tools - 18.9).abs() < 0.5, "Tools should stay ~18.9%, got {}", tools);

        // Remaining should go to user (since kiro is 0)
        assert!(user > 50.0, "User should get most of the remaining, got {}%", user);

        assert!((context + tools + kiro + user - 74.5).abs() < 0.1);
    }

    #[test]
    fn test_adjust_component_realistic_overestimate() {
        // Real scenario from user report: repetitive text causes overestimate
        // Estimates: context 0.1%, tools 18.9%, kiro 0.1%, user 100% = 119.1%
        // Backend: 74.5%
        // Stable: 19%, Remaining: 55.5% distributed to kiro+user
        let (context, tools, kiro, user) = adjust_component_percentages(0.1, 18.9, 0.1, 100.0, 74.5);

        // Context and tools MUST remain stable (the key fix!)
        assert!(
            (context - 0.1).abs() < 0.1,
            "Context should stay ~0.1%, got {}",
            context
        );
        assert!((tools - 18.9).abs() < 0.5, "Tools should stay ~18.9%, got {}", tools);

        // Remaining 55.5% distributed to kiro+user based on their ratio
        // Original ratio: 0.1:100 ≈ 0:100, so almost all goes to user
        assert!(user > 50.0, "User should get most of remaining, got {}%", user);
        assert!(kiro < 1.0, "Kiro should get minimal amount, got {}%", kiro);

        assert!((context + tools + kiro + user - 74.5).abs() < 0.1);
    }

    #[test]
    fn test_adjust_component_tools_stay_stable() {
        // Verify tools percentage doesn't drop when backend < estimate
        let (_, tools, _, _) = adjust_component_percentages(1.0, 20.0, 5.0, 4.0, 75.0);

        // Tools should NOT decrease (was the original problem)
        assert!(
            tools >= 20.0 - 0.01,
            "Tools should stay stable at 20%, got {:.1}%",
            tools
        );
    }

    #[test]
    fn test_adjust_component_preserves_kiro_user_ratio() {
        // When filling remaining, maintain ratio between kiro and user
        // Context: 1%, Tools: 10%, Kiro: 20%, User: 10%
        // Stable: 11%, Backend: 81%, Remaining: 70%
        let (_, _, kiro, user) = adjust_component_percentages(1.0, 10.0, 20.0, 10.0, 81.0);

        // Original kiro:user ratio is 2:1 (20%:10%)
        // After distributing remaining, ratio should be preserved
        let original_ratio = 20.0 / 10.0;
        let adjusted_ratio = kiro / user;

        assert!(
            (original_ratio - adjusted_ratio).abs() < 0.01,
            "Ratio should be preserved: expected {}, got {}",
            original_ratio,
            adjusted_ratio
        );
    }
}
