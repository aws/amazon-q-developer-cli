use fig_integrations::shell::ShellExt;
use fig_os_shim::{
    ContextArcProvider,
    ContextProvider,
    EnvProvider,
};
use fig_proto::fig::{
    OnboardingAction,
    OnboardingRequest,
};
use fig_settings::{
    SettingsProvider,
    StateProvider,
};
use fig_util::Shell;
use tao::event_loop::ControlFlow;
#[cfg(target_os = "macos")]
use tokio::process::Command;
#[allow(unused_imports)]
use tracing::error;

use super::{
    RequestResult,
    RequestResultImpl,
};
use crate::EventLoopProxy;
use crate::event::Event;

pub async fn post_login() {
    fig_settings::state::set_value("desktop.completedOnboarding", true).ok();
}

pub async fn onboarding<Ctx>(request: OnboardingRequest, proxy: &EventLoopProxy, ctx: &Ctx) -> RequestResult
where
    Ctx: SettingsProvider + StateProvider + ContextProvider + ContextArcProvider + Send + Sync,
{
    match request.action() {
        OnboardingAction::InstallationScript => {
            let mut errs: Vec<String> = vec![];
            for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
                match shell.get_shell_integrations(ctx.env()) {
                    Ok(integrations) => {
                        for integration in integrations {
                            if let Err(err) = integration.install().await {
                                errs.push(format!("{integration}: {err}"));
                            }
                        }
                    },
                    Err(err) => {
                        errs.push(format!("{shell}: {err}"));
                    },
                }
            }

            match &errs[..] {
                [] => RequestResult::success(),
                errs => RequestResult::error(errs.join("\n")),
            }
        },
        OnboardingAction::Uninstall => {
            use fig_install::{
                InstallComponents,
                uninstall,
            };

            fig_util::open_url(fig_install::UNINSTALL_URL).ok();

            let result = match uninstall(InstallComponents::all(), ctx.context_arc()).await {
                Ok(_) => RequestResult::success(),
                Err(err) => RequestResult::error(err.to_string()),
            };

            proxy.send_event(Event::ControlFlow(ControlFlow::Exit)).ok();
            result
        },
        OnboardingAction::FinishOnboarding => {
            post_login().await;
            RequestResult::success()
        },
        OnboardingAction::LaunchShellOnboarding => {
            fig_settings::state::set_value("user.onboarding", false).ok();

            cfg_if::cfg_if! {
                if #[cfg(target_os = "linux")] {
                    use fig_util::terminal::LINUX_TERMINALS;

                    for terminal_executable in LINUX_TERMINALS.iter().flat_map(|term| term.executable_names()) {
                        if let Ok(terminal_executable_path) = which::which(terminal_executable) {
                            tokio::spawn(tokio::process::Command::new(terminal_executable_path).output());
                            return RequestResult::success();
                        }
                    }
                    RequestResult::error("Failed to open any terminal")
                } else if #[cfg(target_os = "macos")] {
                    let home = fig_util::directories::home_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));

                    if let Err(err) = Command::new("open").args(["-b", "com.apple.Terminal"]).arg(home).spawn() {
                        error!(%err, "Failed to open onboarding");
                        return RequestResult::error("Failed to open onboarding");
                    }

                    RequestResult::success()
                } else if #[cfg(target_os = "windows")] {
                    use std::os::windows::process::CommandExt;

                    let create_new_console = 0x10;
                    match std::process::Command::new("cmd").creation_flags(create_new_console).arg("/c").raw_arg(r#"""%PROGRAMFILES%/Git/bin/bash.exe"""#).spawn() {
                        Ok(_) => RequestResult::success(),
                        Err(e) => RequestResult::error(format!("Failed to start Git Bash: {e}")),
                    }
                }
            }
        },
        OnboardingAction::PromptForAccessibilityPermission => {
            use crate::local_ipc::{
                LocalResponse,
                commands,
            };
            let res = commands::prompt_for_accessibility_permission(ctx)
                .await
                .unwrap_or_else(|e| e);
            match res {
                LocalResponse::Success(_) => RequestResult::success(),
                LocalResponse::Error {
                    message: Some(message), ..
                } => RequestResult::error(message),
                _ => RequestResult::error("Failed to prompt for accessibility permissions"),
            }
        },
        OnboardingAction::PostLogin => {
            post_login().await;
            RequestResult::success()
        },
        OnboardingAction::CloseAccessibilityPromptWindow
        | OnboardingAction::RequestRestart
        | OnboardingAction::CloseInputMethodPromptWindow => RequestResult::error("Unimplemented"),
    }
}
