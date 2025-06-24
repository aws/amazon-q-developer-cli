use std::sync::Mutex;

use fig_os_shim::{
    Context,
    ContextArcProvider,
    ContextProvider,
};
use fig_proto::local::command_response::Response as CommandResponseTypes;
use fig_proto::local::dump_state_command::Type as DumpStateType;
use fig_proto::local::{
    BundleMetadataResponse,
    DebugModeCommand,
    DiagnosticsCommand,
    DiagnosticsResponse,
    DumpStateCommand,
    DumpStateResponse,
    LogLevelCommand,
    LogLevelResponse,
    OpenBrowserCommand,
    OpenUiElementCommand,
    QuitCommand,
    UiElement,
};
use fig_remote_ipc::figterm::FigtermState;
use fig_settings::StateProvider;
use fig_settings::settings::SettingsProvider;
use tao::event_loop::ControlFlow;
use tracing::error;

use super::{
    LocalResponse,
    LocalResult,
};
use crate::event::{
    Event,
    WindowEvent,
};
use crate::platform::PlatformState;
use crate::webview::DASHBOARD_SIZE;
use crate::webview::notification::WebviewNotificationsState;
use crate::{
    AUTOCOMPLETE_ID,
    DASHBOARD_ID,
    EventLoopProxy,
    platform,
};

pub async fn debug(command: DebugModeCommand, proxy: &EventLoopProxy) -> LocalResult {
    static DEBUG_MODE: Mutex<bool> = Mutex::new(false);

    let debug_mode = match command.set_debug_mode {
        Some(b) => {
            *DEBUG_MODE.lock().unwrap() = b;
            b
        },
        None => match command.toggle_debug_mode {
            Some(true) => {
                let mut locked_debug = DEBUG_MODE.lock().unwrap();
                *locked_debug = !*locked_debug;
                *locked_debug
            },
            _ => *DEBUG_MODE.lock().unwrap(),
        },
    };

    proxy
        .send_event(Event::WindowEvent {
            window_id: AUTOCOMPLETE_ID.clone(),
            window_event: WindowEvent::DebugMode(debug_mode),
        })
        .unwrap();

    Ok(LocalResponse::Success(None))
}

pub async fn quit(_: QuitCommand, proxy: &EventLoopProxy) -> LocalResult {
    proxy
        .send_event(Event::ControlFlow(ControlFlow::Exit))
        .map(|_| LocalResponse::Success(None))
        .map_err(|_err| {
            #[allow(clippy::exit)]
            std::process::exit(0)
        })
}

pub async fn diagnostic(_: DiagnosticsCommand, figterm_state: &FigtermState) -> LocalResult {
    let (edit_buffer_string, edit_buffer_cursor, shell_context, intercept_enabled, intercept_global_enabled) = {
        match figterm_state.most_recent() {
            Some(session) => (
                Some(session.edit_buffer.text.clone()),
                Some(session.edit_buffer.cursor),
                session.context.clone(),
                Some(session.intercept.into()),
                Some(session.intercept_global.into()),
            ),
            None => (None, None, None, None, None),
        }
    };

    let response = DiagnosticsResponse {
        autocomplete_active: Some(platform::autocomplete_active()),
        #[cfg(target_os = "macos")]
        path_to_bundle: macos_utils::bundle::get_bundle_path()
            .and_then(|path| path.to_str().map(|s| s.to_owned()))
            .unwrap_or_default(),
        #[cfg(target_os = "macos")]
        accessibility: if macos_utils::accessibility::accessibility_is_enabled() {
            "true".into()
        } else {
            "false".into()
        },

        edit_buffer_string,
        edit_buffer_cursor,
        shell_context,
        intercept_enabled,
        intercept_global_enabled,

        ..Default::default()
    };

    Ok(LocalResponse::Message(Box::new(CommandResponseTypes::Diagnostics(
        response,
    ))))
}

pub async fn open_ui_element(command: OpenUiElementCommand, proxy: &EventLoopProxy) -> LocalResult {
    match command.element() {
        UiElement::Settings => {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID.clone(),
                    window_event: WindowEvent::Batch(vec![
                        WindowEvent::NavigateRelative {
                            path: "/preferences".into(),
                        },
                        WindowEvent::Show,
                    ]),
                })
                .unwrap();
        },
        UiElement::MissionControl => {
            let events = if let Some(path) = command.route {
                vec![WindowEvent::NavigateRelative { path: path.into() }, WindowEvent::Show]
            } else {
                vec![WindowEvent::Show]
            };

            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID.clone(),
                    window_event: WindowEvent::Batch(events),
                })
                .unwrap();
        },
        UiElement::MenuBar => error!("Opening menu bar is unimplemented"),
        UiElement::InputMethodPrompt => error!("Opening input method prompt is unimplemented"),
    };

    Ok(LocalResponse::Success(None))
}

