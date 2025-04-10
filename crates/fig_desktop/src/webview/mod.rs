pub mod autocomplete;
pub mod companion;
pub mod dashboard;
pub mod menu;
pub mod notification;
pub mod window;
pub mod window_id;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{
    Arc,
    LazyLock,
    OnceLock,
};
use std::time::Duration;

use cfg_if::cfg_if;
use fig_desktop_api::init_script::javascript_init;
use fig_desktop_api::kv::DashKVStore;
use fig_os_shim::Context;
use fig_proto::fig::ClientOriginatedMessage;
use fig_proto::fig::client_originated_message::Submessage;
use fig_remote_ipc::figterm::FigtermState;
use fig_util::consts::PRODUCT_NAME;
use fig_util::{
    URL_SCHEMA,
    directories,
};
use fnv::FnvBuildHasher;
use muda::MenuEvent;
use regex::RegexSet;
use tao::dpi::LogicalSize;
use tao::event::{
    Event as WryEvent,
    StartCause,
    WindowEvent as WryWindowEvent,
};
use tao::event_loop::{
    ControlFlow,
    EventLoopBuilder,
};
use tao::window::{
    Theme as TaoTheme,
    Window,
    WindowBuilder,
    WindowId as WryWindowId,
};
use tokio::time::MissedTickBehavior;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};
use url::Url;
use window::WindowState;
use wry::{
    Theme as WryTheme,
    WebContext,
    WebView,
    WebViewBuilder,
};

use self::menu::menu_bar;
use self::notification::WebviewNotificationsState;
use self::window_id::DashboardId;
use crate::event::{
    Event,
    ShowMessageNotification,
    WindowEvent,
};
use crate::notification_bus::{
    JsonNotification,
    NOTIFICATION_BUS,
};
use crate::platform::{
    PlatformBoundEvent,
    PlatformState,
};
use crate::protocol::spec::clear_index_cache;
use crate::protocol::{
    api,
    icons,
    resource,
    spec,
};
use crate::remote_ipc::RemoteHook;
use crate::request::api_request;
use crate::tray::{
    self,
    build_tray,
    get_context_menu,
    get_icon,
};
use crate::webview::window_id::AutocompleteId;
pub use crate::webview::window_id::{
    AUTOCOMPLETE_ID,
    DASHBOARD_ID,
    WindowId,
};
use crate::{
    EventLoop,
    EventLoopProxy,
    InterceptState,
    auth_watcher,
    file_watcher,
    local_ipc,
    utils,
};

pub const DASHBOARD_SIZE: LogicalSize<f64> = LogicalSize::new(960.0, 720.0);
pub const DASHBOARD_MINIMUM_SIZE: LogicalSize<f64> = LogicalSize::new(700.0, 480.0);

pub const AUTOCOMPLETE_WINDOW_TITLE: &str = "Fig Autocomplete";

pub const LOGIN_PATH: &str = "/";

fn map_theme(theme: &str) -> Option<WryTheme> {
    match theme {
        "dark" => Some(WryTheme::Dark),
        "light" => Some(WryTheme::Light),
        _ => None,
    }
}

fn to_tao_theme(theme: WryTheme) -> Option<TaoTheme> {
    match theme {
        WryTheme::Dark => Some(TaoTheme::Dark),
        WryTheme::Light => Some(TaoTheme::Light),
        WryTheme::Auto => None,
    }
}

pub static THEME: LazyLock<Option<WryTheme>> = LazyLock::new(|| {
    fig_settings::settings::get_string("app.theme")
        .ok()
        .flatten()
        .as_deref()
        .and_then(map_theme)
});

pub type FigIdMap = HashMap<WindowId, Rc<WindowState>, FnvBuildHasher>;
pub type WryIdMap = HashMap<WryWindowId, Rc<WindowState>, FnvBuildHasher>;

pub struct WebviewManager {
    fig_id_map: FigIdMap,
    window_id_map: WryIdMap,
    event_loop: EventLoop,
    figterm_state: Arc<FigtermState>,
    intercept_state: Arc<InterceptState>,
    platform_state: Arc<PlatformState>,
    notifications_state: Arc<WebviewNotificationsState>,
    dash_kv_store: Arc<DashKVStore>,
    context: Arc<Context>,
}

