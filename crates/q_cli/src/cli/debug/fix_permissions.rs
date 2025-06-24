use std::fs::Metadata;
use std::io::ErrorKind;
use std::os::unix::fs::{
    MetadataExt as _,
    PermissionsExt,
};
use std::path::Path;

use anstream::println;
use color_eyre::owo_colors::OwoColorize;
use eyre::{
    Context,
    ContextCompat,
    Result,
    bail,
};
use fig_integrations::shell::ShellExt as _;
use fig_os_shim::Env;
use fig_util::directories::home_dir;
use fig_util::{
    CLI_BINARY_NAME,
    Shell,
};
use nix::libc::uid_t;
use nix::unistd::{
    Gid,
    Uid,
};
use tracing::info;

pub fn fix_permissions(env: &Env) -> Result<()> {
    let Ok(sudo_uid_str) = std::env::var("SUDO_UID") else {
        bail!("This command must be run with sudo");
    };

    let sudo_uid: uid_t = sudo_uid_str.parse().context("Invalid SUDO_UID")?;
    let user = nix::unistd::User::from_uid(sudo_uid.into())
        .context("Failed to get user from SUDO_UID")?
        .context("Failed to find user from SUDO_UID")?;

    info!(?user, "Loaded user");

    let home_dir = home_dir()?;

    let user_uid = user.uid;
    let user_gid = user.gid;

    let mut updated = false;

    for shell in Shell::all() {
        let shell_dir = shell.get_config_directory(env)?;

        // Only fix if the shell dir is not the home dir
        if shell_dir.is_dir() && shell_dir != home_dir {
            for entry in walkdir::WalkDir::new(shell_dir) {
                let entry = entry?;
                let metadata = entry.metadata()?;
                updated |= fix_permissions_for_path(entry.path(), &metadata, &user_uid, &user_gid)?;
            }
        }

        if let Ok(integrations) = shell.get_shell_integrations(env) {
            for integration in integrations {
                let path = integration.path();
                match std::fs::metadata(&path) {
                    Ok(metadata) => {
                        updated |= fix_permissions_for_path(&path, &metadata, &user_uid, &user_gid)?;
                    },
                    Err(err) if err.kind() == ErrorKind::NotFound => {},
                    Err(err) => return Err(err.into()),
                }
            }
        }
    }

    println!(
        "\n{}\n\nIf you continue to experience issues:\n  1. Run {}\n  2. Report the issue with {}\n",
        if updated {
            "The permissions should be fixed now!".bold()
        } else {
            "The permissions have not been modified".bold()
        },
        format!("{CLI_BINARY_NAME} doctor").magenta(),
        format!("{CLI_BINARY_NAME} issue").magenta(),
    );

    Ok(())
}

fn fix_permissions_for_path(path: &Path, metadata: &Metadata, user_uid: &Uid, user_gid: &Gid) -> Result<bool> {
    info!(
        "{}: permission {:o}, uid: {}, gid: {}",
        path.display(),
        metadata.permissions().mode(),
        metadata.uid(),
        metadata.gid()
    );

    // Skip symlinks
    if metadata.is_symlink() {
        return Ok(false);
    }

    let mut updated = false;

    // ensure owner is correct
    if metadata.uid() != user_uid.as_raw() || metadata.gid() != user_gid.as_raw() {
        println!("Fixing owner for {}", path.display().bold());
        nix::unistd::chown(path, Some(*user_uid), Some(*user_gid))?;
        updated = true;
    }

    if metadata.is_dir() && metadata.mode() & 0o700 != 0o700 {
        println!("Fixing permissions for {}", path.display().bold());
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o700);
        std::fs::set_permissions(path, permissions)?;
        updated = true;
    }

    if metadata.is_file() && metadata.mode() & 0o600 != 0o600 {
        println!("Fixing permissions for {}", path.display().bold());
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o600);
        std::fs::set_permissions(path, permissions)?;
        updated = true;
    }

    Ok(updated)
}
