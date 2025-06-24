pub mod ibus;
pub mod icons;
pub mod integrations;
mod sway;
mod x11;

use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};

use fig_os_shim::Context;
use fig_util::Terminal;
use fig_util::consts::linux::DESKTOP_APP_WM_CLASS;
use fig_util::system_info::linux::{
    DesktopEnvironment,
    DisplayServer,
    get_desktop_environment,
    get_display_server,
};
use parking_lot::Mutex;
use serde::Serialize;
use tao::dpi::{
    LogicalPosition,
    PhysicalPosition,
    PhysicalSize,
    Position,
};
use tracing::{
    error,
    info,
    trace,
    warn,
};

use self::x11::X11State;
use super::PlatformBoundEvent;
use crate::platform::linux::sway::SwayState;
use crate::protocol::icons::{
    AssetSpecifier,
    ProcessedAsset,
};
use crate::utils::Rect;
use crate::webview::notification::WebviewNotificationsState;
use crate::webview::{
    FigIdMap,
    WindowId,
};
use crate::{
    EventLoopProxy,
    EventLoopWindowTarget,
};

/// Whether or not the desktop app has received a request containing
/// window data (e.g. window focus, position, etc.). Essentially if this
/// is false, then we know autocomplete is not working.
///
/// From where we receive requests depends on the display server protocol in use:
/// - X11: directly from a connection with X Server
/// - Wayland (GNOME): from the GNOME shell extension
static WM_REVICED_DATA: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Copy, Clone, Serialize)]
#[allow(dead_code)] // we will definitely need inner_x and inner_y at some point
pub(super) struct ActiveWindowData {
    // Inner params correspond to https://mutter.gnome.org/meta/method.Window.get_frame_rect.html
    inner_x: i32,
    inner_y: i32,
    inner_width: i32,
    inner_height: i32,
    // Outer params correspond to https://mutter.gnome.org/meta/method.Window.get_buffer_rect.html
    outer_x: i32,
    outer_y: i32,
    outer_width: i32,
    outer_height: i32,
    scale: f32,
}