pub static GLOBAL_PROXY: OnceLock<EventLoopProxy> = OnceLock::new();
pub static FIGTERM_STATE: OnceLock<Arc<FigtermState>> = OnceLock::new();
pub static INTERCEPT_STATE: OnceLock<Arc<InterceptState>> = OnceLock::new();
pub static PLATFORM_STATE: OnceLock<Arc<PlatformState>> = OnceLock::new();
pub static NOTIFICATIONS_STATE: OnceLock<Arc<WebviewNotificationsState>> = OnceLock::new();
pub static DASH_KV_STORE: OnceLock<Arc<DashKVStore>> = OnceLock::new();

impl WebviewManager {
    #[allow(unused_variables)]
    #[allow(unused_mut)]
    pub fn new(context: Arc<Context>, visible: bool) -> Self {
        let mut event_loop = EventLoopBuilder::with_user_event().build();

        #[cfg(target_os = "macos")]
        if !visible {
            use tao::platform::macos::{
                ActivationPolicy,
                EventLoopExtMacOS,
            };

            use crate::platform::ACTIVATION_POLICY;

            *ACTIVATION_POLICY.lock().unwrap() = ActivationPolicy::Accessory;
            event_loop.set_activation_policy(ActivationPolicy::Accessory);
        }

        let proxy = event_loop.create_proxy();
        GLOBAL_PROXY.set(proxy.clone()).unwrap();

        let figterm_state = Arc::new(FigtermState::default());
        FIGTERM_STATE.set(figterm_state.clone()).unwrap();

        let intercept_state = Arc::new(InterceptState::default());
        INTERCEPT_STATE.set(intercept_state.clone()).unwrap();

        let platform_state = Arc::new(PlatformState::new(proxy));
        PLATFORM_STATE.set(platform_state.clone()).unwrap();

        let notifications_state = Arc::new(WebviewNotificationsState::default());
        NOTIFICATIONS_STATE.set(notifications_state.clone()).unwrap();

        let dash_kv_store = Arc::new(DashKVStore::new());
        DASH_KV_STORE.set(dash_kv_store.clone()).unwrap();

        Self {
            fig_id_map: Default::default(),
            window_id_map: Default::default(),
            event_loop,
            figterm_state,
            intercept_state,
            platform_state,
            notifications_state,
            dash_kv_store,
            context,
        }
    }

    fn insert_webview(
        &mut self,
        window: Window,
        window_id: WindowId,
        webview: WebView,
        context: WebContext,
        enabled: bool,
        url: Url,
    ) {
        let webview_arc = Rc::new(WindowState::new(
            window,
            window_id.clone(),
            webview,
            context,
            enabled,
            url,
        ));
        self.fig_id_map.insert(window_id, webview_arc.clone());
        self.window_id_map.insert(webview_arc.window.id(), webview_arc);
    }

    pub fn build_webview<T>(
        &mut self,
        window_id: WindowId,
        builder: impl Fn(Arc<Context>, &mut WebContext, &EventLoop, T) -> anyhow::Result<(Window, WebView)>,
        options: T,
        enabled: bool,
        url_fn: impl Fn() -> Url,
    ) -> anyhow::Result<()> {
        let web_context_path = directories::fig_data_dir()?
            .join("webcontexts")
            .join(window_id.0.as_ref());
        let mut web_context = WebContext::new(Some(web_context_path));
        let (window, webview) = builder(Arc::clone(&self.context), &mut web_context, &self.event_loop, options)?;
        self.insert_webview(window, window_id, webview, web_context, enabled, url_fn());
        Ok(())
    }

