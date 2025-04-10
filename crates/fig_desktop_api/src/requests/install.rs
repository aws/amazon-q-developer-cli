use std::fmt::Display;

use anstream::adapter::strip_str;
use fig_integrations::Integration;
use fig_integrations::desktop_entry::{
    AutostartIntegration,
    DesktopEntryIntegration,
};
use fig_integrations::shell::ShellExt;
use fig_integrations::ssh::SshIntegration;
use fig_os_shim::{
    ContextArcProvider,
    ContextProvider,
    EnvProvider,
};
use fig_proto::fig::install_response::{
    InstallationStatus,
    Response,
};
use fig_proto::fig::result::Result as ProtoResultEnum;
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    InstallAction,
    InstallComponent,
    InstallRequest,
    InstallResponse,
    Result as ProtoResult,
};
use fig_settings::settings::SettingsProvider;
use fig_settings::state::StateProvider;
use fig_util::Shell;
use tracing::error;

use super::RequestResult;

#[allow(dead_code)]
async fn integration_status(integration: impl fig_integrations::Integration) -> ServerOriginatedSubMessage {
    ServerOriginatedSubMessage::InstallResponse(InstallResponse {
        response: Some(Response::InstallationStatus(match integration.is_installed().await {
            Ok(_) => InstallationStatus::Installed.into(),
            Err(_) => InstallationStatus::NotInstalled.into(),
        })),
    })
}

#[allow(dead_code)]
fn integration_unsupported() -> ServerOriginatedSubMessage {
    ServerOriginatedSubMessage::InstallResponse(InstallResponse {
        response: Some(Response::InstallationStatus(InstallationStatus::NotSupported.into())),
    })
}

fn integration_result(result: Result<(), impl Display>) -> ServerOriginatedSubMessage {
    ServerOriginatedSubMessage::InstallResponse(InstallResponse {
        response: Some(Response::Result(match result {
            Ok(()) => ProtoResult {
                result: ProtoResultEnum::Ok.into(),
                error: None,
            },
            Err(err) => ProtoResult {
                result: ProtoResultEnum::Error.into(),
                error: Some(err.to_string()),
            },
        })),
    })
}

