pub(crate) mod commands;
mod hooks;

use std::sync::Arc;

use anyhow::{
    Context,
    Result,
};
use fig_install::UpdateOptions;
use fig_ipc::{
    BufferedUnixStream,
    RecvMessage,
    SendMessage,
};
use fig_os_shim::{
    Context as FigContext,
    ContextArcProvider,
    ContextProvider,
};
use fig_proto::local::command_response::Response as CommandResponseTypes;
use fig_proto::local::local_message::Type as LocalMessageType;
use fig_proto::local::{
    CommandResponse,
    ErrorResponse,
    LocalMessage,
    SuccessResponse,
};
use fig_remote_ipc::figterm::FigtermState;
use fig_settings::settings::SettingsProvider;
use fig_settings::{
    Settings,
    State,
    StateProvider,
};
use fig_util::directories;
use tokio::net::UnixListener;
use tracing::{
    debug,
    error,
    trace,
    warn,
};

use crate::event::Event;
use crate::platform::PlatformState;
use crate::webview::notification::WebviewNotificationsState;
use crate::{
    AUTOCOMPLETE_ID,
    DASHBOARD_ID,
    EventLoopProxy,
};

pub enum LocalResponse {
    Error { code: Option<i32>, message: Option<String> },
    Success(Option<String>),
    Message(Box<CommandResponseTypes>),
}

pub type LocalResult = Result<LocalResponse, LocalResponse>;

#[derive(Debug)]
struct LocalIpcContext {
    settings: Settings,
    state: State,
    context: Arc<FigContext>,
}

impl LocalIpcContext {
    fn new() -> Self {
        Self {
            settings: Settings::new(),
            state: State::new(),
            context: FigContext::new(),
        }
    }
}

impl SettingsProvider for LocalIpcContext {
    fn settings(&self) -> &Settings {
        &self.settings
    }
}

impl StateProvider for LocalIpcContext {
    fn state(&self) -> &State {
        &self.state
    }
}

impl ContextProvider for LocalIpcContext {
    fn context(&self) -> &FigContext {
        &self.context
    }
}

impl ContextArcProvider for LocalIpcContext {
    fn context_arc(&self) -> Arc<FigContext> {
        Arc::clone(&self.context)
    }
}

pub async fn start_local_ipc(
    platform_state: Arc<PlatformState>,
    figterm_state: Arc<FigtermState>,
    webview_notifications_state: Arc<WebviewNotificationsState>,
    proxy: EventLoopProxy,
) -> Result<()> {
    let socket_path = directories::desktop_socket_path()?;
    if let Some(parent) = socket_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).context("Failed creating socket path")?;
        }

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, Permissions::from_mode(0o700))?;
        }
    }

    tokio::fs::remove_file(&socket_path).await.ok();

    let listener = UnixListener::bind(&socket_path)?;

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(handle_local_ipc(
            BufferedUnixStream::new(stream),
            platform_state.clone(),
            figterm_state.clone(),
            webview_notifications_state.clone(),
            proxy.clone(),
            LocalIpcContext::new(),
        ));
    }

    Ok(())
}