    #[allow(unused_mut)]
    pub async fn run(mut self) -> wry::Result<()> {
        self.platform_state
            .handle(
                PlatformBoundEvent::Initialize,
                &self.event_loop,
                &self.fig_id_map,
                &self.notifications_state,
            )
            .expect("Failed to initialize platform state");

        // TODO: implement
        // tokio::spawn(figterm::clean_figterm_cache(self.figterm_state.clone()));

        // Start the local ipc task, listens for requests to the desktop socket.
        {
            let platform_state = self.platform_state.clone();
            let figterm_state = self.figterm_state.clone();
            let notifications_state = self.notifications_state.clone();
            let event_loop = self.event_loop.create_proxy();
            tokio::spawn(async move {
                match local_ipc::start_local_ipc(platform_state, figterm_state, notifications_state, event_loop).await {
                    Ok(_) => (),
                    Err(err) => error!("Unable to start local ipc: {:?}", err),
                }
            });
        }

        tokio::spawn(fig_remote_ipc::remote::start_remote_ipc(
            fig_util::directories::local_remote_socket_path().unwrap(),
            self.figterm_state.clone(),
            RemoteHook {
                notifications_state: self.notifications_state.clone(),
                proxy: self.event_loop.create_proxy(),
            },
        ));

        let (api_handler_tx, mut api_handler_rx) = tokio::sync::mpsc::unbounded_channel::<(WindowId, String)>();
        let (sync_api_handler_tx, mut sync_api_handler_rx) = tokio::sync::mpsc::unbounded_channel::<(
            WindowId,
            fig_desktop_api::error::Result<ClientOriginatedMessage>,
        )>();

        {
            let sync_proxy = self.event_loop.create_proxy();
            let sync_figterm_state = self.figterm_state.clone();
            let sync_intercept_state = self.intercept_state.clone();
            let sync_notifications_state = self.notifications_state.clone();
            let dash_kv_store = self.dash_kv_store.clone();

            tokio::spawn(async move {
                while let Some((fig_id, message)) = sync_api_handler_rx.recv().await {
                    let proxy = sync_proxy.clone();
                    let figterm_state = sync_figterm_state.clone();
                    let intercept_state = sync_intercept_state.clone();
                    let notifications_state = sync_notifications_state.clone();
                    api_request(
                        fig_id,
                        message,
                        &figterm_state,
                        &intercept_state,
                        &notifications_state,
                        &proxy.clone(),
                        &dash_kv_store,
                    )
                    .await;
                }
            });

            let proxy = self.event_loop.create_proxy();
            let figterm_state = self.figterm_state.clone();
            let intercept_state = self.intercept_state.clone();
            let notifications_state = self.notifications_state.clone();
            let dash_kv_store = self.dash_kv_store.clone();

            tokio::spawn(async move {
                while let Some((fig_id, payload)) = api_handler_rx.recv().await {
                    let message = fig_desktop_api::handler::request_from_b64(&payload);
                    if matches!(
                        message,
                        Ok(ClientOriginatedMessage {
                            id: _,
                            submessage: Some(Submessage::PositionWindowRequest(_) | Submessage::WindowFocusRequest(_))
                        })
                    ) {
                        sync_api_handler_tx.send((fig_id, message)).ok();
                    } else {
                        let proxy = proxy.clone();
                        let figterm_state = figterm_state.clone();
                        let intercept_state = intercept_state.clone();
                        let notifications_state = notifications_state.clone();
                        let dash_kv_store = dash_kv_store.clone();
                        tokio::spawn(async move {
                            api_request(
                                fig_id,
                                message,
                                &figterm_state,
                                &intercept_state,
                                &notifications_state,
                                &proxy.clone(),
                                &dash_kv_store,
                            )
                            .await;
                        });
                    }
                }
            });
        }

        file_watcher::setup_listeners(self.notifications_state.clone(), self.event_loop.create_proxy()).await;
        auth_watcher::spawn_auth_watcher();

        init_webview_notification_listeners(self.event_loop.create_proxy()).await;

        let tray_visible = !fig_settings::settings::get_bool_or("app.hideMenubarIcon", false);
        let tray = build_tray(&self.event_loop, &self.figterm_state).await.unwrap();
        if let Err(err) = tray.set_visible(tray_visible) {
            error!(%err, "Failed to set tray visible");
        }

        #[allow(unused_variables)]
        let menu_bar = menu_bar();

        // TODO: fix these
        // #[cfg(target_os = "windows")]
        // menu_bar.init_for_hwnd(window_hwnd);
        // #[cfg(target_os = "linux")]
        // menu_bar.init_for_gtk_window(&gtk_window, Some(&vertical_gtk_box));
        #[cfg(target_os = "macos")]
        menu_bar.init_for_nsapp();

        let proxy = self.event_loop.create_proxy();
        proxy
            .send_event(Event::PlatformBoundEvent(PlatformBoundEvent::InitializePostRun))
            .expect("Failed to send post init event");

        self.event_loop.run(move |event, window_target, control_flow| {
            *control_flow = ControlFlow::Wait;
            trace!(?event, "Main loop event");

            if let Ok(menu_event) = MenuEvent::receiver().try_recv() {
                info!(?menu_event, "Menu Event");
                menu::handle_event(&menu_event, &proxy);
                tray::handle_event(&menu_event, &proxy);
            }

            match event {
                WryEvent::NewEvents(StartCause::Init) => info!("Fig has started"),
                WryEvent::WindowEvent { event, window_id, .. } => {
                    if let Some(window_state) = self.window_id_map.get(&window_id) {
                        match event {
                            WryWindowEvent::CloseRequested => {
                                // This is async so we need to pass 'visible' explicitly
                                window_state.window.set_visible(false);

                                if window_state.window_id == DASHBOARD_ID {
                                    proxy
                                        .send_event(Event::PlatformBoundEvent(
                                            PlatformBoundEvent::AppWindowFocusChanged {
                                                window_id: DASHBOARD_ID,
                                                focused: true, /* set to true, in order to update activation
                                                                * policy & remove from dock */
                                                fullscreen: false,
                                                visible: false,
                                            },
                                        ))
                                        .ok();
                                }
                            },
                            WryWindowEvent::ThemeChanged(theme) => window_state.set_theme(match theme {
                                TaoTheme::Light => Some(WryTheme::Light),
                                TaoTheme::Dark => Some(WryTheme::Dark),
                                _ => None,
                            }),
                            WryWindowEvent::Focused(focused) => {
                                if focused && window_state.window_id != AUTOCOMPLETE_ID {
                                    proxy
                                        .send_event(Event::WindowEvent {
                                            window_id: AUTOCOMPLETE_ID,
                                            window_event: WindowEvent::Hide,
                                        })
                                        .unwrap();
                                }

                                proxy
                                    .send_event(Event::PlatformBoundEvent(PlatformBoundEvent::AppWindowFocusChanged {
                                        window_id: window_state.window_id.clone(),
                                        focused,
                                        fullscreen: window_state.window.fullscreen().is_some(),
                                        visible: window_state.window.is_visible(),
                                    }))
                                    .unwrap();
                            },
                            _ => (),
                        }
                    }
                },
                WryEvent::UserEvent(event) => {
                    match event {
                        Event::WindowEvent {
                            window_id,
                            window_event,
                        } => match self.fig_id_map.get(&window_id) {
                            Some(window_state) => {
                                if window_state.enabled() || window_event.is_allowed_while_disabled() {
                                    window_state.handle(
                                        window_event,
                                        &self.figterm_state,
                                        &self.platform_state,
                                        &self.notifications_state,
                                        window_target,
                                        &api_handler_tx,
                                    );
                                } else {
                                    trace!(
                                        window_id =% window_state.window_id,
                                        ?window_event,
                                        "Ignoring event for disabled window"
                                    );
                                }
                            },
                            None => {
                                // TODO(grant): figure out how to handle this gracefully
                                warn!("No window {window_id} available for event");
                                trace!(?window_event, "Event");
                            },
                        },
                        Event::WindowEventAll { window_event } => {
                            for (_window_id, window_state) in self.window_id_map.iter() {
                                if window_state.enabled() || window_event.is_allowed_while_disabled() {
                                    window_state.handle(
                                        window_event.clone(),
                                        &self.figterm_state,
                                        &self.platform_state,
                                        &self.notifications_state,
                                        window_target,
                                        &api_handler_tx,
                                    );
                                } else {
                                    trace!(
                                        window_id =% window_state.window_id,
                                        ?window_event,
                                        "Ignoring event for disabled window"
                                    );
                                }
                            }
                        },
                        Event::ControlFlow(new_control_flow) => {
                            *control_flow = new_control_flow;
                        },
                        Event::ReloadTray { is_logged_in } => {
                            tray.set_icon(Some(get_icon(is_logged_in)))
                                .map_err(|err| error!(?err))
                                .ok();
                            tray.set_icon_as_template(true);
                            tray.set_menu(Some(Box::new(get_context_menu(is_logged_in))));
                        },
                        Event::ReloadCredentials => {
                            // tray.set_menu(Some(Box::new(get_context_menu())));

                            let autocomplete_enabled =
                                !fig_settings::settings::get_bool_or("autocomplete.disable", false)
                                    && PlatformState::accessibility_is_enabled().unwrap_or(true);
                            // && fig_request::fig_auth::is_logged_in();

                            proxy
                                .send_event(Event::WindowEvent {
                                    window_id: AUTOCOMPLETE_ID,
                                    window_event: WindowEvent::SetEnabled(autocomplete_enabled),
                                })
                                .unwrap();
                        },
                        Event::ReloadAccessibility => {
                            // tray.set_menu(Some(Box::new(get_context_menu())));

                            let autocomplete_enabled =
                                !fig_settings::settings::get_bool_or("autocomplete.disable", false)
                                    && PlatformState::accessibility_is_enabled().unwrap_or(true);
                            // && fig_request::fig_auth::is_logged_in();

                            proxy
                                .send_event(Event::WindowEvent {
                                    window_id: AUTOCOMPLETE_ID,
                                    window_event: WindowEvent::SetEnabled(autocomplete_enabled),
                                })
                                .unwrap();
                        },
                        Event::SetTrayVisible(visible) => {
                            if let Err(err) = tray.set_visible(visible) {
                                error!(%err, "Failed to set tray visible");
                            }
                        },
                        Event::PlatformBoundEvent(native_event) => {
                            if let Err(err) = self.platform_state.handle(
                                native_event,
                                window_target,
                                &self.fig_id_map,
                                &self.notifications_state,
                            ) {
                                debug!(%err, "Failed to handle native event");
                            }
                        },
                        Event::ShowMessageNotification(ShowMessageNotification {
                            title,
                            body,
                            parent,
                            buttons,
                            buttons_result,
                        }) => {
                            let mut dialog = rfd::AsyncMessageDialog::new().set_title(title).set_description(body);

                            if let Some(parent) = parent {
                                if let Some(parent_window) = self.fig_id_map.get(&parent) {
                                    dialog = dialog.set_parent(&parent_window.window);
                                }
                            }

                            let dialog = match (buttons, buttons_result.as_ref()) {
                                (Some(buttons), Some(_)) => dialog.set_buttons(buttons),
                                _ => dialog,
                            };

                            tokio::spawn(async move {
                                let res = dialog.show().await;
                                if let Some(buttons_result) = buttons_result {
                                    buttons_result
                                        .send(res)
                                        .await
                                        .map_err(|err| error!(?err, "Failed to send dialog result"))
                                        .ok();
                                }
                            });
                        },
                    }
                },
                WryEvent::Opened { urls } => {
                    let mut events = Vec::new();
                    for url in urls {
                        if url.scheme() == URL_SCHEMA {
                            match url.host_str() {
                                Some("dashboard") => {
                                    events.push(WindowEvent::NavigateRelative {
                                        path: url.path().to_owned().into(),
                                    });
                                },
                                host => {
                                    error!(?host, "Invalid deep link");
                                },
                            }
                        } else {
                            error!(scheme = %url.scheme(), %url, "Invalid scheme");
                        }
                    }

                    if let Err(err) = proxy.send_event(Event::WindowEvent {
                        window_id: DASHBOARD_ID,
                        window_event: WindowEvent::Batch(events),
                    }) {
                        warn!(%err, "Error sending event");
                    }
                },
                WryEvent::MainEventsCleared | WryEvent::NewEvents(StartCause::WaitCancelled { .. }) => {},
                event => trace!(?event, "Unhandled event"),
            }
        });
    }
}