pub async fn install<Ctx>(request: InstallRequest, ctx: &Ctx) -> RequestResult
where
    Ctx: SettingsProvider + StateProvider + ContextProvider + ContextArcProvider + Send + Sync,
{
    let response = match (request.component(), request.action()) {
        (InstallComponent::Dotfiles, action) => {
            let mut errs: Vec<String> = vec![];
            for shell in Shell::all() {
                match shell.get_shell_integrations(ctx.env()) {
                    Ok(integrations) => {
                        for integration in integrations {
                            let res = match action {
                                InstallAction::Install => integration.install().await,
                                InstallAction::Uninstall => integration.uninstall().await,
                                InstallAction::Status => integration.is_installed().await,
                            };

                            if let Err(err) = res {
                                errs.push(format!(
                                    "{integration}: {}",
                                    strip_str(&err.verbose_message().to_string())
                                ));
                            }
                        }
                    },
                    Err(err) => {
                        errs.push(format!("{shell}: {}", strip_str(&err.verbose_message().to_string())));
                    },
                }
            }

            match action {
                InstallAction::Install | InstallAction::Uninstall => integration_result(match &errs[..] {
                    [] => Ok(()),
                    errs => Err(errs.join("\n\n")),
                }),
                InstallAction::Status => ServerOriginatedSubMessage::InstallResponse(InstallResponse {
                    response: Some(Response::InstallationStatus(
                        if errs.is_empty() {
                            InstallationStatus::Installed
                        } else {
                            InstallationStatus::NotInstalled
                        }
                        .into(),
                    )),
                }),
            }
        },
        (InstallComponent::Ssh, action) => match SshIntegration::new() {
            Ok(ssh_integration) => match action {
                InstallAction::Install => integration_result(ssh_integration.install().await),
                InstallAction::Uninstall => integration_result(ssh_integration.uninstall().await),
                InstallAction::Status => integration_status(ssh_integration).await,
            },
            Err(err) => integration_result(Err(err)),
        },
        (InstallComponent::Ibus, _) => integration_result(Err("IBus install is legacy")),
        (InstallComponent::Accessibility, InstallAction::Install) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use macos_utils::accessibility::{
                        open_accessibility,
                        accessibility_is_enabled
                    };

                    if !accessibility_is_enabled() {
                        open_accessibility();
                    }

                    integration_result(Ok::<(), &str>(()))
                } else {
                    integration_result(Err("Accessibility permissions cannot be queried"))
                }
            }
        },
        (InstallComponent::Accessibility, InstallAction::Status) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use macos_utils::accessibility::accessibility_is_enabled;

                    ServerOriginatedSubMessage::InstallResponse(InstallResponse {
                        response: Some(Response::InstallationStatus(if accessibility_is_enabled() {
                            InstallationStatus::Installed.into()
                        } else {
                            InstallationStatus::NotInstalled.into()
                        })),
                    })
                } else {
                    integration_result(Err("Accessibility permissions cannot be queried"))
                }
            }
        },
        (InstallComponent::Accessibility, InstallAction::Uninstall) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    integration_result(Ok::<(), &str>(()))
                } else {
                    integration_result(Err("Accessibility permissions cannot be queried"))
                }
            }
        },
        (InstallComponent::InputMethod, InstallAction::Install) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use fig_integrations::input_method::{
                        InputMethod,
                    };
                    use fig_integrations::Integration;

                    integration_result(match InputMethod::default().install().await {
                        Ok(_) => Ok(()),
                        Err(err) => Err(format!("Could not install input method: {err}")),
                    })
                } else {
                    integration_result(Err("Input method install is only supported on macOS"))
                }
            }
        },
        (InstallComponent::InputMethod, InstallAction::Uninstall) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use fig_integrations::input_method::{
                        InputMethod,
                        InputMethodError,
                    };
                    use fig_integrations::Error;
                    use fig_integrations::Integration;

                    integration_result(match InputMethod::default().uninstall().await {
                        Ok(_) | Err(Error::InputMethod(InputMethodError::CouldNotListInputSources)) => {
                            Ok(())
                        },
                        Err(err) => Err(format!("Could not uninstall input method: {err}")),
                    })
                } else {
                    integration_result(Err("Input method uninstall is only supported on macOS"))
                }
            }
        },
        (InstallComponent::InputMethod, InstallAction::Status) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use fig_integrations::input_method::{
                        InputMethod,
                    };

                    integration_status(InputMethod::default()).await
                } else {
                    integration_unsupported()
                }
            }
        },
        (InstallComponent::DesktopEntry, action) => {
            if !ctx.env().in_appimage() {
                integration_result(Err(
                    "Desktop entry installation is only supported for AppImage bundles.",
                ))
            } else {
                let exec_path = ctx.env().get("APPIMAGE").map_err(super::Error::from_std)?;
                let entry_path = ctx
                    .env()
                    .current_dir()
                    .map_err(super::Error::from_std)?
                    .join("share/applications/q-desktop.desktop");
                let icon_path = ctx
                    .env()
                    .current_dir()
                    .map_err(super::Error::from_std)?
                    .join("share/icons/hicolor/128x128/apps/q-desktop.png");
                let desktop_integration =
                    DesktopEntryIntegration::new(ctx, Some(entry_path), Some(icon_path), Some(exec_path.into()));
                match action {
                    InstallAction::Install => {
                        ctx.state()
                            .set_value("appimage.manageDesktopEntry", true)
                            .map_err(|err| error!(?err, "unable to set `appimage.manageDesktopEntry`"))
                            .ok();
                        integration_result(desktop_integration.install().await)
                    },
                    InstallAction::Uninstall => {
                        ctx.state()
                            .set_value("appimage.manageDesktopEntry", false)
                            .map_err(|err| error!(?err, "unable to set `appimage.manageDesktopEntry`"))
                            .ok();
                        integration_result(desktop_integration.uninstall().await)
                    },
                    InstallAction::Status => integration_status(desktop_integration).await,
                }
            }
        },
        (InstallComponent::AutostartEntry, action) => {
            let ctx = ctx.context();
            let integration = AutostartIntegration::new(&ctx).map_err(super::Error::from_std)?;
            match action {
                InstallAction::Install => integration_result(integration.install().await),
                InstallAction::Uninstall => integration_result(integration.uninstall().await),
                InstallAction::Status => integration_status(integration).await,
            }
        },
        #[allow(unused_variables)]
        (InstallComponent::GnomeExtension, action) => {
            cfg_if::cfg_if! {
                if #[cfg(target_os = "linux")] {
                    use std::sync::Arc;
                    let shell_extensions = dbus::gnome_shell::ShellExtensions::new(Arc::downgrade(&ctx.context_arc()));
                    install_gnome_extension(action, ctx, &shell_extensions).await.map_err(super::Error::Std)?
                }
                else {
                    integration_result(Err("Not supported on platforms other than Linux."))
                }
            }
        },
    };

    RequestResult::Ok(Box::new(response))
}

