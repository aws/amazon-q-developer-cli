use std::ffi::{
    CStr,
    CString,
    OsStr,
};
use std::os::unix::prelude::{
    OsStrExt,
    PermissionsExt,
};
use std::os::unix::process::CommandExt;
use std::path::{
    Path,
    PathBuf,
};

use fig_util::consts::{
    APP_BUNDLE_ID,
    CLI_BINARY_NAME,
};
use fig_util::macos::BUNDLE_CONTENTS_MACOS_PATH;
use fig_util::{
    APP_BUNDLE_NAME,
    directories,
};
use regex::Regex;
use security_framework::authorization::{
    Authorization,
    AuthorizationItemSetBuilder,
    Flags as AuthorizationFlags,
};
use tokio::fs;
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::sync::mpsc::Sender;
use tracing::{
    debug,
    error,
    warn,
};

use crate::download::download_file;
use crate::index::UpdatePackage;
use crate::{
    Error,
    UpdateStatus,
};

pub(crate) async fn update(
    update: UpdatePackage,
    tx: Sender<UpdateStatus>,
    interactive: bool,
    relaunch_dashboard: bool,
) -> Result<(), Error> {
    debug!("starting update");

    // Get all of the paths up front so we can get an error early if something is wrong
    let temp_dir = tempfile::Builder::new()
        .prefix(&format!("{CLI_BINARY_NAME}-download"))
        .tempdir()?;

    let dmg_name = update
        .download_url
        .path_segments()
        .and_then(|s| s.last())
        .unwrap_or(APP_BUNDLE_NAME);

    let dmg_path = temp_dir.path().join(dmg_name);

    // Set the permissions to 700 so that only the user can read and write
    let permissions = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(temp_dir.path(), permissions)?;

    debug!(?dmg_path, "downloading dmg");

    let real_hash = download_file(update.download_url, &dmg_path, update.size, Some(tx.clone())).await?;

    // validate the dmg hash
    let expected_hash = update.sha256;
    if expected_hash != real_hash {
        return Err(Error::UpdateFailed(format!(
            "dmg hash mismatch. Expected: {expected_hash}, Actual: {real_hash}"
        )));
    }

    tx.send(UpdateStatus::Message("Unpacking update...".into())).await.ok();

    // Shell out to hdiutil to mount the dmg
    let hdiutil_attach_output = tokio::process::Command::new("hdiutil")
        .arg("attach")
        .arg(&dmg_path)
        .args(["-readonly", "-nobrowse", "-plist"])
        .output()
        .await?;

    if !hdiutil_attach_output.status.success() {
        return Err(Error::UpdateFailed(
            String::from_utf8_lossy(&hdiutil_attach_output.stderr).to_string(),
        ));
    }

    debug!("mounted dmg");

    let plist = String::from_utf8_lossy(&hdiutil_attach_output.stdout).to_string();

    let regex = Regex::new(r"<key>mount-point</key>\s*<\S+>([^<]+)</\S+>").unwrap();
    let mount_point = PathBuf::from(
        regex
            .captures(&plist)
            .unwrap()
            .get(1)
            .expect("mount-point will always exist")
            .as_str(),
    );

    let all_entries = std::fs::read_dir(&mount_point)?
        .filter_map(|entry| entry.ok())
        .collect::<Vec<_>>();

    let all_entries_name = all_entries
        .iter()
        .map(|entry| entry.path().to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    // find `.app` file in the dmg
    let app_entries = all_entries
        .iter()
        .filter(|entry| {
            let entry_path = entry.path();
            entry_path.is_dir() && entry_path.extension() == Some(OsStr::new("app"))
        })
        .collect::<Vec<_>>();

    if app_entries.is_empty() {
        return Err(Error::UpdateFailed(format!(
            "the update dmg is missing a .app directory: {}",
            all_entries_name.join(", ")
        )));
    }
    if app_entries.len() > 1 {
        return Err(Error::UpdateFailed(format!(
            "the update dmg has more than one .app directory: {}",
            all_entries_name.join(", ")
        )));
    }

    let mount_app_path = app_entries[0].path();

    let app_name = match mount_app_path.file_name() {
        Some(name) => Path::new(name),
        None => {
            return Err(Error::UpdateFailed("No app name found in the update dmg".into()));
        },
    };

    let temp_app_path = temp_dir.path().join(app_name);
    let temp_app_path_cstr = CString::new(temp_app_path.as_os_str().as_bytes())?;

    let ditto_output = tokio::process::Command::new("ditto")
        .arg(&mount_app_path)
        .arg(&temp_app_path)
        .output()
        .await?;

    if !ditto_output.status.success() {
        return Err(Error::UpdateFailed(
            String::from_utf8_lossy(&ditto_output.stderr).to_string(),
        ));
    }

    tx.send(UpdateStatus::Message("Installing update...".into())).await.ok();

    // This points at the currently installed CLI
    let cli_path = fig_util::app_bundle_path()
        .join(BUNDLE_CONTENTS_MACOS_PATH)
        .join(CLI_BINARY_NAME);

    if !cli_path.exists() {
        return Err(Error::UpdateFailed(format!(
            "the current app bundle is missing the CLI with the correct name {CLI_BINARY_NAME}"
        )));
    }

    let same_bundle_name = app_name == Path::new(APP_BUNDLE_NAME);

    let installed_app_path = if same_bundle_name {
        fig_util::app_bundle_path()
    } else {
        Path::new("/Applications").join(app_name)
    };

    let installed_app_path_cstr = CString::new(installed_app_path.as_os_str().as_bytes())?;

    match install(&temp_app_path_cstr, &installed_app_path_cstr, same_bundle_name) {
        Ok(()) => debug!("swapped app bundle"),
        // Try to elevate permissions if we can't swap the app bundle and in interactive mode
        Err(err) if interactive => {
            error!(?err, "failed to swap app bundle, trying to elevate permissions");

            let mut file = {
                let rights = AuthorizationItemSetBuilder::new()
                    .add_right("system.privilege.admin")?
                    .build();

                let auth = Authorization::new(
                    Some(rights),
                    None,
                    AuthorizationFlags::DEFAULTS
                        | AuthorizationFlags::INTERACTION_ALLOWED
                        | AuthorizationFlags::PREAUTHORIZE
                        | AuthorizationFlags::EXTEND_RIGHTS,
                )?;

                let mut arguments = vec![OsStr::new("_"), OsStr::new("swap-files")];

                if !same_bundle_name {
                    arguments.push(OsStr::new("--not-same-bundle-name"));
                }

                arguments.extend([temp_app_path.as_os_str(), installed_app_path.as_os_str()]);

                let file = auth.execute_with_privileges_piped(&cli_path, arguments, AuthorizationFlags::DEFAULTS)?;

                fs::File::from_std(file)
            };

            let mut out = String::new();
            file.read_to_string(&mut out).await?;

            match out.trim() {
                "success" => {
                    debug!("swapped app bundle");
                },
                other => {
                    return Err(Error::UpdateFailed(other.to_owned()));
                },
            }
        },
        Err(err) => return Err(err),
    }

    // Shell out to unmount the dmg
    let output = tokio::process::Command::new("hdiutil")
        .arg("detach")
        .arg(&mount_point)
        .output()
        .await?;

    if !output.status.success() {
        error!(command =% String::from_utf8_lossy(&output.stderr).to_string(), "the update succeeded, but unmounting the dmg failed");
    } else {
        debug!("unmounted dmg");
    }

    // This points at the newly installed CLI via the cli symlink
    let new_cli_path = match update.cli_path {
        Some(path) => installed_app_path.join(path),
        None => installed_app_path
            .join(BUNDLE_CONTENTS_MACOS_PATH)
            .join(CLI_BINARY_NAME),
    };
    if !new_cli_path.exists() {
        return Err(Error::UpdateFailed(format!(
            "the update succeeded, but the cli did not have the expected name or was missing, expected {CLI_BINARY_NAME}"
        )));
    }

    debug!(?new_cli_path, "using cli at path");

    tx.send(UpdateStatus::Message("Relaunching...".into())).await.ok();

    debug!("restarting app");
    let mut cmd = std::process::Command::new(&new_cli_path);
    cmd.process_group(0).args(["_", "finish-update"]);

    // If the bundle name changed, delete the old bundle
    if !same_bundle_name {
        cmd.arg("--delete-bundle").arg(fig_util::app_bundle_path());
    }

    if relaunch_dashboard {
        cmd.arg("--relaunch-dashboard");
    }

    cmd.spawn()?;

    tx.send(UpdateStatus::Exit).await.ok();

    #[allow(clippy::exit)]
    std::process::exit(0);
}

async fn remove_in_dir_with_prefix_unless(dir: &Path, prefix: &str, unless: impl Fn(&str) -> bool) {
    if let Ok(mut entries) = fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(prefix) && !unless(name) {
                    fs::remove_file(entry.path()).await.ok();
                    fs::remove_dir_all(entry.path()).await.ok();
                }
            }
        }
    }
}