fn navigation_handler<I, S>(window_id: WindowId, exprs: I) -> impl Fn(String) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let regex_set = RegexSet::new(exprs);

    if let Err(ref err) = regex_set {
        error!("Failed to compile regex: {err}");
    }

    move |url: String| match regex_set.as_ref().ok().and_then(|r| {
        Url::parse(&url)
            .ok()
            .and_then(|url| url.domain().map(|domain| r.is_match(domain)))
    }) {
        Some(true) => {
            trace!("{window_id} allowed url: {url}");
            true
        },
        Some(false) | None => {
            warn!("{window_id} denied url: {url}");
            false
        },
    }
}

pub struct DashboardOptions {
    pub show_onboarding: bool,
    pub visible: bool,
    pub page: Option<String>,
}

pub fn build_dashboard(
    ctx: Arc<Context>,
    web_context: &mut WebContext,
    event_loop: &EventLoop,
    DashboardOptions {
        show_onboarding,
        visible,
        page,
    }: DashboardOptions,
) -> anyhow::Result<(Window, WebView)> {
    let window = WindowBuilder::new()
        .with_title(PRODUCT_NAME)
        .with_inner_size(DASHBOARD_SIZE)
        .with_min_inner_size(DASHBOARD_MINIMUM_SIZE)
        .with_resizable(true)
        .with_maximizable(false)
        .with_visible(visible)
        .with_focused(visible)
        .with_always_on_top(false)
        .with_window_icon(Some(utils::icon()))
        .with_theme(THEME.and_then(to_tao_theme))
        .build(event_loop)?;

    // #[cfg(not(target_os = "linux"))]
    // {
    //     window = window.with_menu(menu::menu_bar());
    // }

    #[cfg(target_os = "linux")]
    {
        use gtk::traits::GtkWindowExt;
        use tao::platform::unix::WindowExtUnix;

        window.gtk_window().set_role("dashboard");
    }

    let proxy = event_loop.create_proxy();

    let mut url = dashboard::url();

    if show_onboarding {
        url.set_path(LOGIN_PATH);
    } else if let Some(page) = page {
        url.set_path(&page);
    }

    let webview_builder = WebViewBuilder::with_web_context(web_context)
        .with_url(url.as_str())
        .with_ipc_handler(move |payload| {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID.clone(),
                    window_event: WindowEvent::Api {
                        payload: payload.into_body(),
                    },
                })
                .unwrap();
        })
        .with_devtools(true)
        .with_asynchronous_custom_protocol(
            "qcliresource".into(),
            utils::wrap_custom_protocol(
                Arc::clone(&ctx),
                "qcliresource::Dashboard",
                DashboardId,
                resource::handle::<resource::Dashboard>,
            ),
        )
        .with_asynchronous_custom_protocol(
            "api".into(),
            utils::wrap_custom_protocol(Arc::clone(&ctx), "api", DashboardId, api::handle),
        )
        .with_navigation_handler(navigation_handler(DASHBOARD_ID, &[r"^localhost$", r"^127\.0\.0\.1$"]))
        .with_initialization_script(&javascript_init(true))
        .with_clipboard(true)
        .with_hotkeys_zoom(true);

    cfg_if! {
        if #[cfg(target_os = "linux")] {
            use tao::platform::unix::WindowExtUnix;
            use wry::WebViewBuilderExtUnix;
            let vbox = window.default_vbox().unwrap();
            let webview = webview_builder.build_gtk(vbox)?;
        } else {
            let webview = webview_builder.build(&window)?;
        }
    };

    Ok((window, webview))
}

