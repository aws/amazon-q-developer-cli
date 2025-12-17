use std::collections::HashMap;
use std::io::Write;

use chat_cli_ui::conduit::{
    ControlEnd,
    DestinationStdout,
};
use eyre::Result;

use super::ToolInfo;
use super::code::Code;
use super::custom_tool::CustomTool;
use super::delegate::Delegate;
use super::execute::ExecuteCommand;
use super::fs_read::FsRead;
use super::fs_write::FsWrite;
use super::gh_issue::GhIssue;
use super::grep::Grep;
use super::introspect::Introspect;
use super::knowledge::Knowledge;
use super::thinking::Thinking;
use super::todo::TodoList;
use super::use_aws::UseAws;
use super::use_subagent::UseSubagent;
use super::web_fetch::WebFetch;
use super::web_search::WebSearch;
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::line_tracker::FileLineTracker;
use crate::os::Os;

/// Proxy for accessing tool metadata without importing all tool structs
pub struct ToolMetadata;

impl ToolMetadata {
    /// All native tool infos for iteration
    const ALL: &[&ToolInfo] = &[
        Self::FS_READ,
        Self::FS_WRITE,
        Self::EXECUTE_COMMAND,
        Self::USE_AWS,
        Self::GH_ISSUE,
        Self::INTROSPECT,
        Self::KNOWLEDGE,
        Self::CODE,
        Self::THINKING,
        Self::TODO,
        Self::DELEGATE,
        Self::WEB_SEARCH,
        Self::WEB_FETCH,
        Self::USE_SUBAGENT,
    ];
    pub const CODE: &ToolInfo = &Code::INFO;
    pub const DELEGATE: &ToolInfo = &Delegate::INFO;
    pub const EXECUTE_COMMAND: &ToolInfo = &ExecuteCommand::INFO;
    pub const FS_READ: &ToolInfo = &FsRead::INFO;
    pub const FS_WRITE: &ToolInfo = &FsWrite::INFO;
    pub const GH_ISSUE: &ToolInfo = &GhIssue::INFO;
    pub const GREP: &ToolInfo = &Grep::INFO;
    pub const INTROSPECT: &ToolInfo = &Introspect::INFO;
    pub const KNOWLEDGE: &ToolInfo = &Knowledge::INFO;
    pub const THINKING: &ToolInfo = &Thinking::INFO;
    pub const TODO: &ToolInfo = &TodoList::INFO;
    pub const USE_AWS: &ToolInfo = &UseAws::INFO;
    pub const USE_SUBAGENT: &ToolInfo = &UseSubagent::INFO;
    pub const WEB_FETCH: &ToolInfo = &WebFetch::INFO;
    pub const WEB_SEARCH: &ToolInfo = &WebSearch::INFO;

    /// Get ToolInfo by tool specification name
    pub fn get_by_spec_name(spec_name: &str) -> Option<&'static ToolInfo> {
        Self::ALL.iter().copied().find(|info| info.spec_name == spec_name)
    }
}

/// Check if a tool name matches any native tool (by any alias)
pub fn is_native_tool(name: &str) -> bool {
    ToolMetadata::ALL.iter().any(|info| info.aliases.contains(&name))
}

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
    Code(Code),
    Thinking(Thinking),
    Todo(TodoList),
    Delegate(Delegate),
    WebSearch(WebSearch),
    WebFetch(WebFetch),
    UseSubagent(UseSubagent),
    Grep(Grep),
}

