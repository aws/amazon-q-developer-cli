use std::io::stdout;
use std::process::ExitCode;

use anstream::println;
use clap::Args;
use crossterm::style::Stylize;
use eyre::Result;
use fig_install::index::UpdatePackage;
use fig_install::{
    UpdateOptions,
    UpdateStatus,
};
use fig_ipc::local::{
    bundle_metadata_command,
    update_command,
};
use fig_os_shim::{
    Context,
    Os,
};
use fig_proto::local::command_response::Response;
use fig_proto::local::{
    CommandResponse,
    ErrorResponse,
};
use fig_settings::keys::UPDATE_AVAILABLE_KEY;
use fig_util::manifest::{
    BundleMetadata,
    FileType,
    Variant,
    manifest,
};
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};
use tracing::{
    error,
    info,
    warn,
};

use crate::util::dialoguer_theme;
use crate::util::spinner::{
    Spinner,
    SpinnerComponent,
};

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
        let ctx = Context::new();
        if ctx.platform().os() == Os::Linux && manifest().variant == Variant::Full {
            return try_linux_update().await;
        }

        let UpdateArgs {
            non_interactive,
            relaunch_dashboard,
            rollout,
        } = &self;

        let res = fig_install::update(
            Context::new(),
            Some(Box::new(|mut recv| {
                tokio::runtime::Handle::current().spawn(async move {
                    let progress_bar = indicatif::ProgressBar::new(100);
                    loop {
                        match recv.recv().await {
                            Some(UpdateStatus::Percent(p)) => {
                                progress_bar.set_position(p as u64);
                            },
                            Some(UpdateStatus::Message(m)) => {
                                progress_bar.set_message(m);
                            },
                            Some(UpdateStatus::Error(e)) => {
                                progress_bar.abandon();
                                return Err(eyre::eyre!(e));
                            },
                            Some(UpdateStatus::Exit) | None => {
                                progress_bar.finish_with_message("Done!");
                                break;
                            },
                        }
                    }
                    Ok(())
                });
            })),
            UpdateOptions {
                ignore_rollout: !rollout,
                interactive: !non_interactive,
                relaunch_dashboard: *relaunch_dashboard,
            },
        )
        .await;

        match res {
            Ok(true) => {
                if let Err(err) = fig_settings::state::remove_value(UPDATE_AVAILABLE_KEY) {
                    warn!("Failed to remove update.new-version-available: {:?}", err);
                }
                Ok(ExitCode::SUCCESS)
            },
            Ok(false) => {
                println!(
                    "No updates available, \n{} is the latest version.",
                    env!("CARGO_PKG_VERSION").bold()
                );
                Ok(ExitCode::SUCCESS)
            },
            Err(err) => eyre::bail!(
                "{err}\n\nIf this is unexpected, try running {} and then try again.\n",
                format!("{CLI_BINARY_NAME} doctor").bold()
            ),
        }
    }
}

