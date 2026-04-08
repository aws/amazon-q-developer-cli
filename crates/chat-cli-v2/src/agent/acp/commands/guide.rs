//! /guide command execution — swaps to the kiro_guide agent

use agent::protocol::SwapAgentArgs;
use agent::tui_commands::{
    CommandResult,
    GuideArgs,
};

use super::CommandContext;

const GUIDE_AGENT_NAME: &str = "kiro_guide";

pub async fn execute(args: &GuideArgs, ctx: &CommandContext<'_>) -> CommandResult {
    // If already on guide agent, toggle back or forward question
    if ctx.current_agent_name == GUIDE_AGENT_NAME {
        if let Some(question) = &args.question {
            return CommandResult::success_with_data(
                format!("Asking guide agent: {}", question),
                serde_json::json!({ "prompt": question }),
            );
        }
        // Toggle back to previous agent
        let target = ctx.previous_agent_name.unwrap_or("kiro_default");
        let agent_config = match ctx.agent_configs.iter().find(|c| c.name() == target) {
            Some(c) => c,
            None => return CommandResult::error(format!("Previous agent '{}' not found.", target)),
        };

        if let Err(e) = ctx
            .agent
            .swap_agent(SwapAgentArgs {
                agent_config: agent_config.clone(),
                local_mcp_path: ctx.local_mcp_path.cloned(),
                global_mcp_path: ctx.global_mcp_path.cloned(),
                force: false,
            })
            .await
        {
            return CommandResult::error(format!("Failed to switch back: {}", e));
        }

        return CommandResult::success_with_data(
            format!("Agent changed to {}", target),
            serde_json::json!({ "agent": { "name": target } }),
        );
    }

    let agent_config = match ctx.agent_configs.iter().find(|c| c.name() == GUIDE_AGENT_NAME) {
        Some(c) => c,
        None => return CommandResult::error("Guide agent not found.".to_string()),
    };

    if let Err(e) = ctx
        .agent
        .swap_agent(SwapAgentArgs {
            agent_config: agent_config.clone(),
            local_mcp_path: ctx.local_mcp_path.cloned(),
            global_mcp_path: ctx.global_mcp_path.cloned(),
            force: false,
        })
        .await
    {
        return CommandResult::error(format!("Failed to switch to guide agent: {}", e));
    }

    let data = if let Some(question) = &args.question {
        serde_json::json!({ "agent": { "name": GUIDE_AGENT_NAME }, "prompt": question })
    } else {
        serde_json::json!({ "agent": { "name": GUIDE_AGENT_NAME } })
    };

    CommandResult::success_with_data(format!("Agent changed to {}", GUIDE_AGENT_NAME), data)
}
