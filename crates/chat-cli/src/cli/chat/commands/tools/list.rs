use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use crossterm::queue;
use crossterm::style::{
    self,
    Attribute,
    Color,
};

use crate::cli::chat::command::Command;
use crate::cli::chat::commands::context_adapter::CommandContextAdapter;
use crate::cli::chat::commands::handler::CommandHandler;
use crate::cli::chat::consts::DUMMY_TOOL_NAME;
use crate::cli::chat::{
    ChatError,
    ChatState,
    FigTool,
    QueuedTool,
};

/// Static instance of the tools list command handler
pub static LIST_TOOLS_HANDLER: ListToolsCommand = ListToolsCommand;

/// Handler for the tools list command
pub struct ListToolsCommand;

impl CommandHandler for ListToolsCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    fn description(&self) -> &'static str {
        "List all available tools and their status"
    }

    fn usage(&self) -> &'static str {
        "/tools list"
    }

    fn help(&self) -> String {
        "List all available tools and their current permission status.".to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Tools { subcommand: None })
    }

    fn execute_command<'a>(
        &'a self,
        _command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            // Determine how to format the output nicely.
            let terminal_width = ctx.terminal_width();
            let longest = ctx
                .conversation_state
                .tools
                .values()
                .flatten()
                .map(|FigTool::ToolSpecification(spec)| spec.name.len())
                .max()
                .unwrap_or(0);

            queue!(
                ctx.output,
                style::Print("\n"),
                style::SetAttribute(Attribute::Bold),
                style::Print({
                    // Adding 2 because of "- " preceding every tool name
                    let width = longest + 2 - "Tool".len() + 4;
                    format!("Tool{:>width$}Permission", "", width = width)
                }),
                style::SetAttribute(Attribute::Reset),
                style::Print("\n"),
                style::Print("▔".repeat(terminal_width)),
            )?;

            ctx.conversation_state.tools.iter().for_each(|(origin, tools)| {
                let to_display = tools
                    .iter()
                    .filter(|FigTool::ToolSpecification(spec)| spec.name != DUMMY_TOOL_NAME)
                    .fold(String::new(), |mut acc, FigTool::ToolSpecification(spec)| {
                        let width = longest - spec.name.len() + 4;
                        acc.push_str(
                            format!(
                                "- {}{:>width$}{}\n",
                                spec.name,
                                "",
                                ctx.tool_permissions.display_label(&spec.name),
                                width = width
                            )
                            .as_str(),
                        );
                        acc
                    });
                let _ = queue!(
                    ctx.output,
                    style::SetAttribute(Attribute::Bold),
                    style::Print(format!("{}:\n", origin)),
                    style::SetAttribute(Attribute::Reset),
                    style::Print(to_display),
                    style::Print("\n")
                );
            });

            queue!(
                ctx.output,
                style::Print("\nTrusted tools can be run without confirmation\n"),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(format!("\n{}\n", "* Default settings")),
                style::Print("\n💡 Use "),
                style::SetForegroundColor(Color::Green),
                style::Print("/tools help"),
                style::SetForegroundColor(Color::Reset),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(" to edit permissions."),
                style::SetForegroundColor(Color::Reset),
            )?;
            ctx.output.flush()?;

            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // List command doesn't require confirmation
    }
}
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::*;
    use crate::cli::chat::conversation_state::ConversationState;
    use crate::cli::chat::input_source::InputSource;
    use crate::cli::chat::tools::ToolPermissions;
    use crate::cli::chat::util::shared_writer::SharedWriter;
    use crate::platform::Context;
    use crate::settings::Settings;

    #[tokio::test]
    async fn test_tools_list_command() {
        let handler = ListToolsCommand;

        // Create a minimal context
        let context = Arc::new(Context::new());
        let output = SharedWriter::null();
        let mut conversation_state = ConversationState::new(
            Arc::clone(&context),
            "test-conversation",
            HashMap::new(),
            None,
            Some(SharedWriter::null()),
        )
        .await;
        let mut tool_permissions = ToolPermissions::new(0);
        let mut input_source = InputSource::new_mock(vec![]);
        let settings = Settings::new();

        let mut ctx = CommandContextAdapter {
            context: &context,
            output: &mut output.clone(),
            conversation_state: &mut conversation_state,
            tool_permissions: &mut tool_permissions,
            interactive: true,
            input_source: &mut input_source,
            settings: &settings,
        };

        // Execute the list subcommand
        let args = vec![];
        let result = handler.execute(args, &mut ctx, None, None).await;

        assert!(result.is_ok());
    }
}
