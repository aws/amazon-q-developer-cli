use std::collections::HashMap;
use std::io::Write;

use chat_cli_ui::conduit::{
    ControlEnd,
    DestinationStdout,
};
use eyre::Result;

use super::custom_tool::CustomTool;
use super::delegate::Delegate;
use super::execute::ExecuteCommand;
use super::fs_read::FsRead;
use super::fs_write::FsWrite;
use super::gh_issue::GhIssue;
use super::introspect::Introspect;
use super::knowledge::Knowledge;
use super::thinking::Thinking;
use super::todo::TodoList;
use super::use_aws::UseAws;
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::line_tracker::FileLineTracker;
use crate::os::Os;
use crate::theme::StyledText;

/// Enum representing tool types without data, used for consistent naming
#[derive(Debug, Clone, Copy)]
enum ToolMetadata {
    FsRead,
    FsWrite,
    ExecuteCommand,
    UseAws,
    GhIssue,
    Introspect,
    Knowledge,
    Thinking,
    Todo,
    Delegate,
}

impl ToolMetadata {
    const fn name(self) -> &'static str {
        match self {
            ToolMetadata::FsRead => "fs_read",
            ToolMetadata::FsWrite => "fs_write",
            ToolMetadata::ExecuteCommand => "execute_bash",
            ToolMetadata::UseAws => "use_aws",
            ToolMetadata::GhIssue => "gh_issue",
            ToolMetadata::Introspect => "introspect",
            ToolMetadata::Knowledge => "knowledge",
            ToolMetadata::Thinking => "thinking",
            ToolMetadata::Todo => "todo_list",
            ToolMetadata::Delegate => "delegate",
        }
    }
}

pub const NATIVE_TOOL_NAMES: &[&str] = &[
    ToolMetadata::FsRead.name(),
    ToolMetadata::FsWrite.name(),
    ToolMetadata::ExecuteCommand.name(),
    ToolMetadata::UseAws.name(),
    ToolMetadata::GhIssue.name(),
    ToolMetadata::Knowledge.name(),
    ToolMetadata::Thinking.name(),
    ToolMetadata::Todo.name(),
    ToolMetadata::Delegate.name(),
];

/// Represents an executable tool use.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum Tool {
    FsRead(FsRead),
    FsWrite(FsWrite),
    ExecuteCommand(ExecuteCommand),
    UseAws(UseAws),
    Custom(CustomTool),
    GhIssue(GhIssue),
    Introspect(Introspect),
    Knowledge(Knowledge),
    Thinking(Thinking),
    Todo(TodoList),
    Delegate(Delegate),
}

impl Tool {
    /// The display name of a tool
    pub fn display_name(&self) -> String {
        match self {
            Tool::FsRead(_) => ToolMetadata::FsRead.name(),
            Tool::FsWrite(_) => ToolMetadata::FsWrite.name(),
            Tool::ExecuteCommand(_) => ToolMetadata::ExecuteCommand.name(),
            Tool::UseAws(_) => ToolMetadata::UseAws.name(),
            Tool::Custom(custom_tool) => &custom_tool.name,
            Tool::GhIssue(_) => ToolMetadata::GhIssue.name(),
            Tool::Introspect(_) => ToolMetadata::Introspect.name(),
            Tool::Knowledge(_) => ToolMetadata::Knowledge.name(),
            Tool::Thinking(_) => "thinking (prerelease)",
            Tool::Todo(_) => ToolMetadata::Todo.name(),
            Tool::Delegate(_) => ToolMetadata::Delegate.name(),
        }
        .to_owned()
    }

    /// Whether or not the tool should prompt the user to accept before [Self::invoke] is called.
    pub fn requires_acceptance(&self, os: &Os, agent: &Agent) -> PermissionEvalResult {
        match self {
            Tool::FsRead(fs_read) => fs_read.eval_perm(os, agent),
            Tool::FsWrite(fs_write) => fs_write.eval_perm(os, agent),
            Tool::ExecuteCommand(execute_command) => execute_command.eval_perm(os, agent),
            Tool::UseAws(use_aws) => use_aws.eval_perm(os, agent),
            Tool::Custom(custom_tool) => custom_tool.eval_perm(os, agent),
            Tool::GhIssue(_) => PermissionEvalResult::Allow,
            Tool::Introspect(_) => PermissionEvalResult::Allow,
            Tool::Thinking(_) => PermissionEvalResult::Allow,
            Tool::Todo(_) => PermissionEvalResult::Allow,
            Tool::Knowledge(knowledge) => knowledge.eval_perm(os, agent),
            Tool::Delegate(_) => PermissionEvalResult::Allow,
        }
    }

