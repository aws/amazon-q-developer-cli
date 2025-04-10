use std::process::ExitCode;

use anstream::println;
use crossterm::style::Stylize;
use eyre::Result;
use fig_os_shim::Context;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};

use crate::util::dialoguer_theme;

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

    let ctx = Context::new();
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            uninstall(ctx).await?;
        } else if #[cfg(target_os = "linux")] {
            use fig_util::manifest::is_minimal;
            if is_minimal() {
                uninstall_linux_minimal(ctx).await?;
            } else {
                uninstall_linux_full(ctx).await?;
            }

        }
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(target_os = "macos")]
async fn uninstall(ctx: std::sync::Arc<fig_os_shim::Context>) -> Result<()> {
    use fig_install::UNINSTALL_URL;
    use tracing::error;

    if let Err(err) = fig_util::open_url_async(UNINSTALL_URL).await {
        error!(%err, %UNINSTALL_URL, "Failed to open uninstall url");
    }

    fig_auth::logout().await.ok();
    fig_install::uninstall(fig_install::InstallComponents::all(), ctx).await?;

    Ok(())
}

#[cfg(target_os = "linux")]
async fn uninstall_linux_minimal(ctx: std::sync::Arc<fig_os_shim::Context>) -> Result<()> {
    use eyre::bail;
    use tracing::error;

    let exe_path = ctx.fs().canonicalize(ctx.env().current_exe()?.canonicalize()?).await?;
    let Some(exe_name) = exe_path.file_name().and_then(|s| s.to_str()) else {
        bail!("Failed to get name of current executable: {exe_path:?}")
    };
    let Some(exe_parent) = exe_path.parent() else {
        bail!("Failed to get parent of current executable: {exe_path:?}")
    };
    // canonicalize to handle if the home dir is a symlink (like on Dev Desktops)
    let local_bin = fig_util::directories::home_local_bin_ctx(&ctx)?.canonicalize()?;

    if exe_parent != local_bin {
        bail!(
            "Uninstall is only supported for binaries installed in {local_bin:?}, the current executable is in {exe_parent:?}"
        );
    }

    if exe_name != CLI_BINARY_NAME {
        bail!("Uninstall is only supported for {CLI_BINARY_NAME:?}, the current executable is {exe_name:?}");
    }

    if let Err(err) = fig_auth::logout().await {
        error!(%err, "Failed to logout");
    }
    fig_install::uninstall(fig_install::InstallComponents::all_linux_minimal(), ctx).await?;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn uninstall_linux_full(ctx: std::sync::Arc<fig_os_shim::Context>) -> Result<()> {
    use eyre::bail;
    use fig_install::{
        InstallComponents,
        UNINSTALL_URL,
        uninstall,
    };
    use tracing::error;

    // TODO: Add a better way to distinguish binaries distributed between AppImage and package
    // managers.
    // We want to support q uninstall for AppImage, but not for package managers.
    match ctx.process_info().current_pid().exe() {
        Some(exe) => {
            let Some(exe_parent) = exe.parent() else {
                bail!("Failed to get parent of current executable: {exe:?}")
            };
            let local_bin = fig_util::directories::home_local_bin_ctx(&ctx)?.canonicalize()?;
            if exe_parent != local_bin {
                bail!(
                    "Managed uninstalls are not supported. Please use your package manager to uninstall {}",
                    PRODUCT_NAME
                );
            }
        },
        None => bail!("Unable to determine the current process executable."),
    }

    if let Err(err) = fig_util::open_url_async(UNINSTALL_URL).await {
        error!(%err, %UNINSTALL_URL, "Failed to open uninstall url");
    }

    if let Err(err) = fig_auth::logout().await {
        error!(%err, "Failed to logout");
    }
    uninstall(InstallComponents::all(), ctx).await?;
    Ok(())
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
async fn uninstall() -> Result<()> {
    eyre::bail!("Guided uninstallation is not supported on this platform. Please uninstall manually.");
}

// #[cfg(target_os = "linux")]
// mod linux {
//     use eyre::Result;
//
//     pub async fn uninstall_apt(pkg: String) -> Result<()> {
//         tokio::process::Command::new("apt")
//             .arg("remove")
//             .arg("-y")
//             .arg(pkg)
//             .status()
//             .await?;
//         std::fs::remove_file("/etc/apt/sources.list.d/fig.list")?;
//         std::fs::remove_file("/etc/apt/keyrings/fig.gpg")?;
//
//         Ok(())
//     }
//
//     pub async fn uninstall_dnf(pkg: String) -> Result<()> {
//         tokio::process::Command::new("dnf")
//             .arg("remove")
//             .arg("-y")
//             .arg(pkg)
//             .status()
//             .await?;
//         std::fs::remove_file("/etc/yum.repos.d/fig.repo")?;
//
//         Ok(())
//     }
//
//     pub async fn uninstall_pacman(pkg: String) -> Result<()> {
//         tokio::process::Command::new("pacman")
//             .arg("-Rs")
//             .arg(pkg)
//             .status()
//             .await?;
//
//         Ok(())
//     }
// }