#[cfg(target_os = "linux")]
async fn install_gnome_extension<'a, Ctx, ExtensionsCtx>(
    action: InstallAction,
    ctx: &'a Ctx,
    shell_extensions: &'a dbus::gnome_shell::ShellExtensions<ExtensionsCtx>,
) -> Result<super::ServerOriginatedSubMessage, Box<dyn std::error::Error + Send + Sync>>
where
    Ctx: SettingsProvider + StateProvider + ContextProvider + Sync,
    ExtensionsCtx: ContextProvider + Send + Sync,
{
    use fig_integrations::gnome_extension::GnomeExtensionIntegration;
    use fig_util::directories::{
        bundled_gnome_extension_version_path,
        bundled_gnome_extension_zip_path,
    };

    let extension_uuid = shell_extensions.extension_uuid().await?;
    let bundled_version: u32 = ctx
        .context()
        .fs()
        .read_to_string(bundled_gnome_extension_version_path(ctx, &extension_uuid)?)
        .await?
        .parse()?;
    let bundle_path = bundled_gnome_extension_zip_path(ctx, &extension_uuid)?;
    let gnome_integration =
        GnomeExtensionIntegration::new(ctx, shell_extensions, Some(bundle_path), Some(bundled_version));
    ctx.state()
        .set_value("desktop.gnomeExtensionInstallationPermissionGranted", true)
        .map_err(|err| {
            error!(
                ?err,
                "unable to set `desktop.gnomeExtensionInstallationPermissionGranted`"
            );
        })
        .ok();
    match action {
        InstallAction::Install => Ok(integration_result(gnome_integration.install().await)),
        InstallAction::Uninstall => Ok(integration_result(gnome_integration.uninstall().await)),
        InstallAction::Status => Ok(integration_status(gnome_integration).await),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fig_integrations::desktop_entry::global_entry_path;
    use fig_os_shim::{
        Context,
        ContextProvider,
    };
    use fig_proto::fig::server_originated_message::Submessage;
    use fig_settings::{
        Settings,
        State,
    };
    use fig_util::directories::{
        appimage_desktop_entry_icon_path,
        appimage_desktop_entry_path,
    };

    use super::*;

    #[derive(Debug, Clone)]
    struct TestContext {
        ctx: Arc<Context>,
        settings: Settings,
        state: State,
    }

    impl SettingsProvider for TestContext {
        fn settings(&self) -> &Settings {
            &self.settings
        }
    }

    impl StateProvider for TestContext {
        fn state(&self) -> &fig_settings::State {
            &self.state
        }
    }

    impl ContextProvider for TestContext {
        fn context(&self) -> &Context {
            &self.ctx
        }
    }

    impl ContextArcProvider for TestContext {
        fn context_arc(&self) -> Arc<Context> {
            Arc::clone(&self.ctx)
        }
    }

    async fn assert_status(ctx: &TestContext, component: InstallComponent, expected_status: InstallationStatus) {
        let request = InstallRequest {
            component: component.into(),
            action: InstallAction::Status.into(),
        };
        let response = install(request, ctx).await.unwrap();
        assert_submessage_status(*response, expected_status, "");
    }

    fn assert_submessage_status(submessage: Submessage, expected_status: InstallationStatus, message: &str) {
        if let Submessage::InstallResponse(InstallResponse {
            response: Some(Response::InstallationStatus(actual_status)),
        }) = submessage
        {
            let expected_status: i32 = expected_status.into();
            assert_eq!(actual_status, expected_status, "{}", message);
        } else {
            panic!("unexpected response: {:?}", submessage);
        }
    }

    #[tokio::test]
    async fn test_desktop_entry_installation_and_uninstallation() {
        let ctx = Context::builder()
            .with_test_home()
            .await
            .unwrap()
            .with_env_var("APPIMAGE", "/test.appimage")
            .build();
        let fs = ctx.fs();
        let entry_path = appimage_desktop_entry_path(&ctx).unwrap();
        let icon_path = appimage_desktop_entry_icon_path(&ctx).unwrap();
        fs.create_dir_all(entry_path.parent().unwrap()).await.unwrap();
        fs.write(&entry_path, "[Desktop Entry]\nExec=q-desktop").await.unwrap();
        fs.create_dir_all(icon_path.parent().unwrap()).await.unwrap();
        fs.write(&icon_path, "image").await.unwrap();
        let ctx = TestContext {
            ctx,
            settings: Settings::new_fake(),
            state: State::new_fake(),
        };

        // Test installation
        assert_status(&ctx, InstallComponent::DesktopEntry, InstallationStatus::NotInstalled).await;
        let request = InstallRequest {
            component: InstallComponent::DesktopEntry.into(),
            action: InstallAction::Install.into(),
        };
        install(request, &ctx).await.unwrap();
        assert_eq!(ctx.state.get_bool("appimage.manageDesktopEntry").unwrap(), Some(true));
        assert_status(&ctx, InstallComponent::DesktopEntry, InstallationStatus::Installed).await;

        // Test uninstallation
        let request = InstallRequest {
            component: InstallComponent::DesktopEntry.into(),
            action: InstallAction::Uninstall.into(),
        };
        install(request, &ctx).await.unwrap();
        assert_eq!(ctx.state.get_bool("appimage.manageDesktopEntry").unwrap(), Some(false));
        assert_status(&ctx, InstallComponent::DesktopEntry, InstallationStatus::NotInstalled).await;
    }

    #[tokio::test]
    async fn test_autostart_entry_installation_and_uninstallation() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        // Create global desktop entry
        {
            let global_path = global_entry_path(&ctx);
            ctx.fs().create_dir_all(global_path.parent().unwrap()).await.unwrap();
            ctx.fs().write(global_path, "[Desktop Entry]").await.unwrap();
        }
        let ctx = TestContext {
            ctx,
            settings: Settings::new_fake(),
            state: State::new_fake(),
        };

        // Test installation
        assert_status(&ctx, InstallComponent::AutostartEntry, InstallationStatus::NotInstalled).await;
        let request = InstallRequest {
            component: InstallComponent::AutostartEntry.into(),
            action: InstallAction::Install.into(),
        };
        install(request, &ctx).await.unwrap();
        assert_status(&ctx, InstallComponent::AutostartEntry, InstallationStatus::Installed).await;
        assert!(
            AutostartIntegration::to_global(&ctx).is_installed().await.is_ok(),
            "Autostart entry should have been installed."
        );

        // Test uninstallation
        let request = InstallRequest {
            component: InstallComponent::AutostartEntry.into(),
            action: InstallAction::Uninstall.into(),
        };
        install(request, &ctx).await.unwrap();
        assert_status(&ctx, InstallComponent::AutostartEntry, InstallationStatus::NotInstalled).await;
        assert!(
            AutostartIntegration::to_global(&ctx).is_installed().await.is_err(),
            "Autostart entry should have been uninstalled."
        );
    }

    /// Helper function that writes test files to [bundled_extension_zip_path] and
    /// [bundled_extension_version_path].
    #[cfg(target_os = "linux")]
    async fn write_extension_bundle(ctx: &Context, uuid: &str, version: u32) {
        use fig_util::directories::{
            bundled_gnome_extension_version_path,
            bundled_gnome_extension_zip_path,
        };

        let zip_path = bundled_gnome_extension_zip_path(ctx, uuid).unwrap();
        let version_path = bundled_gnome_extension_version_path(ctx, uuid).unwrap();
        ctx.fs().create_dir_all(zip_path.parent().unwrap()).await.unwrap();
        ctx.fs().write(&zip_path, version.to_string()).await.unwrap();
        ctx.fs().write(&version_path, version.to_string()).await.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_gnome_extension_installation_and_uninstallation() {
        use dbus::gnome_shell::{
            GNOME_SHELL_PROCESS_NAME,
            ShellExtensions,
        };
        use fig_os_shim::Os;

        let ctx = Context::builder()
            .with_test_home()
            .await
            .unwrap()
            .with_env_var("APPIMAGE", "/test.appimage")
            .with_os(Os::Linux)
            .with_running_processes(&[GNOME_SHELL_PROCESS_NAME])
            .build_fake();
        let settings = Settings::new_fake();
        let test_ctx = TestContext {
            ctx: Arc::clone(&ctx),
            settings,
            state: State::new_fake(),
        };
        let shell_extensions = ShellExtensions::new_fake(Arc::downgrade(&ctx));
        write_extension_bundle(&ctx, &shell_extensions.extension_uuid().await.unwrap(), 1).await;

        // Not installed by default
        let response = install_gnome_extension(InstallAction::Status, &test_ctx, &shell_extensions)
            .await
            .unwrap();
        assert_submessage_status(
            response,
            InstallationStatus::NotInstalled,
            "Should not be installed by default",
        );

        // Test installation
        install_gnome_extension(InstallAction::Install, &test_ctx, &shell_extensions)
            .await
            .unwrap();
        let response = install_gnome_extension(InstallAction::Status, &test_ctx, &shell_extensions)
            .await
            .unwrap();
        assert_submessage_status(
            response,
            InstallationStatus::Installed,
            "Should be installed after install action",
        );
        assert_eq!(
            test_ctx
                .state
                .get_bool("desktop.gnomeExtensionInstallationPermissionGranted")
                .unwrap(),
            Some(true)
        );

        // Test uninstallation
        install_gnome_extension(InstallAction::Uninstall, &test_ctx, &shell_extensions)
            .await
            .unwrap();
        let response = install_gnome_extension(InstallAction::Status, &test_ctx, &shell_extensions)
            .await
            .unwrap();
        assert_submessage_status(
            response,
            InstallationStatus::NotInstalled,
            "Should be uninstalled after uninstall action",
        );
        assert_eq!(
            test_ctx
                .state
                .get_bool("desktop.gnomeExtensionInstallationPermissionGranted")
                .unwrap(),
            Some(true),
            "Should not be unset on uninstall"
        );
    }
}