#[allow(unused_variables)]
pub(crate) async fn uninstall_desktop(ctx: &fig_os_shim::Context) -> Result<(), Error> {
    // TODO:
    // 1. Set title of running ttys "Restart this terminal to finish uninstalling Q..."
    // 2. Delete webview cache

    // Remove launch agents
    if let Ok(home) = directories::home_dir() {
        let launch_agents = home.join("Library").join("LaunchAgents");
        remove_in_dir_with_prefix_unless(&launch_agents, "com.amazon.codewhisperer.", |p| p.contains("daemon")).await;
    } else {
        warn!("Could not find home directory");
    }

    // Delete Fig defaults on macOS
    tokio::process::Command::new("defaults")
        .args(["delete", APP_BUNDLE_ID])
        .output()
        .await
        .map_err(|err| warn!("Failed to delete defaults: {err}"))
        .ok();

    tokio::process::Command::new("defaults")
        .args(["delete", "com.amazon.codewhisperer.shared"])
        .output()
        .await
        .map_err(|err| warn!("Failed to delete defaults: {err}"))
        .ok();

    uninstall_terminal_integrations().await;

    // Delete data dir
    if let Ok(fig_data_dir) = directories::fig_data_dir() {
        let state = fig_settings::state::get_string("anonymousId").unwrap_or_default();

        for file in std::fs::read_dir(fig_data_dir).ok().into_iter().flatten().flatten() {
            if let Some(file_name) = file.file_name().to_str() {
                if file_name == "credentials.json" {
                } else if file_name == "state.json" {
                    std::fs::write(file.path(), serde_json::json!({ "anonymousId": state }).to_string())
                        .map_err(|err| warn!("Failed to write state.json: {err}"))
                        .ok();
                } else if let Ok(metadata) = file.metadata() {
                    if metadata.is_dir() {
                        fs::remove_dir_all(file.path())
                            .await
                            .map_err(|err| warn!("Failed to remove data dir: {err}"))
                            .ok();
                    } else {
                        fs::remove_file(file.path())
                            .await
                            .map_err(|err| warn!("Failed to remove data dir: {err}"))
                            .ok();
                    }
                }
            }
        }
    }

    let app_path = fig_util::app_bundle_path();
    if app_path.exists() {
        fs::remove_dir_all(&app_path)
            .await
            .map_err(|err| warn!("Failed to remove {app_path:?}: {err}"))
            .ok();
    }

    // Remove the previous codewhisperer data dir only if it is a symlink.
    if let Ok(old_fig_data_dir) = directories::old_fig_data_dir() {
        if old_fig_data_dir.exists() {
            if let Ok(metadata) = fs::symlink_metadata(&old_fig_data_dir).await {
                if metadata.is_symlink() {
                    fs::remove_file(&old_fig_data_dir)
                        .await
                        .map_err(|err| error!("Failed to remove the old fig data dir {old_fig_data_dir:?}: {err}"))
                        .ok();
                }
            }
        }
    }

    Ok(())
}

