//! /prompts command execution

use agent::AgentHandle;
use agent::tui_commands::{
    CommandOption,
    CommandOptionsResponse,
    CommandResult,
    PromptsArgs,
};

pub async fn execute(args: &PromptsArgs) -> CommandResult {
    let Some(name) = &args.prompt_name else {
        return CommandResult::success("Use the selection menu to pick a prompt.");
    };
    // Prompt execution requires streaming via session/prompt, which the command
    // execute channel doesn't support. Return the prompt name so the TUI can
    // send it as a user message through the normal session/prompt flow.
    CommandResult::success_with_data("", serde_json::json!({ "executePrompt": format!("/{name}") }))
}

pub async fn get_options(agent: &AgentHandle) -> CommandOptionsResponse {
    let mut options = Vec::new();

    if let Ok(mcp_prompts) = agent.get_mcp_prompts().await {
        for (server_name, server_prompts) in mcp_prompts {
            for prompt in server_prompts {
                let arg_hint = prompt.arguments.as_ref().and_then(|args| {
                    let s: String = args
                        .iter()
                        .map(|a| {
                            if a.required.unwrap_or(false) {
                                format!("<{}>", a.name)
                            } else {
                                format!("[{}]", a.name)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    if s.is_empty() { None } else { Some(s) }
                });
                options.push(CommandOption {
                    value: prompt.name.clone(),
                    label: format!("/{}", prompt.name),
                    description: Some(prompt.description.unwrap_or_default()),
                    group: Some(server_name.clone()),
                    hint: arg_hint,
                });
            }
        }
    }

    if let Ok(file_prompts) = agent.get_file_prompts().await {
        for (source, source_prompts) in file_prompts {
            for prompt in source_prompts {
                options.push(CommandOption {
                    value: prompt.name.clone(),
                    label: format!("/{}", prompt.name),
                    description: prompt.description.or(Some("(file prompt)".to_string())),
                    group: Some(source.clone()),
                    hint: None,
                });
            }
        }
    }

    options.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
    CommandOptionsResponse {
        options,
        has_more: false,
    }
}