async fn handle_local_ipc<Ctx>(
    mut stream: BufferedUnixStream,
    platform_state: Arc<PlatformState>,
    figterm_state: Arc<FigtermState>,
    webview_notifications_state: Arc<WebviewNotificationsState>,
    proxy: EventLoopProxy,
    ctx: Ctx,
) where
    Ctx: SettingsProvider + StateProvider + ContextProvider + ContextArcProvider + Send + Sync,
{
    while let Some(message) = stream.recv_message::<LocalMessage>().await.unwrap_or_else(|err| {
        if !err.is_disconnect() {
            error!("Failed receiving local message: {err}");
        }
        None
    }) {
        trace!("Received local message: {message:?}");
        match message.r#type {
            Some(LocalMessageType::Command(command)) => {
                let response = match command.command {
                    None => LocalResponse::Error {
                        code: None,
                        message: Some("Local ipc command was None".into()),
                    },
                    Some(command) => {
                        use fig_proto::local::command::Command::{
                            BundleMetadata,
                            ConnectToIbus,
                            DebugMode,
                            Devtools,
                            Diagnostics,
                            DumpState,
                            InputMethod,
                            ListTerminalIntegrations,
                            LogLevel,
                            Login,
                            Logout,
                            OpenBrowser,
                            OpenUiElement,
                            PromptAccessibility,
                            Quit,
                            ReportWindow,
                            ResetCache,
                            Restart,
                            RestartSettingsListener,
                            RunInstallScript,
                            TerminalIntegration,
                            Update,
                        };

                        match command {
                            DebugMode(command) => commands::debug(command, &proxy).await,
                            OpenUiElement(command) => commands::open_ui_element(command, &proxy).await,
                            Quit(command) => commands::quit(command, &proxy).await,
                            Diagnostics(command) => commands::diagnostic(command, &figterm_state).await,
                            OpenBrowser(command) => commands::open_browser(command).await,
                            PromptAccessibility(_) => commands::prompt_for_accessibility_permission(&ctx).await,
                            LogLevel(command) => commands::log_level(command),
                            Login(_) => commands::login(&proxy).await,
                            Logout(_) => commands::logout(&proxy).await,
                            DumpState(command) => commands::dump_state(
                                command,
                                &figterm_state,
                                &webview_notifications_state,
                                &platform_state,
                            ),
                            ConnectToIbus(_) => commands::connect_to_ibus(proxy.clone(), &platform_state).await,
                            BundleMetadata(_) => commands::bundle_metadata(&ctx.context_arc()).await,
                            Update(_) => fig_install::update(
                                ctx.context_arc(),
                                Some(Box::new(move |_| {
                                    debug!("Updating from proto");
                                })),
                                UpdateOptions::default(),
                            )
                            .await
                            .map(|_| LocalResponse::Success(None))
                            .map_err(|err| LocalResponse::Error {
                                code: None,
                                message: Some(format!("Failed to update: {err}")),
                            }),
                            Devtools(command) => {
                                let window_id = match command.window() {
                                    fig_proto::local::devtools_command::Window::DevtoolsAutocomplete => AUTOCOMPLETE_ID,
                                    fig_proto::local::devtools_command::Window::DevtoolsDashboard => DASHBOARD_ID,
                                };

                                proxy
                                    .send_event(Event::WindowEvent {
                                        window_id,
                                        window_event: crate::event::WindowEvent::Devtools,
                                    })
                                    .ok();

                                Ok(LocalResponse::Success(None))
                            },
                            TerminalIntegration(_)
                            | ListTerminalIntegrations(_)
                            | Restart(_)
                            | ReportWindow(_)
                            | RestartSettingsListener(_)
                            | RunInstallScript(_)
                            | ResetCache(_)
                            | InputMethod(_) => {
                                debug!(?command, "Unhandled command");
                                Err(LocalResponse::Error {
                                    code: None,
                                    message: Some("Unknown command".to_owned()),
                                })
                            },
                        }
                        .unwrap_or_else(|r| r)
                    },
                };

                match command.no_response {
                    Some(true) => {},
                    _ => {
                        let message = {
                            CommandResponse {
                                id: command.id,
                                response: Some(match response {
                                    LocalResponse::Error {
                                        code: exit_code,
                                        message,
                                    } => CommandResponseTypes::Error(ErrorResponse { exit_code, message }),
                                    LocalResponse::Success(message) => {
                                        CommandResponseTypes::Success(SuccessResponse { message })
                                    },
                                    LocalResponse::Message(m) => *m,
                                }),
                            }
                        };

                        if let Err(err) = stream.send_message(message).await {
                            error!(%err, "Failed sending local response");
                            break;
                        }
                    },
                }
            },
            Some(LocalMessageType::Hook(hook)) => {
                use fig_proto::ReflectMessage;
                use fig_proto::local::hook::Hook::{
                    Callback,
                    CaretPosition,
                    ClearAutocompleteCache,
                    EditBuffer,
                    Event,
                    FileChanged,
                    FocusChange,
                    FocusedWindowData,
                    Hide,
                    Init,
                    IntegrationReady,
                    InterceptedKey,
                    KeyboardFocusChanged,
                    OpenedSshConnection,
                    PostExec,
                    PreExec,
                    Prompt,
                    TmuxPaneChanged,
                };

                if let Err(err) = match hook.hook {
                    Some(CaretPosition(request)) => hooks::caret_position(request, &proxy).await,
                    Some(FocusChange(_)) => hooks::focus_change(&proxy).await,
                    Some(FileChanged(request)) => hooks::file_changed(request).await,
                    Some(FocusedWindowData(request)) => {
                        hooks::focused_window_data(request, &platform_state, &proxy).await
                    },
                    Some(KeyboardFocusChanged(_)) => hooks::focus_change(&proxy).await,
                    Some(Event(event)) => hooks::event(event, &proxy).await,
                    Some(ClearAutocompleteCache(event)) => hooks::clear_autocomplete_cache(event, &proxy).await,
                    Some(
                        Init(_)
                        | PostExec(_)
                        | TmuxPaneChanged(_)
                        | OpenedSshConnection(_)
                        | Callback(_)
                        | IntegrationReady(_)
                        | Hide(_)
                        | PreExec(_)
                        | InterceptedKey(_)
                        | EditBuffer(_)
                        | Prompt(_),
                    ) => {
                        warn!("received legacy hook `{}`", hook.descriptor().name());
                        Ok(())
                    },
                    None => {
                        warn!("Received unknown or empty hook");
                        Ok(())
                    },
                } {
                    error!("Error processing hook: {err:?}");
                }
            },
            None => warn!("Received empty local message"),
        }
    }
}
