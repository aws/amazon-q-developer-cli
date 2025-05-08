use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use crossterm::queue;
use crossterm::style::{
    self,
    Attribute,
    Color,
};

use crate::cli::chat::command::{
    Command,
    ToolsSubcommand,
};
use crate::cli::chat::commands::context_adapter::CommandContextAdapter;
use crate::cli::chat::commands::handler::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    FigTool,
    QueuedTool,
};

/// Static instance of the tools trustall command handler
pub static TRUSTALL_TOOLS_HANDLER: TrustAllToolsCommand = TrustAllToolsCommand;

/// Handler for the tools trustall command
pub struct TrustAllToolsCommand;

impl CommandHandler for TrustAllToolsCommand {
    fn name(&self) -> &'static str {
        "trustall"
    }

    fn description(&self) -> &'static str {
        "Trust all tools for the session"
    }

    fn usage(&self) -> &'static str {
        "/tools trustall"
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Tools {
            subcommand: Some(ToolsSubcommand::TrustAll { from_deprecated: false }),
        })
    }

    fn help(&self) -> String {
        "Trust all tools for the session. This will allow all tools to run without confirmation.".to_string()
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            if let Command::Tools {
                subcommand: Some(ToolsSubcommand::TrustAll { from_deprecated }),
            } = command
            {
                // Show deprecation message if needed
                if *from_deprecated {
                    queue!(
                        ctx.output,
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("\n/acceptall is deprecated. Use /tools instead.\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                    ctx.output.flush()?;
                }

                // Trust all tools
                ctx.conversation_state
                    .tools
                    .values()
                    .flatten()
                    .for_each(|FigTool::ToolSpecification(spec)| {
                        ctx.tool_permissions.trust_tool(spec.name.as_str());
                    });

                queue!(
                    ctx.output,
                    style::SetForegroundColor(Color::Green),
                    style::Print("\nAll tools are now trusted ("),
                    style::SetForegroundColor(Color::Red),
                    style::Print("!"),
                    style::SetForegroundColor(Color::Green),
                    style::Print("). Amazon Q will execute tools "),
                    style::SetAttribute(Attribute::Bold),
                    style::Print("without"),
                    style::SetAttribute(Attribute::NoBold),
                    style::Print(" asking for confirmation.\n"),
                    style::Print("Agents can sometimes do unexpected things so understand the risks.\n"),
                    style::ResetColor,
                    style::Print("\n")
                )?;
                ctx.output.flush()?;

                Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: false,
                })
            } else {
                Err(ChatError::Custom(
                    "TrustAllToolsCommand can only execute TrustAll commands".into(),
                ))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Trustall command requires confirmation
    }
}