impl Tool {
    /// The display name of a tool
    pub fn display_name(&self) -> &str {
        match self {
            Tool::FsRead(_) => FsRead::INFO.preferred_alias,
            Tool::FsWrite(_) => FsWrite::INFO.preferred_alias,
            Tool::ExecuteCommand(_) => ExecuteCommand::INFO.preferred_alias,
            Tool::UseAws(_) => UseAws::INFO.preferred_alias,
            Tool::Custom(custom_tool) => &custom_tool.name,
            Tool::GhIssue(_) => GhIssue::INFO.preferred_alias,
            Tool::Introspect(_) => Introspect::INFO.preferred_alias,
            Tool::Knowledge(_) => Knowledge::INFO.preferred_alias,
            Tool::Code(_) => Code::INFO.preferred_alias,
            Tool::Thinking(_) => Thinking::INFO.preferred_alias,
            Tool::Todo(_) => TodoList::INFO.preferred_alias,
            Tool::Delegate(_) => Delegate::INFO.preferred_alias,
            Tool::WebSearch(_) => WebSearch::INFO.preferred_alias,
            Tool::WebFetch(_) => WebFetch::INFO.preferred_alias,
            Tool::UseSubagent(_) => UseSubagent::INFO.preferred_alias,
            Tool::Grep(_) => Grep::INFO.preferred_alias,
        }
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
            Tool::Code(_) => Code::eval_perm(os, agent),
            Tool::Delegate(_) => PermissionEvalResult::Allow,
            Tool::WebSearch(web_search) => web_search.eval_perm(os, agent),
            Tool::WebFetch(web_fetch) => web_fetch.eval_perm(os, agent),
            Tool::UseSubagent(_use_subagent) => PermissionEvalResult::Allow,
            Tool::Grep(grep) => grep.eval_perm(os, agent),
        }
    }

    /// Invokes the tool asynchronously
    pub async fn invoke(
        &self,
        os: &Os,
        stdout: &mut impl Write,
        line_tracker: &mut HashMap<String, FileLineTracker>,
        agents: &crate::cli::agent::Agents,
        code_intelligence_client: &Option<std::sync::Arc<tokio::sync::RwLock<code_agent_sdk::CodeIntelligence>>>,
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
            Tool::Code(code) => code.invoke(os, stdout, code_intelligence_client).await,
            Tool::Thinking(think) => think.invoke(stdout).await,
            Tool::Todo(todo) => todo.invoke(os, stdout).await,
            Tool::Delegate(delegate) => delegate.invoke(os, stdout, agents).await,
            Tool::WebSearch(web_search) => web_search.invoke(os, stdout).await,
            Tool::WebFetch(web_fetch) => web_fetch.invoke(os, stdout).await,
            Tool::UseSubagent(use_subagent) => use_subagent.invoke(os, agents).await,
            Tool::Grep(grep) => grep.invoke(os, stdout).await,
        }
    }

    /// Queues up a tool's intention in a human readable format
    pub async fn queue_description(&self, os: &Os, output: &mut ControlEnd<DestinationStdout>) -> Result<()> {
        use chat_cli_ui::protocol::{
            SessionEvent,
            ToolCallArgs,
        };

        if output.should_send_structured_event {
            let mut buf = Vec::<u8>::new();

            match self {
                Tool::FsRead(fs_read) => fs_read.queue_description(self, os, &mut buf).await,
                Tool::FsWrite(fs_write) => fs_write.queue_description(self, os, &mut buf),
                Tool::ExecuteCommand(execute_command) => execute_command.queue_description(self, &mut buf),
                Tool::UseAws(use_aws) => use_aws.queue_description(self, &mut buf),
                Tool::Custom(custom_tool) => custom_tool.queue_description(self, &mut buf),
                Tool::GhIssue(gh_issue) => gh_issue.queue_description(self, &mut buf),
                Tool::Introspect(_) => Introspect::queue_description(self, &mut buf),
                Tool::Knowledge(knowledge) => knowledge.queue_description(self, os, &mut buf).await,
                Tool::Code(code) => code.queue_description(self, &mut buf),
                Tool::Thinking(thinking) => thinking.queue_description(self, &mut buf),
                Tool::Todo(_) => Ok(()),
                Tool::Delegate(delegate) => delegate.queue_description(self, &mut buf),
                Tool::WebSearch(web_search) => web_search.queue_description(self, &mut buf),
                Tool::WebFetch(web_fetch) => web_fetch.queue_description(self, &mut buf),
                Tool::UseSubagent(use_subagent) => use_subagent.queue_description(self, &mut buf),
                Tool::Grep(grep) => grep.queue_description(self, &mut buf),
            }?;

            let tool_call_args = ToolCallArgs {
                tool_call_id: Default::default(),
                delta: {
                    let sanitized = strip_ansi_escapes::strip_str(String::from_utf8_lossy(&buf));
                    serde_json::Value::String(sanitized)
                },
            };

            output.send(SessionEvent::AgentEvent(chat_cli_ui::protocol::AgentEvent {
                agent_id: Default::default(),
                kind: chat_cli_ui::protocol::AgentEventKind::ToolCallArgs(tool_call_args),
            }))?;
        } else {
            match self {
                Tool::FsRead(fs_read) => fs_read.queue_description(self, os, output).await,
                Tool::FsWrite(fs_write) => fs_write.queue_description(self, os, output),
                Tool::ExecuteCommand(execute_command) => execute_command.queue_description(self, output),
                Tool::UseAws(use_aws) => use_aws.queue_description(self, output),
                Tool::Custom(custom_tool) => custom_tool.queue_description(self, output),
                Tool::GhIssue(gh_issue) => gh_issue.queue_description(self, output),
                Tool::Introspect(_) => Introspect::queue_description(self, output),
                Tool::Knowledge(knowledge) => knowledge.queue_description(self, os, output).await,
                Tool::Code(code) => code.queue_description(self, output),
                Tool::Thinking(thinking) => thinking.queue_description(self, output),
                Tool::Todo(_) => Ok(()),
                Tool::Delegate(delegate) => delegate.queue_description(self, output),
                Tool::WebSearch(web_search) => web_search.queue_description(self, output),
                Tool::WebFetch(web_fetch) => web_fetch.queue_description(self, output),
                Tool::UseSubagent(use_subagent) => use_subagent.queue_description(self, output),
                Tool::Grep(grep) => grep.queue_description(self, output),
            }?;
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
            Tool::Code(code) => code.validate(os).await,
            Tool::Thinking(think) => think.validate(os).await,
            Tool::Todo(todo) => todo.validate(os).await,
            Tool::Delegate(_) => Ok(()),
            Tool::WebSearch(web_search) => web_search.validate(os).await,
            Tool::WebFetch(web_fetch) => web_fetch.validate(os).await,
            Tool::UseSubagent(use_subagent) => use_subagent.validate(),
            Tool::Grep(grep) => grep.validate(os).await,
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
