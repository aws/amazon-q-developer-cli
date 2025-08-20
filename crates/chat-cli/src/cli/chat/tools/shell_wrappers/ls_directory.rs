use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    Result,
    bail,
};
use serde::Deserialize;

use super::super::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
    sanitize_path_tool_arg,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::sanitize_unicode_tags;
use crate::cli::chat::tools::execute::run_command;
use crate::os::Os;

#[derive(Debug, Clone, Deserialize)]
pub struct LsDirectory {
    pub path: String,
    pub show_hidden: Option<bool>,
    pub long_format: Option<bool>,
    pub recursive: Option<bool>,
    pub sort_by: Option<SortBy>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum SortBy {
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "size")]
    Size,
    #[serde(rename = "time")]
    Time,
}

impl LsDirectory {
    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("I will list directory contents: "),)?;

        queue!(
            output,
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::Print("\n"),
            style::ResetColor
        )?;

        Ok(())
    }

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        let sanitized_path = sanitize_path_tool_arg(os, &self.path);

        if !sanitized_path.exists() {
            bail!("Path does not exist: {}", self.path);
        }

        if !sanitized_path.is_dir() {
            bail!("Path is not a directory: {}", self.path);
        }

        self.path = sanitized_path.to_string_lossy().to_string();
        Ok(())
    }

    pub fn eval_perm(&self, agent: &Agent) -> PermissionEvalResult {
        _ = self;
        if agent.allowed_tools.contains("shell_read_only_ls_directory") {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }
    }

    pub async fn invoke(&self, os: &Os, output: &mut impl Write) -> Result<InvokeOutput> {
        let mut command = "ls".to_string();

        if self.long_format.unwrap_or(false) {
            command.push_str(" -l");
        }

        if self.show_hidden.unwrap_or(false) {
            command.push_str(" -a");
        }

        if self.recursive.unwrap_or(false) {
            command.push_str(" -R");
        }

        match &self.sort_by {
            Some(SortBy::Size) => command.push_str(" -S"),
            Some(SortBy::Time) => command.push_str(" -t"),
            _ => {},
        }

        command.push_str(&format!(" '{}'", self.path));

        let result = run_command(os, &command, MAX_TOOL_RESPONSE_SIZE / 3, Some(output)).await?;
        let clean_stdout = sanitize_unicode_tags(&result.stdout);
        let clean_stderr = sanitize_unicode_tags(&result.stderr);

        let json_result = serde_json::json!({
            "exit_status": result.exit_status.unwrap_or(0).to_string(),
            "stdout": clean_stdout,
            "stderr": clean_stderr,
        });

        Ok(InvokeOutput {
            output: OutputKind::Json(json_result),
        })
    }
}