pub struct AutocompleteOptions;

pub fn build_autocomplete(
    ctx: Arc<Context>,
    web_context: &mut WebContext,
    event_loop: &EventLoop,
    _autocomplete_options: AutocompleteOptions,
) -> anyhow::Result<(Window, WebView)> {
    let mut window_builder = WindowBuilder::new()
        .with_title(AUTOCOMPLETE_WINDOW_TITLE)
        .with_transparent(true)
        .with_decorations(false)
        .with_always_on_top(true)
        .with_focused(false)
        .with_window_icon(Some(utils::icon()))
        .with_inner_size(LogicalSize::new(1.0, 1.0))
        .with_theme(THEME.and_then(to_tao_theme));

    cfg_if!(
        if #[cfg(target_os = "linux")] {
            use tao::platform::unix::WindowBuilderExtUnix;
            window_builder = window_builder.with_resizable(true).with_skip_taskbar(true);
        } else if #[cfg(target_os = "macos")] {
            use tao::platform::macos::WindowBuilderExtMacOS;
            window_builder = window_builder.with_resizable(false).with_has_shadow(false).with_visible(false);
        } else if #[cfg(target_os = "windows")] {
            use tao::platform::windows::WindowBuilderExtWindows;
            window_builder = window_builder.with_resizable(false).with_skip_taskbar(true).with_visible(false);
        }
    );

    let window = window_builder.build(event_loop)?;

    #[cfg(target_os = "linux")]
    {
        use gtk::gdk::WindowTypeHint;
        use gtk::traits::{
            GtkWindowExt,
            WidgetExt,
        };
        use tao::platform::unix::WindowExtUnix;

        let gtk_window = window.gtk_window();
        gtk_window.set_role("autocomplete");
        gtk_window.set_type_hint(WindowTypeHint::Utility);
        gtk_window.set_accept_focus(false);
        gtk_window.set_decorated(false);
        if let Some(window) = gtk_window.window() {
            window.set_override_redirect(true);
        }
    }

    let proxy = event_loop.create_proxy();

    let webview_builder = WebViewBuilder::with_web_context(web_context)
        .with_url(autocomplete::url().as_str())
        .with_ipc_handler(move |payload| {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: AUTOCOMPLETE_ID.clone(),
                    window_event: WindowEvent::Api {
                        payload: payload.into_body(),
                    },
                })
                .unwrap();
        })
        .with_asynchronous_custom_protocol(
            "fig".into(),
            utils::wrap_custom_protocol(Arc::clone(&ctx), "fig", AutocompleteId, icons::handle),
        )
        .with_asynchronous_custom_protocol(
            "icon".into(),
            utils::wrap_custom_protocol(Arc::clone(&ctx), "icon", AutocompleteId, icons::handle),
        )
        .with_asynchronous_custom_protocol(
            "spec".into(),
            utils::wrap_custom_protocol(Arc::clone(&ctx), "spec", AutocompleteId, spec::handle),
        )
        .with_asynchronous_custom_protocol(
            "qcliresource".into(),
            utils::wrap_custom_protocol(
                Arc::clone(&ctx),
                "qcliresource::Autocomplete",
                AutocompleteId,
                resource::handle::<resource::Autocomplete>,
            ),
        )
        .with_asynchronous_custom_protocol(
            "api".into(),
            utils::wrap_custom_protocol(Arc::clone(&ctx), "api", AutocompleteId, api::handle),
        )
        .with_devtools(true)
        .with_transparent(true)
        .with_initialization_script(&javascript_init(true))
        .with_navigation_handler(navigation_handler(AUTOCOMPLETE_ID, &[r"localhost$", r"^127\.0\.0\.1$"]))
        .with_clipboard(true)
        .with_hotkeys_zoom(true)
        .with_accept_first_mouse(true);

    cfg_if! {
        if #[cfg(target_os = "linux")] {
            use tao::platform::unix::WindowExtUnix;
            use wry::WebViewBuilderExtUnix;
            let vbox = window.default_vbox().unwrap();
            let webview = webview_builder.build_gtk(vbox)?;
        } else {
            let webview = webview_builder.build(&window)?;
        }
    };

    Ok((window, webview))
}

