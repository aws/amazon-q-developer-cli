use std::process::ExitCode;

use anstream::println;
use clap::Args;
use crossterm::style::Stylize;
use eyre::Result;
use fig_util::system_info::is_remote;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};

#[derive(Debug, Args, PartialEq, Eq)]
pub struct IssueArgs {
    /// Force issue creation
    #[arg(long, short = 'f')]
    force: bool,
    /// Issue description
    description: Vec<String>,
}

impl IssueArgs {
    #[allow(unreachable_code)]
    pub async fn execute(&self) -> Result<ExitCode> {
        // Check if fig is running
        if !(self.force || is_remote() || crate::util::desktop::desktop_app_running()) {
            println!(
                "\n→ {PRODUCT_NAME} is not running.\n  Please launch {PRODUCT_NAME} with {} or run {} to create the issue anyways",
                format!("{CLI_BINARY_NAME} launch").magenta(),
                format!("{CLI_BINARY_NAME} issue --force").magenta()
            );
            return Ok(ExitCode::FAILURE);
        }

        let joined_description = self.description.join(" ").trim().to_owned();

        let issue_title = match joined_description.len() {
            0 => dialoguer::Input::with_theme(&crate::util::dialoguer_theme())
                .with_prompt("Issue Title")
                .interact_text()?,
            _ => joined_description,
        };

        let _ = q_chat::util::issue::IssueCreator {
            title: Some(issue_title),
            expected_behavior: None,
            actual_behavior: None,
            steps_to_reproduce: None,
            additional_environment: None,
        }
        .create_url()
        .await;

        Ok(ExitCode::SUCCESS)
    }
}
