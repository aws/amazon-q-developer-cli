use crate::cli::agent::{PermissionEvalResult, Agents};
use crate::cli::chat::tools::QueuedTool;
use crate::os::Os;

/// Result of evaluating permissions for a single tool
#[derive(Debug, Clone)]
pub enum ToolPermissionResult {
    /// Tool is allowed to execute without confirmation
    Allowed { tool_index: usize },
    /// Tool requires user confirmation before execution
    RequiresConfirmation { 
        tool_index: usize, 
        tool_name: String 
    },
    /// Tool is denied and should not be executed
    Denied { 
        tool_index: usize, 
        tool_name: String, 
        rules: Vec<String> 
    },
}

/// Context information for permission evaluation
#[derive(Debug)]
pub struct PermissionContext {
    pub trust_all_tools: bool,
}

/// Evaluates permissions for all tools in the queue
/// 
/// This is pure logic that determines what should happen with each tool
/// based on agent configuration, trust settings, and tool requirements.
/// It contains no UI logic or side effects.
pub fn evaluate_tool_permissions(
    tools: &[QueuedTool],
    agents: &Agents,
    os: &Os,
) -> Vec<ToolPermissionResult> {
    let context = PermissionContext {
        trust_all_tools: agents.trust_all_tools,
    };
    
    let result = tools
        .iter()
        .enumerate()
        .map(|(i, tool)| evaluate_single_tool_permission(i, tool, agents, os, &context))
        .collect();

    tracing::debug!(func="evaluate_tool_permission", context = ?context, result = ?result);

    result
}

/// Evaluates permission for a single tool
fn evaluate_single_tool_permission(
    tool_index: usize,
    tool: &QueuedTool,
    agents: &Agents,
    os: &Os,
    context: &PermissionContext,
) -> ToolPermissionResult {
    tracing::debug!(func="evaluate_single_tool_permission", tool = ?tool);

    // If tool is already accepted, it's allowed
    if tool.accepted {
        return ToolPermissionResult::Allowed { tool_index };
    }

    // Check agent-based permissions
    let permission_result = agents
        .get_active()
        .map(|agent| tool.tool.requires_acceptance(os, agent))
        .unwrap_or(PermissionEvalResult::Ask);
    
    match permission_result {
        PermissionEvalResult::Allow => {
            ToolPermissionResult::Allowed { tool_index }
        }
        PermissionEvalResult::Ask => {
            // Check if trust_all_tools overrides the ask
            if context.trust_all_tools {
                ToolPermissionResult::Allowed { tool_index }
            } else {
                ToolPermissionResult::RequiresConfirmation {
                    tool_index,
                    tool_name: tool.name.clone(),
                }
            }
        }
        PermissionEvalResult::Deny(rules) => {
            ToolPermissionResult::Denied {
                tool_index,
                tool_name: tool.name.clone(),
                rules,
            }
        }
    }
}