async fn init_webview_notification_listeners(proxy: EventLoopProxy) {
    #[allow(unused_macros)]
    macro_rules! watcher {
        ($type:ident, $name:expr, $on_update:expr) => {{
            paste::paste! {
                let proxy = proxy.clone();
                tokio::spawn(async move {
                    let mut rx = NOTIFICATION_BUS.[<subscribe_ $type>]($name.into());
                    loop {
                        let res = rx.recv().await;
                        match res {
                            Ok(val) => {
                                #[allow(clippy::redundant_closure_call)]
                                ($on_update)(val, &proxy);
                            },
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                warn!("Notification bus '{}' lagged by {n} messages", $name);
                            },
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
            }
        };};
    }

    #[cfg(target_os = "linux")]
    {
        use fig_integrations::Integration;
        use fig_integrations::desktop_entry::{
            AutostartIntegration,
            should_install_autostart_entry,
        };
        use fig_settings::{
            Settings,
            State,
        };

        use crate::notification_bus::JsonNotification;
        watcher!(
            settings,
            "autocomplete.disable",
            |notification: JsonNotification, proxy: &EventLoopProxy| {
                let enabled = !notification.as_bool().unwrap_or(false);
                debug!(%enabled, "Autocomplete");
                proxy
                    .send_event(Event::WindowEvent {
                        window_id: AUTOCOMPLETE_ID,
                        window_event: WindowEvent::SetEnabled(enabled),
                    })
                    .unwrap();
            }
        );
        watcher!(
            settings,
            "app.launchOnStartup",
            |notification: JsonNotification, _proxy: &EventLoopProxy| {
                let enabled = !notification.as_bool().unwrap_or(true);
                debug!(%enabled, "app.launchOnStartup");
                tokio::spawn(async move {
                    let ctx = Context::new();
                    let settings = Settings::new();
                    let state = State::new();
                    let autostart = match AutostartIntegration::new(&ctx) {
                        Ok(autostart) => autostart,
                        Err(err) => {
                            error!(
                                ?err,
                                "failed to update the autostart integration installed status to {}", enabled
                            );
                            return;
                        },
                    };
                    if should_install_autostart_entry(&ctx, &settings, &state) {
                        autostart
                            .install()
                            .await
                            .map_err(|err| warn!(?err, "unable to install autostart integration"))
                            .ok();
                    } else {
                        autostart
                            .uninstall()
                            .await
                            .map_err(|err| warn!(?err, "unable to uninstall autostart integration"))
                            .ok();
                    }
                });
            }
        );
    }

    watcher!(
        settings,
        "app.theme",
        |notification: JsonNotification, proxy: &EventLoopProxy| {
            let theme = notification.as_string().as_deref().and_then(map_theme);
            debug!(?theme, "Theme changed");
            proxy
                .send_event(Event::WindowEventAll {
                    window_event: WindowEvent::SetTheme(theme),
                })
                .unwrap();
        }
    );

    watcher!(
        settings,
        "app.hideMenubarIcon",
        |notification: JsonNotification, proxy: &EventLoopProxy| {
            let enabled = !notification.as_bool().unwrap_or(false);
            debug!(%enabled, "Tray icon");
            proxy.send_event(Event::SetTrayVisible(enabled)).unwrap();
        }
    );

    watcher!(
        settings,
        "developer.dashboard.host",
        |_notification: JsonNotification, proxy: &EventLoopProxy| {
            let url = dashboard::url();
            debug!(%url, "Dashboard host");
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID,
                    window_event: WindowEvent::NavigateAbsolute { url },
                })
                .unwrap();
        }
    );

    watcher!(
        settings,
        "developer.dashboard.build",
        |_notification: JsonNotification, proxy: &EventLoopProxy| {
            let url = dashboard::url();
            debug!(%url, "Dashboard host");
            proxy
                .send_event(Event::WindowEvent {
                    window_id: DASHBOARD_ID,
                    window_event: WindowEvent::NavigateAbsolute { url },
                })
                .unwrap();
        }
    );

    watcher!(
        settings,
        "developer.autocomplete.host",
        |_notification: JsonNotification, proxy: &EventLoopProxy| {
            let url = autocomplete::url();
            debug!(%url, "Autocomplete host");
            proxy
                .send_event(Event::WindowEvent {
                    window_id: AUTOCOMPLETE_ID,
                    window_event: WindowEvent::NavigateAbsolute { url },
                })
                .unwrap();
        }
    );

    watcher!(
        settings,
        "developer.autocomplete.build",
        |_notification: JsonNotification, proxy: &EventLoopProxy| {
            let url = autocomplete::url();
            debug!(%url, "Autocomplete host");
            proxy
                .send_event(Event::WindowEvent {
                    window_id: AUTOCOMPLETE_ID,
                    window_event: WindowEvent::NavigateAbsolute { url },
                })
                .unwrap();
        }
    );

    // I don't think this is meant to be here anymore
    // watcher!(settings, "app.beta", |_: JsonNotification, proxy: &EventLoopProxy| {
    //     let proxy = proxy.clone();
    //     tokio::spawn(fig_install::update(
    //         Some(Box::new(move |_| {
    //             proxy
    //                 .send_event(Event::ShowMessageNotification {
    //                     title: "Fig Update".into(),
    //                     body: "Fig is updating in the background. You can continue to use Fig while
    // it updates.".into(),                     parent: None,
    //                 })
    //                 .unwrap();
    //         })),
    //         fig_install::UpdateOptions {
    //             ignore_rollout: true,
    //             interactive: true,
    //             relaunch_dashboard: true,
    //         },
    //     ));
    // });

    // Midway watcher
    tokio::spawn(async move {
        let mut res = NOTIFICATION_BUS.subscribe_midway();

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);

        // debounce thread
        tokio::spawn(async move {
            let mut should_send = false;
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = rx.recv() => {
                        should_send = true;
                        interval.reset();
                    }
                    _ = interval.tick() => {
                        if should_send {
                            info!("clearing autocomplete cache");
                            let _ = proxy.send_event(
                                Event::WindowEvent {
                                    window_id: AUTOCOMPLETE_ID,
                                    window_event: WindowEvent::Event {
                                        event_name: "clear-cache".into(),
                                        payload: None
                                    }
                                }
                            );
                            clear_index_cache().await;
                            should_send = false;
                        }
                    }
                }
            }
        });

        loop {
            match res.recv().await {
                Ok(()) => {
                    if let Err(err) = tx.send(()).await {
                        error!("Error sending notification: {err}");
                    }
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Notification bus 'midway' lagged by {n} messages");
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