async fn try_linux_update() -> Result<ExitCode> {
    match (fig_install::check_for_updates(true).await, bundle_metadata().await) {
        (ref update_result @ Ok(Some(ref pkg)), Some(file_type)) => {
            if file_type == FileType::AppImage {
                let should_continue = dialoguer::Select::with_theme(&dialoguer_theme())
                    .with_prompt(format!(
                        "A new version of {} is available: {}\nWould you like to update now?",
                        PRODUCT_NAME, pkg.version
                    ))
                    .items(&["Yes", "No"])
                    .default(0)
                    .interact_opt()?;

                if should_continue == Some(0) {
                    let mut spinner = Spinner::new(vec![
                        SpinnerComponent::Spinner,
                        SpinnerComponent::Text(format!(" Updating {PRODUCT_NAME}, please wait...")),
                    ]);
                    tokio::spawn(async {
                        tokio::signal::ctrl_c().await.unwrap();
                        println!(
                            "\nThe app is still updating. You can view the progress by running {}",
                            format!("{CLI_BINARY_NAME} debug logs").bold()
                        );
                        crossterm::execute!(stdout(), crossterm::cursor::Show).unwrap();
                        #[allow(clippy::exit)]
                        std::process::exit(0);
                    });

                    let update_cmd_result = update_command(true).await;
                    println!("");
                    match update_cmd_result {
                        Ok(Some(CommandResponse {
                            response: Some(Response::Success(_)),
                            ..
                        })) => {
                            spinner.stop_with_message("Update complete".into());
                            if let Err(err) = fig_settings::state::remove_value(UPDATE_AVAILABLE_KEY) {
                                warn!("Failed to remove update.new-version-available: {:?}", err);
                            }
                            Ok(ExitCode::SUCCESS)
                        },
                        Ok(Some(CommandResponse {
                            response: Some(Response::Error(ErrorResponse { message, .. })),
                            ..
                        })) => {
                            spinner.stop();
                            let message = message.unwrap_or("An unknown error occurred attempting to update".into());
                            eyre::bail!(
                                "{message}\n\nFailed to update. If this is unexpected, try running {} and then try again.\n",
                                format!("{CLI_BINARY_NAME} doctor").bold()
                            )
                        },
                        Ok(_) => {
                            // This case shouldn't happen, we expect a response from the desktop
                            // app.
                            spinner.stop_with_message("Update complete".into());
                            Ok(ExitCode::SUCCESS)
                        },
                        Err(err) => {
                            spinner.stop();
                            match err {
                                fig_ipc::Error::Timeout => {
                                    eyre::bail!(
                                        "Timed out while waiting for the app to update. Updating may still be in progress - you can view app logs by running {}",
                                        format!("{CLI_BINARY_NAME} debug logs").bold()
                                    )
                                },
                                err => {
                                    eyre::bail!(
                                        "{err}\n\nFailed to update. If this is unexpected, try running {} and then try again.\n",
                                        format!("{CLI_BINARY_NAME} doctor").bold()
                                    )
                                },
                            }
                        },
                    }
                } else {
                    println!("Cancelled");
                    Ok(ExitCode::FAILURE)
                }
            } else {
                display_update_check_result(update_result)
            }
        },
        (update_result, _) => display_update_check_result(&update_result),
    }
}

/// Tries to get the bundle metadata packaged with the desktop app, returning [Option::None] if
/// either an error was encountered, or no metadata was found.
async fn bundle_metadata() -> Option<FileType> {
    match bundle_metadata_command().await {
        Ok(metadata) => match metadata.json {
            Some(metadata) => match serde_json::from_str::<BundleMetadata>(&metadata) {
                Ok(bundle_metadata) => Some(bundle_metadata.packaged_as),
                Err(err) => {
                    error!("Unable to parse the bundled metadata: {:?}", err);
                    None
                },
            },
            None => {
                info!("No bundled metadata was found");
                None
            },
        },
        Err(err) => {
            error!("An error occurred checking for updates: {:?}", err);
            None
        },
    }
}

fn display_update_check_result(
    check_for_updates_result: &Result<Option<UpdatePackage>, fig_install::Error>,
) -> Result<ExitCode> {
    match check_for_updates_result {
        Ok(Some(pkg)) => {
            println!("A new version of {} is available: {}", CLI_BINARY_NAME, pkg.version);
            Ok(ExitCode::SUCCESS)
        },
        Ok(None) => {
            println!(
                "No updates available, \n{} is the latest version.",
                env!("CARGO_PKG_VERSION").bold()
            );
            Ok(ExitCode::SUCCESS)
        },
        Err(err) => {
            eyre::bail!(
                "{err}\n\nFailed checking for updates. If this is unexpected, try running {} and then try again.\n",
                format!("{CLI_BINARY_NAME} doctor").bold()
            )
        },
    }
}
