use std::process::ExitCode;

use anstream::println;
use crossterm::style::Stylize;
use eyre::Result;

use crate::util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
    dialoguer_theme,
};

pub async fn uninstall_command(no_confirm: bool) -> Result<ExitCode> {
    if !no_confirm {
        println!(
            "\nIs {PRODUCT_NAME} not working? Try running {}\n",
            format!("{CLI_BINARY_NAME} doctor").bold().magenta()
        );
        let should_continue = dialoguer::Select::with_theme(&dialoguer_theme())
            .with_prompt(format!("Are you sure want to continue uninstalling {PRODUCT_NAME}?"))
            .items(&["Yes", "No"])
            .default(0)
            .interact_opt()?;

        if should_continue == Some(0) {
            println!("Uninstalling {PRODUCT_NAME}");
        } else {
            println!("Cancelled");
            return Ok(ExitCode::FAILURE);
        }
    };

    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            uninstall().await?;
        } else if #[cfg(target_os = "linux")] {
            ();
        }
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(target_os = "macos")]
async fn uninstall() -> Result<()> {
    crate::auth::logout().await.ok();
    crate::install::uninstall().await?;
    Ok(())
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
async fn uninstall() -> Result<()> {
    eyre::bail!("Guided uninstallation is not supported on this platform. Please uninstall manually.");
}
