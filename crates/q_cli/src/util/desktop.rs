use std::process::Command;

use eyre::{
    Result,
    eyre,
};
use fig_util::{
    PRODUCT_NAME,
    directories,
    manifest,
    system_info,
};

pub struct LaunchArgs {
    /// Should we wait for the socket to continue execution
    pub wait_for_socket: bool,
    /// Should we open the dashboard right away
    ///
    /// Note that this won't open the dashboard if the app is already running
    pub open_dashboard: bool,
    /// Should we do the first update check or skip it
    pub immediate_update: bool,
    /// Print output to user
    pub verbose: bool,
}

#[cfg(target_os = "macos")]
pub fn desktop_app_running() -> bool {
    use std::ffi::OsString;

    use fig_util::consts::{
        APP_BUNDLE_ID,
        APP_PROCESS_NAME,
    };
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::ns_string;
    use sysinfo::{
        ProcessRefreshKind,
        RefreshKind,
        System,
    };

    let bundle_id = ns_string!(APP_BUNDLE_ID);
    let running_applications = unsafe { NSRunningApplication::runningApplicationsWithBundleIdentifier(bundle_id) };

    if !running_applications.is_empty() {
        return true;
    }

    // Fallback to process name check
    let app_process_name = OsString::from(APP_PROCESS_NAME);
    let system = System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
    let mut processes = system.processes_by_exact_name(&app_process_name);
    processes.next().is_some()
}

#[cfg(target_os = "windows")]
pub fn desktop_app_running() -> bool {
    use crate::consts::APP_PROCESS_NAME;

    let output = match std::process::Command::new("tasklist.exe")
        .args(["/NH", "/FI", "IMAGENAME eq fig_desktop.exe"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return false,
    };

    match std::str::from_utf8(&output.stdout) {
        Ok(result) => result.contains(CODEWHISPERER_DESKTOP_PROCESS_NAME),
        Err(_) => false,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn desktop_app_running() -> bool {
    use std::ffi::OsString;

    use fig_util::consts::APP_PROCESS_NAME;
    use sysinfo::{
        ProcessRefreshKind,
        RefreshKind,
        System,
    };

    let s = System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
    let app_process_name = OsString::from(APP_PROCESS_NAME);
    let mut processes = s.processes_by_exact_name(&app_process_name);
    processes.next().is_some()
}

pub fn launch_fig_desktop(args: LaunchArgs) -> Result<()> {
    if manifest::is_minimal() {
        return Err(eyre!(
            "launching {PRODUCT_NAME} from minimal installs is not yet supported"
        ));
    }

    if system_info::is_remote() {
        return Err(eyre!(
            "launching {PRODUCT_NAME} from remote installs is not yet supported"
        ));
    }

    match desktop_app_running() {
        true => return Ok(()),
        false => {
            if args.verbose {
                println!("Launching {PRODUCT_NAME}...");
            }
        },
    }

    std::fs::remove_file(directories::desktop_socket_path()?).ok();

    let mut common_args = vec![];
    if !args.open_dashboard {
        common_args.push("--no-dashboard");
    }
    if !args.immediate_update {
        common_args.push("--ignore-immediate-update");
    }

    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            let output = Command::new("open")
                .args(["-g", "-b", fig_util::consts::APP_BUNDLE_ID, "--args"])
                .args(common_args)
                .output()?;

            if !output.status.success() {
                eyre::bail!("failed to launch: {}", String::from_utf8_lossy(&output.stderr));
            }
        } else if #[cfg(windows)] {
            use std::os::windows::process::CommandExt;
            use windows::Win32::System::Threading::DETACHED_PROCESS;

            Command::new("fig_desktop")
                .creation_flags(DETACHED_PROCESS.0)
                .spawn()?;
        } else {
            let state = fig_settings::State::new();
            let ctx = fig_os_shim::Context::new();
            launch_linux_desktop(ctx, &state)?;
            // Need to wait some time for the app to launch and appear in the process list.
            // 1 second to be safe.
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    }

    if !args.wait_for_socket {
        return Ok(());
    }

    if !desktop_app_running() {
        return Err(eyre!("{PRODUCT_NAME} was unable launch successfully"));
    }

    // Wait for socket to exist
    let path = directories::desktop_socket_path()?;

    cfg_if::cfg_if! {
        if #[cfg(windows)] {
            for _ in 0..30 {
                match path.metadata() {
                    Ok(_) => return Ok(()),
                    Err(err) => if let Some(code) = err.raw_os_error() {
                        // Windows can't query socket file existence
                        // Check against arbitrary error code
                        if code == 1920 {
                            return Ok(())
                        }
                    },
                }

                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        } else {
            for _ in 0..30 {
                // Wait for socket to exist
                if path.exists() {
                    return Ok(());
                }

                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }

    Err(eyre!("failed to connect to socket".to_owned()))
}

#[cfg(target_os = "linux")]
fn launch_linux_desktop(ctx: std::sync::Arc<fig_os_shim::Context>, state: &fig_settings::State) -> eyre::Result<()> {
    use std::process::Stdio;

    use eyre::Context;
    use fig_integrations::desktop_entry::{
        EntryContents,
        local_entry_path,
    };
    use fig_util::APP_PROCESS_NAME;
    use tracing::error;

    if state.get_bool_or("appimage.manageDesktopEntry", false) {
        if let Some(exec) = EntryContents::from_path_sync(&ctx, local_entry_path(&ctx)?)?.get_field("Exec") {
            match Command::new(exec)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(_) => return Ok(()),
                Err(err) => {
                    error!(
                        ?err,
                        "Unable to launch desktop app according to the local desktop entry."
                    );
                },
            }
        }
        // Fall back to calling q-desktop if on the user's path
    }

    Command::new(APP_PROCESS_NAME)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context(format!("Executable '{}' not in the user's path", APP_PROCESS_NAME))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "not in ci"]
    fn test_e2e_desktop_app_running() {
        println!("{}", desktop_app_running());
    }

    #[test]
    #[ignore = "not in ci"]
    fn test_e2e_launch_fig_desktop() {
        launch_fig_desktop(LaunchArgs {
            wait_for_socket: true,
            open_dashboard: true,
            immediate_update: false,
            verbose: true,
        })
        .unwrap();
    }

    #[test]
    #[ignore = "not in ci"]
    #[cfg(target_os = "linux")]
    fn test_e2e_launch_linux_desktop() {
        use fig_os_shim::Context;
        use fig_settings::State;

        launch_linux_desktop(Context::new(), &State::new()).unwrap();
    }
}