impl From<ActiveWindowData> for Rect {
    fn from(value: ActiveWindowData) -> Rect {
        Rect {
            position: PhysicalPosition::new(value.outer_x, value.outer_y).into(),
            size: PhysicalSize::new(value.outer_width, value.outer_height).into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) enum DisplayServerState {
    X11(Arc<x11::X11State>),
    /// Used in GNOME.
    Mutter,
    /// Not supported
    Sway(Arc<sway::SwayState>),
}

#[derive(Debug)]
pub struct PlatformWindowImpl;

#[derive(Debug, Serialize)]
pub struct PlatformStateImpl {
    #[serde(skip)]
    pub(super) proxy: EventLoopProxy,

    /// Dimensions of the window currently in focus.
    pub(super) active_window_data: Mutex<Option<ActiveWindowData>>,

    /// State associated with the detected display server.
    pub(super) display_server_state: Mutex<Option<DisplayServerState>>,

    /// The terminal emulator currently in focus. Note that this does
    /// not include "special" terminals like tmux.
    pub(super) active_terminal: Mutex<Option<Terminal>>,

    pub(super) ibus_connected: AtomicBool,
}

impl PlatformStateImpl {
    pub(super) fn new(proxy: EventLoopProxy) -> Self {
        Self {
            proxy,
            active_window_data: Mutex::new(None),
            display_server_state: Mutex::new(None),
            active_terminal: Mutex::new(None),
            ibus_connected: AtomicBool::new(false),
        }
    }

    pub(super) fn handle(
        self: &Arc<Self>,
        event: PlatformBoundEvent,
        _: &EventLoopWindowTarget,
        _: &FigIdMap,
        _: &Arc<WebviewNotificationsState>,
    ) -> anyhow::Result<()> {
        match event {
            PlatformBoundEvent::Initialize => {
                let platform_state = self.clone();
                tokio::runtime::Handle::current().spawn(async move {
                    let proxy_ = platform_state.proxy.clone();
                    match get_display_server(&Context::new()) {
                        Ok(DisplayServer::X11) => {
                            info!("Detected X11 server");

                            let x11_state = Arc::new(X11State::default());
                            *platform_state.display_server_state.lock() =
                                Some(DisplayServerState::X11(x11_state.clone()));

                            let platform_state_ = platform_state.clone();
                            tokio::spawn(async { x11::handle_x11(proxy_, x11_state, platform_state_).await });
                        },
                        Ok(DisplayServer::Wayland) => {
                            info!("Detected Wayland server");

                            match get_desktop_environment(&Context::new()) {
                                Ok(env @ DesktopEnvironment::Gnome) => {
                                    info!("Detected {env:?}");
                                    *platform_state.display_server_state.lock() = Some(DisplayServerState::Mutter);
                                },
                                Ok(env @ DesktopEnvironment::Plasma) => {
                                    info!("Detected {env:?}");
                                },
                                Ok(DesktopEnvironment::Sway) => {
                                    if let Ok(sway_socket) = std::env::var("SWAYSOCK") {
                                        info!(%sway_socket, "Detected sway");
                                        let (sway_tx, sway_rx) = flume::unbounded();
                                        let sway_state = Arc::new(SwayState {
                                            active_window_rect: Mutex::new(None),
                                            active_terminal: Mutex::new(None),
                                            sway_tx,
                                        });
                                        *platform_state.display_server_state.lock() =
                                            Some(DisplayServerState::Sway(sway_state.clone()));
                                        tokio::spawn(async {
                                            sway::handle_sway(proxy_, sway_state, sway_socket, sway_rx).await;
                                        });
                                    }
                                },
                                Ok(env) => warn!("Detected non wayland compositor {env:?}"),
                                Err(err) => error!(%err, "Unknown wayland compositor"),
                            }
                        },
                        Err(err) => error!(%err, "Unable to detect display server"),
                    }

                    if let Err(err) = icons::init() {
                        error!(%err, "Unable to initialize icons");
                    }

                    if let Err(err) =
                        ibus::launch_ibus_connection(platform_state.proxy.clone(), platform_state.clone()).await
                    {
                        error!(%err, "Unable to initialize ibus");
                    }
                });
            },
            PlatformBoundEvent::InitializePostRun => {
                trace!("Ignoring initialize post run event");
            },
            PlatformBoundEvent::EditBufferChanged => {
                trace!("Ignoring edit buffer changed event");
            },
            PlatformBoundEvent::FullscreenStateUpdated { .. } => {
                trace!("Ignoring full screen state updated event");
            },
            PlatformBoundEvent::AccessibilityUpdated { .. } => {
                trace!("Ignoring accessibility updated event");
            },
            PlatformBoundEvent::AppWindowFocusChanged { .. } => {
                trace!("Ignoring app window focus changed event");
            },
            PlatformBoundEvent::CaretPositionUpdateRequested => {
                trace!("Ignoring caret position update requested event");
            },
            PlatformBoundEvent::WindowDestroyed { .. } => {
                trace!("Ignoring window destroyed event");
            },
            PlatformBoundEvent::ExternalWindowFocusChanged { .. } => {
                trace!("Ignoring external window focus changed event");
            },
            PlatformBoundEvent::AccessibilityUpdateRequested => {
                trace!("Ignoring accessibility update requested event");
            },
        }
        Ok(())
    }

    pub(super) fn position_window(
        &self,
        webview_window: &tao::window::Window,
        _window_id: &WindowId,
        position: Position,
    ) -> wry::Result<()> {
        match &*self.display_server_state.lock() {
            Some(DisplayServerState::Sway(sway)) => {
                let (x, y) = match position {
                    Position::Physical(PhysicalPosition { x, y }) => (x, y),
                    // TODO(grant): prob do something with logical position here
                    Position::Logical(LogicalPosition { x, y }) => (x as i32, y as i32),
                };

                if let Err(err) = sway.sway_tx.send(sway::SwayCommand::PositionWindow {
                    x: x as i64,
                    y: y as i64,
                }) {
                    tracing::warn!(%err, "Failed to send sway command");
                }
            },
            _ => {
                webview_window.set_outer_position(position);
            },
        };
        Ok(())
    }

    #[allow(clippy::unused_self)]
    pub(super) fn get_cursor_position(&self) -> Option<crate::utils::Rect> {
        None
    }

    pub(super) fn get_active_window(&self) -> Option<super::PlatformWindow> {
        match &*self.display_server_state.lock() {
            Some(DisplayServerState::X11(x11_state)) => x11_state.active_window.lock().as_ref().and_then(|window| {
                window.window_geometry.map(|rect| super::PlatformWindow {
                    rect,
                    inner: PlatformWindowImpl,
                })
            }),
            Some(DisplayServerState::Mutter) => self.active_window_data.lock().map(|window| super::PlatformWindow {
                rect: window.into(),
                inner: PlatformWindowImpl,
            }),
            _ => None,
        }
    }

    pub(super) async fn icon_lookup(asset: &AssetSpecifier<'_>) -> Option<ProcessedAsset> {
        match asset {
            AssetSpecifier::Named(name) => icons::lookup(name).await,
            AssetSpecifier::PathBased(path) => {
                if let Ok(metadata) = path.metadata() {
                    let name = if metadata.is_dir() {
                        Some("folder")
                    } else if metadata.is_file() {
                        Some("text-x-generic-template")
                    } else {
                        None
                    };
                    if let Some(name) = name {
                        icons::lookup(name).await
                    } else {
                        None
                    }
                } else {
                    icons::lookup(if path.to_str().map(|x| x.ends_with('/')).unwrap_or_default() {
                        "folder"
                    } else {
                        "text-x-generic-template"
                    })
                    .await
                }
            },
        }
    }

    pub fn accessibility_is_enabled() -> Option<bool> {
        None
    }
}

pub fn autocomplete_active() -> bool {
    WM_REVICED_DATA.load(Ordering::Relaxed)
}

pub mod gtk {
    use super::DESKTOP_APP_WM_CLASS;

    /// Initializes gtk, setting the X11 WM_CLASS to [FIG_WM_CLASS]. This should be called before
    /// creating any windows or webviews.
    ///
    /// This does almost the exact same as [gtk::init] except we need
    /// to keep the WM_CLASS consistent by always using [FIG_WM_CLASS]
    /// instead of the program name.
    pub fn init() -> Result<(), gtk::glib::BoolError> {
        use gtk::glib::translate::{
            ToGlibPtr,
            from_glib,
        };
        use gtk::{
            ffi,
            glib,
            is_initialized,
            set_initialized,
        };

        if gtk::is_initialized_main_thread() {
            return Ok(());
        } else if is_initialized() {
            panic!("Attempted to initialize GTK from two different threads.");
        }
        unsafe {
            let name = [DESKTOP_APP_WM_CLASS];
            if from_glib(ffi::gtk_init_check(&mut 1, &mut name.to_glib_none().0)) {
                let result: bool = from_glib(glib::ffi::g_main_context_acquire(
                    gtk::glib::ffi::g_main_context_default(),
                ));
                if !result {
                    return Err(glib::bool_error!("Failed to acquire default main context"));
                }
                set_initialized();
                Ok(())
            } else {
                Err(glib::bool_error!("Failed to initialize GTK"))
            }
        }
    }
}