pub async fn open_browser(command: OpenBrowserCommand) -> LocalResult {
    if let Err(err) = fig_util::open_url(command.url) {
        error!(%err, "Error opening browser");
    }
    Ok(LocalResponse::Success(None))
}

#[allow(unused_variables)]
pub async fn prompt_for_accessibility_permission<Ctx>(ctx: &Ctx) -> LocalResult
where
    Ctx: SettingsProvider + StateProvider + ContextProvider + ContextArcProvider + Send + Sync,
{
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            use fig_desktop_api::requests::install::install;
            use fig_proto::fig::{InstallRequest, InstallComponent, InstallAction};

            install(
                InstallRequest {
                    component: InstallComponent::Accessibility.into(),
                    action: InstallAction::Install.into()
                },
                ctx
            ).await.ok();
            Ok(LocalResponse::Success(None))
        } else {
            Err(LocalResponse::Error {
                code: None,
                message: Some("Accessibility API not supported on this platform".to_owned()),
            })
        }
    }
}

pub fn log_level(LogLevelCommand { level }: LogLevelCommand) -> LocalResult {
    let old_level = fig_log::set_log_level(level).map_err(|err| LocalResponse::Error {
        code: None,
        message: Some(format!("Error setting log level: {err}")),
    })?;

    Ok(LocalResponse::Message(Box::new(CommandResponseTypes::LogLevel(
        LogLevelResponse {
            old_level: Some(old_level),
        },
    ))))
}

pub async fn login(proxy: &EventLoopProxy) -> LocalResult {
    proxy
        .send_event(Event::WindowEvent {
            window_id: DASHBOARD_ID,
            window_event: WindowEvent::Batch(vec![
                WindowEvent::UpdateWindowGeometry {
                    size: Some(DASHBOARD_SIZE),
                    position: None,
                    anchor: None,
                    tx: None,
                    dry_run: false,
                },
                WindowEvent::Reload,
                WindowEvent::Show,
            ]),
        })
        .map_err(|err| error!(?err))
        .ok();

    proxy
        .send_event(Event::ReloadTray { is_logged_in: true })
        .map_err(|err| error!(?err))
        .ok();

    Ok(LocalResponse::Success(None))
}

pub async fn logout(proxy: &EventLoopProxy) -> LocalResult {
    fig_auth::logout().await.ok();

    proxy
        .send_event(Event::WindowEvent {
            window_id: DASHBOARD_ID,
            window_event: WindowEvent::Batch(vec![WindowEvent::Reload, WindowEvent::Show]),
        })
        .map_err(|err| error!(?err))
        .ok();

    proxy
        .send_event(Event::ReloadTray { is_logged_in: false })
        .map_err(|err| error!(?err))
        .ok();

    Ok(LocalResponse::Success(None))
}

pub fn dump_state(
    command: DumpStateCommand,
    figterm_state: &FigtermState,
    webview_notifications_state: &WebviewNotificationsState,
    platform_state: &PlatformState,
) -> LocalResult {
    let json = match command.r#type() {
        DumpStateType::DumpStateFigterm => {
            serde_json::to_string_pretty(&figterm_state).unwrap_or_else(|err| format!("unable to dump: {err}"))
        },
        DumpStateType::DumpStateWebNotifications => serde_json::to_string_pretty(&webview_notifications_state)
            .unwrap_or_else(|err| format!("unable to dump: {err}")),
        DumpStateType::DumpStatePlatform => {
            serde_json::to_string_pretty(&platform_state).unwrap_or_else(|err| format!("unable to dump: {err}"))
        },
    };

    LocalResult::Ok(LocalResponse::Message(Box::new(CommandResponseTypes::DumpState(
        DumpStateResponse { json },
    ))))
}

#[allow(unused_variables)]
pub async fn connect_to_ibus(proxy: EventLoopProxy, platform_state: &PlatformState) -> LocalResult {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "linux")] {
            use crate::platform::ibus::launch_ibus_connection;
            match launch_ibus_connection(proxy, platform_state.inner()).await {
                Ok(_) => Ok(LocalResponse::Success(None)),
                Err(err) => {
                    Err(LocalResponse::Error {
                        code: None,
                        message: Some(format!("Failed connecting to ibus: {:?}", err)),
                    })
                },
            }
        } else {
            Err(LocalResponse::Error {
                code: None,
                message: Some("Connecting to IBus is only supported on Linux".to_owned()),
            })
        }
    }
}

pub async fn bundle_metadata(ctx: &Context) -> LocalResult {
    match fig_util::manifest::bundle_metadata_json(ctx).await {
        Ok(json) => Ok(LocalResponse::Message(Box::new(CommandResponseTypes::BundleMetadata(
            BundleMetadataResponse { json },
        )))),
        Err(err) => Err(LocalResponse::Error {
            code: None,
            message: Some(format!("Failed to get the bundled metadata: {:?}", err)),
        }),
    }
}
