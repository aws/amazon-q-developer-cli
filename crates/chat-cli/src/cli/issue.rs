use std::process::ExitCode;

use clap::Args;
use eyre::Result;

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
        let joined_description = self.description.join(" ").trim().to_owned();

        let issue_title = match joined_description.len() {
            0 => dialoguer::Input::with_theme(&crate::fig_util::dialoguer_theme())
                .with_prompt("Issue Title")
                .interact_text()?,
            _ => joined_description,
        };

        let _ = crate::cli::chat::util::issue::IssueCreator {
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
