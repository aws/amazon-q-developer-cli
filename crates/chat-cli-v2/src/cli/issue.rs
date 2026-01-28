use std::process::ExitCode;

use clap::Args;
use eyre::Result;

use crate::os::Os;

#[derive(Clone, Debug, Args, PartialEq, Eq)]
pub struct IssueArgs {
    /// Force issue creation
    #[arg(long, short = 'f')]
    force: bool,
    /// Issue description
    description: Vec<String>,
}

impl IssueArgs {
    pub async fn execute(&self, os: &Os) -> Result<ExitCode> {
        let joined_description = self.description.join(" ").trim().to_owned();

        let issue_title = match joined_description.len() {
            0 => dialoguer::Input::with_theme(&crate::util::dialoguer_theme())
                .with_prompt("Issue Title")
                .interact_text()?,
            _ => joined_description,
        };

        let _ = crate::cli::chat::legacy::util::issue::IssueCreator {
            title: Some(issue_title),
            expected_behavior: None,
            actual_behavior: None,
            steps_to_reproduce: None,
            additional_environment: None,
        }
        .create_url(os)
        .await;

        Ok(ExitCode::SUCCESS)
    }
}
