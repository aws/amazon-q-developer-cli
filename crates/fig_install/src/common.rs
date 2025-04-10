use std::path::PathBuf;
use std::sync::Arc;

use fig_integrations::Integration;
use fig_integrations::shell::ShellExt;
use fig_integrations::ssh::SshIntegration;
use fig_os_shim::{
    Context,
    Env,
};
use fig_util::{
    CLI_BINARY_NAME,
    OLD_CLI_BINARY_NAMES,
    OLD_PTY_BINARY_NAMES,
    PTY_BINARY_NAME,
    Shell,
    directories,
};

use crate::Error;

bitflags::bitflags! {
    /// The different components that can be installed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct InstallComponents: u64 {
        /// Removal of the integrations from user's dotfiles
        const SHELL_INTEGRATIONS    = 0b00000001;
        /// This handles the removal of the CLI and pty binaries as well as legacy copies
        const BINARY                = 0b00000010;
        /// Removal of the ssh integration from the ~/.ssh/config file
        const SSH                   = 0b00000100;
        const DESKTOP_APP           = 0b00001000;
        const INPUT_METHOD          = 0b00010000;
        const DESKTOP_ENTRY         = 0b00100000;
        const GNOME_SHELL_EXTENSION = 0b01000000;
    }
}

#[cfg(target_os = "linux")]
impl InstallComponents {
    pub fn all_linux_minimal() -> Self {
        Self::SHELL_INTEGRATIONS | Self::BINARY | Self::SSH
    }
}

pub async fn uninstall(components: InstallComponents, ctx: Arc<Context>) -> Result<(), Error> {
    let ssh_result = if components.contains(InstallComponents::SSH) {
        SshIntegration::new()?.uninstall().await
    } else {
        Ok(())
    };

    let shell_integration_result = {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            for integration in shell.get_shell_integrations(ctx.env())? {
                integration.uninstall().await?;
            }
        }
        Ok(())
    };

    if components.contains(InstallComponents::BINARY) {
        let remove_binary = |path: PathBuf| async move {
            match tokio::fs::remove_file(&path).await {
                Ok(_) => tracing::info!("Removed binary: {path:?}"),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {},
                Err(err) => tracing::warn!(%err, "Failed to remove binary: {path:?}"),
            }
        };

        // let folders = [directories::home_local_bin()?, Path::new("/usr/local/bin").into()];
        let folders = [directories::home_local_bin()?];

        let mut all_binary_names = vec![CLI_BINARY_NAME, PTY_BINARY_NAME];
        all_binary_names.extend(OLD_CLI_BINARY_NAMES);
        all_binary_names.extend(OLD_PTY_BINARY_NAMES);

        let mut pty_names = vec![PTY_BINARY_NAME];
        pty_names.extend(OLD_PTY_BINARY_NAMES);

        for folder in folders {
            for binary_name in &all_binary_names {
                let binary_path = folder.join(binary_name);
                remove_binary(binary_path).await;
            }

            for shell in Shell::all() {
                for pty_name in &pty_names {
                    let pty_path = folder.join(format!("{shell} ({pty_name})"));
                    remove_binary(pty_path).await;
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    if components.contains(InstallComponents::GNOME_SHELL_EXTENSION) {
        let shell_extensions = dbus::gnome_shell::ShellExtensions::new(Arc::downgrade(&ctx));
        super::os::uninstall_gnome_extension(&ctx, &shell_extensions).await?;
    }

    #[cfg(target_os = "linux")]
    if components.contains(InstallComponents::DESKTOP_ENTRY) {
        super::os::uninstall_desktop_entries(&ctx).await?;
    }

    let daemon_result = Ok(());

    #[cfg(target_os = "macos")]
    if components.contains(InstallComponents::INPUT_METHOD) {
        use fig_integrations::Error;
        use fig_integrations::input_method::{
            InputMethod,
            InputMethodError,
        };

        match InputMethod::default().uninstall().await {
            Ok(_) | Err(Error::InputMethod(InputMethodError::CouldNotListInputSources)) => {},
            Err(err) => return Err(err.into()),
        }
    }

    if components.contains(InstallComponents::DESKTOP_APP) {
        super::os::uninstall_desktop(&ctx).await?;
        // Must be last -- this will kill the running desktop process if this is
        // called from the desktop app.
        let quit_res = tokio::process::Command::new("killall")
            .args([fig_util::consts::APP_PROCESS_NAME])
            .output()
            .await;
        if let Err(err) = quit_res {
            tracing::warn!("Failed to quit running Fig app: {err}");
        }
    }

    daemon_result
        .and(shell_integration_result)
        .and(ssh_result.map_err(|e| e.into()))
}

pub async fn install(components: InstallComponents, env: &Env) -> Result<(), Error> {
    if components.contains(InstallComponents::SHELL_INTEGRATIONS) {
        let mut errs: Vec<Error> = vec![];
        for shell in Shell::all() {
            match shell.get_shell_integrations(env) {
                Ok(integrations) => {
                    for integration in integrations {
                        if let Err(e) = integration.install().await {
                            errs.push(e.into());
                        }
                    }
                },
                Err(e) => {
                    errs.push(e.into());
                },
            }
        }

        if let Some(err) = errs.pop() {
            return Err(err);
        }
    }

    if components.contains(InstallComponents::SSH) {
        SshIntegration::new()?.install().await?;
    }

    #[cfg(target_os = "macos")]
    if components.contains(InstallComponents::INPUT_METHOD) {
        use fig_integrations::input_method::InputMethod;
        InputMethod::default().install().await?;
    }

    Ok(())
}
