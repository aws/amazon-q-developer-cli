use async_trait::async_trait;
use eyre::Result;

use crate::cli::chat::tools::QueuedTool;
use crate::os::Os;

use super::{PermissionContext, PermissionDecision, PermissionInterface};

/// Console-based permission interface that preserves existing terminal behavior
pub struct ConsolePermissionInterface<'a> {
    pub os: &'a Os,
}

#[async_trait]
impl<'a> PermissionInterface for ConsolePermissionInterface<'a> {
    async fn request_permission(
        &mut self,
        _tool: &QueuedTool,
        _context: &PermissionContext,
    ) -> Result<PermissionDecision> {
        // For now, return Approved to maintain existing flow
        // The actual user input handling happens in the existing state machine
        Ok(PermissionDecision::Approved)
    }

    async fn show_denied_tool(
        &mut self,
        tool_name: &str,
        rules: Vec<String>,
    ) -> Result<()> {
        let formatted_set = rules.into_iter().fold(String::new(), |mut acc, rule| {
            acc.push_str(&format!("\n  - {rule}"));
            acc
        });

        // We can't access stderr directly, so we'll use eprintln! for now
        // This will be improved when we have better access to the output streams
        eprintln!(
            "{}Command {}{}{} is rejected because it matches one or more rules on the denied list:{}{}",
            "\x1b[31m", // Red
            "\x1b[33m", // Yellow
            tool_name,
            "\x1b[31m", // Red
            formatted_set,
            "\x1b[0m"   // Reset
        );

        Ok(())
    }

    async fn show_tool_execution(
        &mut self,
        _tool: &QueuedTool,
        _allowed: bool,
    ) -> Result<()> {
        // For now, this is a no-op since we don't have direct access to the streams
        // This will be improved in the next iteration
        Ok(())
    }
}