pub async fn uninstall_terminal_integrations() {
    // Delete integrations
    if let Ok(home) = directories::home_dir() {
        // Delete iTerm integration
        for path in &[
            "Library/Application Support/iTerm2/Scripts/AutoLaunch/fig-iterm-integration.py",
            ".config/iterm2/AppSupport/Scripts/AutoLaunch/fig-iterm-integration.py",
            "Library/Application Support/iTerm2/Scripts/AutoLaunch/fig-iterm-integration.scpt",
        ] {
            fs::remove_file(home.join(path))
                .await
                .map_err(|err| warn!("Could not remove iTerm integration {path}: {err}"))
                .ok();
        }

        // Delete VSCode integration
        for (folder, prefix) in &[
            (".vscode/extensions", "withfig.fig-"),
            (".vscode-insiders/extensions", "withfig.fig-"),
            (".vscode-oss/extensions", "withfig.fig-"),
            (".cursor/extensions", "withfig.fig-"),
            (".cursor-nightly/extensions", "withfig.fig-"),
        ] {
            let folder = home.join(folder);
            remove_in_dir_with_prefix_unless(&folder, prefix, |_| false).await;
        }

        // Remove Hyper integration
        let hyper_path = home.join(".hyper.js");
        if hyper_path.exists() {
            // Read the config file
            match fs::File::open(&hyper_path).await {
                Ok(mut file) => {
                    let mut contents = String::new();
                    match file.read_to_string(&mut contents).await {
                        Ok(_) => {
                            contents = contents.replace("\"fig-hyper-integration\",", "");
                            contents = contents.replace("\"fig-hyper-integration\"", "");

                            // Write the config file
                            match fs::File::create(&hyper_path).await {
                                Ok(mut file) => {
                                    file.write_all(contents.as_bytes())
                                        .await
                                        .map_err(|err| warn!("Could not write to Hyper config: {err}"))
                                        .ok();
                                },
                                Err(err) => {
                                    warn!("Could not create Hyper config: {err}");
                                },
                            }
                        },
                        Err(err) => {
                            warn!("Could not read Hyper config: {err}");
                        },
                    }
                },
                Err(err) => {
                    warn!("Could not open Hyper config: {err}");
                },
            }
        }

        // Remove Kitty integration
        let kitty_path = home.join(".config").join("kitty").join("kitty.conf");
        if kitty_path.exists() {
            // Read the config file
            match fs::File::open(&kitty_path).await {
                Ok(mut file) => {
                    let mut contents = String::new();
                    match file.read_to_string(&mut contents).await {
                        Ok(_) => {
                            contents = contents.replace("watcher ${HOME}/.fig/tools/kitty-integration.py", "");
                            // Write the config file
                            match fs::File::create(&kitty_path).await {
                                Ok(mut file) => {
                                    file.write_all(contents.as_bytes())
                                        .await
                                        .map_err(|err| warn!("Could not write to Kitty config: {err}"))
                                        .ok();
                                },
                                Err(err) => {
                                    warn!("Could not create Kitty config: {err}");
                                },
                            }
                        },
                        Err(err) => {
                            warn!("Could not read Kitty config: {err}");
                        },
                    }
                },
                Err(err) => {
                    warn!("Could not open Kitty config: {err}");
                },
            }
        }
        // TODO: Add Jetbrains integration
    }
}