    /// Invokes the tool asynchronously
    pub async fn invoke(
        &self,
        os: &Os,
        stdout: &mut impl Write,
        line_tracker: &mut HashMap<String, FileLineTracker>,
        agents: &crate::cli::agent::Agents,
    ) -> Result<super::InvokeOutput> {
        let active_agent = agents.get_active();
        match self {
            Tool::FsRead(fs_read) => fs_read.invoke(os, stdout).await,
            Tool::FsWrite(fs_write) => fs_write.invoke(os, stdout, line_tracker).await,
            Tool::ExecuteCommand(execute_command) => execute_command.invoke(os, stdout).await,
            Tool::UseAws(use_aws) => use_aws.invoke(os, stdout).await,
            Tool::Custom(custom_tool) => custom_tool.invoke(os, stdout).await,
            Tool::GhIssue(gh_issue) => gh_issue.invoke(os, stdout).await,
            Tool::Introspect(introspect) => introspect.invoke(os, stdout).await,
            Tool::Knowledge(knowledge) => knowledge.invoke(os, stdout, active_agent).await,
            Tool::Thinking(think) => think.invoke(stdout).await,
            Tool::Todo(todo) => todo.invoke(os, stdout).await,
            Tool::Delegate(delegate) => delegate.invoke(os, stdout, agents).await,
        }
    }

    /// Queues up a tool's intention in a human readable format
    pub async fn queue_description(&self, os: &Os, output: &mut ControlEnd<DestinationStdout>) -> Result<()> {
        use chat_cli_ui::protocol::{
            Event,
            ToolCallArgs,
        };
        use crossterm::{
            queue,
            style,
        };

        if output.should_send_structured_event {
            let mut buf = Vec::<u8>::new();

            match self {
                Tool::FsRead(fs_read) => fs_read.queue_description(os, &mut buf).await,
                Tool::FsWrite(fs_write) => fs_write.queue_description(os, &mut buf),
                Tool::ExecuteCommand(execute_command) => execute_command.queue_description(&mut buf),
                Tool::UseAws(use_aws) => use_aws.queue_description(&mut buf),
                Tool::Custom(custom_tool) => custom_tool.queue_description(&mut buf),
                Tool::GhIssue(gh_issue) => gh_issue.queue_description(&mut buf),
                Tool::Introspect(_) => Introspect::queue_description(&mut buf),
                Tool::Knowledge(knowledge) => knowledge.queue_description(os, &mut buf).await,
                Tool::Thinking(thinking) => thinking.queue_description(&mut buf),
                Tool::Todo(_) => Ok(()),
                Tool::Delegate(delegate) => delegate.queue_description(&mut buf),
            }?;

            let tool_call_args = ToolCallArgs {
                tool_call_id: Default::default(),
                delta: {
                    let sanitized = strip_ansi_escapes::strip_str(String::from_utf8_lossy(&buf));
                    serde_json::Value::String(sanitized)
                },
            };

            output.send(Event::ToolCallArgs(tool_call_args))?;
        } else {
            match self {
                Tool::FsRead(fs_read) => fs_read.queue_description(os, output).await,
                Tool::FsWrite(fs_write) => fs_write.queue_description(os, output),
                Tool::ExecuteCommand(execute_command) => execute_command.queue_description(output),
                Tool::UseAws(use_aws) => use_aws.queue_description(output),
                Tool::Custom(custom_tool) => custom_tool.queue_description(output),
                Tool::GhIssue(gh_issue) => gh_issue.queue_description(output),
                Tool::Introspect(_) => Introspect::queue_description(output),
                Tool::Knowledge(knowledge) => knowledge.queue_description(os, output).await,
                Tool::Thinking(thinking) => thinking.queue_description(output),
                Tool::Todo(_) => Ok(()),
                Tool::Delegate(delegate) => delegate.queue_description(output),
            }?;

            if let Tool::Custom(tool) = self {
                queue!(
                    output,
                    StyledText::secondary_fg(),
                    style::Print(" (from mcp server: "),
                    style::Print(&tool.server_name),
                    style::Print(")"),
                    StyledText::reset(),
                )?;
            } else {
                queue!(
                    output,
                    StyledText::secondary_fg(),
                    style::Print(" (using tool: "),
                    style::Print(self.display_name()),
                    style::Print(")"),
                    StyledText::reset(),
                )?;
            }
        };

        Ok(())
    }

    /// Validates the tool with the arguments supplied
    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        match self {
            Tool::FsRead(fs_read) => fs_read.validate(os).await,
            Tool::FsWrite(fs_write) => fs_write.validate(os).await,
            Tool::ExecuteCommand(execute_command) => execute_command.validate(os).await,
            Tool::UseAws(use_aws) => use_aws.validate(os).await,
            Tool::Custom(custom_tool) => custom_tool.validate(os).await,
            Tool::GhIssue(gh_issue) => gh_issue.validate(os).await,
            Tool::Introspect(introspect) => introspect.validate(os).await,
            Tool::Knowledge(knowledge) => knowledge.validate(os).await,
            Tool::Thinking(think) => think.validate(os).await,
            Tool::Todo(todo) => todo.validate(os).await,
            Tool::Delegate(_) => Ok(()),
        }
    }

    /// Returns additional information about the tool if available
    pub fn get_additional_info(&self) -> Option<serde_json::Value> {
        match self {
            Tool::UseAws(use_aws) => Some(use_aws.get_additional_info()),
            _ => None,
        }
    }

    /// Returns the tool's summary if available
    pub fn get_summary(&self) -> Option<String> {
        match self {
            Tool::FsWrite(fs_write) => fs_write.get_summary().cloned(),
            Tool::ExecuteCommand(execute_cmd) => execute_cmd.summary.clone(),
            Tool::FsRead(fs_read) => fs_read.summary.clone(),
            _ => None,
        }
    }
}
