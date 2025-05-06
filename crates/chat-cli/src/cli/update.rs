use std::process::ExitCode;

use anstream::println;
use clap::Args;
use crossterm::style::Stylize;
use eyre::Result;

use crate::fig_util::CLI_BINARY_NAME;

#[derive(Debug, PartialEq, Args)]
pub struct UpdateArgs {
    /// Don't prompt for confirmation
    #[arg(long, short = 'y')]
    non_interactive: bool,
    /// Relaunch into dashboard after update (false will launch in background)
    #[arg(long, default_value = "true")]
    relaunch_dashboard: bool,
    /// Uses rollout
    #[arg(long)]
    rollout: bool,
}

impl UpdateArgs {
    pub async fn execute(&self) -> Result<ExitCode> {
        todo!();

        // let res = self_update::backends::s3::Update::configure()
        //     .bucket_name("self_update_releases")
        //     .asset_prefix("something/self_update")
        //     .region("eu-west-2")
        //     .bin_name("self_update_example")
        //     .show_download_progress(true)
        //     .current_version(cargo_crate_version!())
        //     .build()?
        //     .update();
        //
        // match res {
        //     Ok(Status::UpToDate(_)) => {
        //         println!(
        //             "No updates available, \n{} is the latest version.",
        //             env!("CARGO_PKG_VERSION").bold()
        //         );
        //         Ok(ExitCode::SUCCESS)
        //     },
        //     Ok(Status::Updated(_)) => Ok(ExitCode::SUCCESS),
        //     Err(err) => {
        //         eyre::bail!(
        //             "{err}\n\nIf this is unexpected, try running {} and then try again.\n",
        //             format!("{CLI_BINARY_NAME} doctor").bold()
        //         )
        //     },
        // }
    }
}