pub fn install(src: impl AsRef<CStr>, dst: impl AsRef<CStr>, same_bundle_name: bool) -> Result<(), Error> {
    // We want to swap the app bundles, like sparkle does
    // https://github.com/sparkle-project/Sparkle/blob/863f85b5f5398c03553f2544668b95816b2860db/Sparkle/SUFileManager.m#L235
    let status = unsafe {
        libc::renamex_np(
            src.as_ref().as_ptr(),
            dst.as_ref().as_ptr(),
            if same_bundle_name { libc::RENAME_SWAP } else { 0 },
        )
    };

    if status != 0 {
        let err = std::io::Error::last_os_error();

        error!(%err, "failed to swap app bundle");

        if matches!(err.kind(), std::io::ErrorKind::PermissionDenied) {
            return Err(Error::UpdateFailed(
                "Failed to swap app bundle due to permission denied. Try restarting the app.".into(),
            ));
        } else {
            return Err(Error::UpdateFailed(format!("Failed to swap app bundle: {err}")));
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use fig_util::manifest::Channel;
    use tempfile::TempDir;

    use super::*;

    #[ignore]
    #[tokio::test]
    async fn test_download_dmg() -> Result<(), Error> {
        let index = crate::index::pull(&Channel::Stable).await.unwrap();
        let version = index.latest().unwrap();
        let dmg_pkg = version.packages.first().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let dmg_path = temp_dir.path().join("CodeWhisperer.dmg");
        let real_hash = download_file(dmg_pkg.download.clone(), dmg_path, 0, None)
            .await
            .unwrap();
        println!("{real_hash}");

        assert_eq!(dmg_pkg.sha256, real_hash);
        Ok(())
    }
}
