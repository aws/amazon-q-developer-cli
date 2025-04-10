use std::fmt::Display;
use std::sync::atomic::AtomicBool;

use base64::prelude::*;
use bytes::BytesMut;
use fig_proto::fig::notification::Type as NotificationEnum;
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    EventNotification,
    Notification,
    NotificationType,
    ServerOriginatedMessage,
};
use fig_proto::local::caret_position_hook::Origin;
use fig_proto::prost::Message;
use fig_remote_ipc::figterm::{
    FigtermCommand,
    FigtermState,
};
use parking_lot::Mutex;
use tao::dpi::{
    LogicalPosition,
    LogicalSize,
    Position,
};
use tao::window::Window;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{
    debug,
    error,
    info,
    instrument,
    warn,
};
use url::Url;
use wry::{
    Theme,
    WebContext,
    WebView,
};

use super::notification::WebviewNotificationsState;
use super::to_tao_theme;
use super::window_id::WindowId;
use crate::event::{
    EmitEventName,
    WindowEvent,
    WindowGeometryResult,
    WindowPosition,
};
use crate::platform::{
    self,
    PlatformState,
};
use crate::utils::Rect;
#[allow(unused_imports)]
use crate::{
    AUTOCOMPLETE_ID,
    DASHBOARD_ID,
    EventLoopWindowTarget,
};

pub struct WindowGeometryState {
    /// The outer position of the window by positioning scheme
    pub position: WindowPosition,
    /// The inner size of the window
    pub size: LogicalSize<f64>,
    /// The window anchor, which is added onto the final position
    pub anchor: LogicalSize<f64>,
}

// TODO: Add state for the active terminal window
#[allow(dead_code)]
pub struct WindowState {
    pub window: Window,
    pub webview: WebView,
    pub context: WebContext,
    pub window_id: WindowId,
    pub window_geometry_state: Mutex<WindowGeometryState>,
    pub enabled: AtomicBool,
    pub url: Mutex<Url>,
}

impl WindowState {
    pub fn new(
        window: Window,
        window_id: WindowId,
        webview: WebView,
        context: WebContext,
        enabled: bool,
        url: Url,
    ) -> Self {
        let scale_factor = window.scale_factor();

        let position = window
            .outer_position()
            .expect("Failed to acquire window position")
            .to_logical(scale_factor);

        let size = window.inner_size().to_logical(scale_factor);

        Self {
            window,
            webview,
            context,
            window_id,
            window_geometry_state: Mutex::new(WindowGeometryState {
                position: WindowPosition::Absolute(Position::Logical(position)),
                size,
                anchor: LogicalSize::<f64>::default(),
            }),
            enabled: enabled.into(),
            url: Mutex::new(url),
        }
    }

    #[instrument(skip(self))]
    fn update_window_geometry(
        &self,
        position: Option<WindowPosition>,
        size: Option<LogicalSize<f64>>,
        anchor: Option<LogicalSize<f64>>,
        platform_state: &PlatformState,
        dry_run: bool,
    ) -> (bool, bool) {
        // Lock our atomic state
        let mut state = self.window_geometry_state.lock();

        // Acquire our position, size, and anchor, and update them if dirty
        let position = match position {
            Some(position) if !dry_run => {
                state.position = position;
                position
            },
            Some(position) => position,
            None => state.position,
        };

        let size = match size {
            Some(size) if !dry_run => {
                state.size = size;
                size
            },
            _ => state.size,
        };

        let anchor = match anchor {
            Some(anchor) if !dry_run => {
                state.anchor = anchor;
                anchor
            },
            _ => state.anchor,
        };

        let window = &self.window;

        let (position, is_above, is_clipped) = match position {
            WindowPosition::Absolute(position) => (position, false, false),
            WindowPosition::Centered => {
                let scale_factor = window.scale_factor();

                let Some(monitor) = window.current_monitor() else {
                    return (false, false);
                };

                let monitor_position: LogicalPosition<f64> = monitor.position().to_logical(scale_factor);
                let monitor_size: LogicalSize<f64> = monitor.size().to_logical(scale_factor);

                (
                    Position::Logical(LogicalPosition::new(
                        monitor_position.x + monitor_size.width * 0.5 - size.width * 0.5,
                        monitor_position.y + monitor_size.height * 0.5 - size.height * 0.5,
                    )),
                    false,
                    false,
                )
            },
            WindowPosition::RelativeToCaret {
                caret_position,
                caret_size,
                origin,
            } => {
                let max_height = fig_settings::settings::get_int_or("autocomplete.height", 140) as f64;

                let primary_monitor = window.primary_monitor();
                let primary_scale_factor = primary_monitor.as_ref().map(|monitor| monitor.scale_factor());
                let primary_monitor_size: Option<LogicalSize<f64>> = primary_monitor
                    .as_ref()
                    .and_then(|monitor| Some(monitor.size().to_logical(primary_scale_factor?)));
                let primary_monitor_position: Option<LogicalPosition<f64>> = primary_monitor
                    .as_ref()
                    .and_then(|monitor| Some(monitor.position().to_logical(primary_scale_factor?)));

                let (
                    caret_position,
                    caret_size,
                    overflows_monitor_above,
                    overflows_monitor_below,
                    scale_factor,
                    monitor_frame,
                ) = window
                    .available_monitors()
                    .find_map(|monitor| {
                        let monitor_scale_factor = monitor.scale_factor();
                        let monitor_frame = Rect {
                            position: monitor.position().into(),
                            size: monitor.size().into(),
                        };

                        let mut logical_caret_position: LogicalPosition<f64> =
                            caret_position.to_logical(primary_scale_factor.unwrap_or(monitor_scale_factor));
                        let logical_caret_size: LogicalSize<f64> =
                            caret_size.to_logical(primary_scale_factor.unwrap_or(monitor_scale_factor));

                        match origin {
                            Origin::BottomLeft => {
                                logical_caret_position.y =
                                    primary_monitor_position.map(|position| position.y).unwrap_or_default()
                                        + primary_monitor_size.map(|size| size.height).unwrap_or_default()
                                        - logical_caret_position.y
                                        - logical_caret_size.height;
                            },
                            Origin::TopLeft => {
                                // This is the default
                            },
                        }

                        monitor_frame
                            .contains(logical_caret_position.into(), monitor_scale_factor)
                            .then(|| {
                                let monitor_position: LogicalPosition<f64> =
                                    monitor.position().to_logical(monitor_scale_factor);
                                let monitor_size: LogicalSize<f64> = monitor.size().to_logical(monitor_scale_factor);

                                (
                                    logical_caret_position,
                                    logical_caret_size,
                                    monitor_position.y >= logical_caret_position.y - max_height,
                                    monitor_position.y + monitor_size.height
                                        < logical_caret_position.y + logical_caret_size.height + max_height,
                                    monitor_scale_factor,
                                    Some((monitor_position, monitor_size)),
                                )
                            })
                    })
                    .unwrap_or_else(|| {
                        (
                            caret_position.to_logical(1.0),
                            caret_size.to_logical(1.0),
                            false,
                            false,
                            1.0,
                            None,
                        )
                    });

                let overflows_window_below = platform_state.get_active_window().is_some_and(|window| {
                    window.rect.bottom(scale_factor) < max_height + caret_position.y + caret_size.height
                });

                let above = !overflows_monitor_above & (overflows_monitor_below | overflows_window_below);

                let mut x: f64 = caret_position.x + anchor.width;
                let mut y: f64 = match above {
                    true => caret_position.y - size.height - anchor.height,
                    false => caret_position.y + caret_size.height + anchor.height,
                };

                #[allow(clippy::all)]
                let clipped = if let Some((monitor_position, monitor_size)) = &monitor_frame {
                    let clipped = caret_position.x + size.width > monitor_position.x + monitor_size.width;

                    x = x
                        .min(monitor_position.x + monitor_size.width - size.width)
                        .max(monitor_position.x);
                    y = y
                        .min(monitor_position.y + monitor_size.height - size.height)
                        .max(monitor_position.y);

                    clipped
                } else {
                    false
                };

                (Position::Logical(LogicalPosition::new(x, y)), above, clipped)
            },
        };

        if !dry_run {
            match platform_state.position_window(&self.window, &self.window_id, position) {
                Ok(_) => {
                    tracing::trace!(window_id =% self.window_id, ?position, ?is_above, ?is_clipped, "updated window geometry: first set");
                },
                Err(err) => tracing::error!(%err, window_id =% self.window_id, "failed to position window"),
            }

            // Apply the diff to atomic state
            self.window.set_inner_size(size);

            match platform_state.position_window(&self.window, &self.window_id, position) {
                Ok(_) => {
                    tracing::trace!(window_id =% self.window_id,"updated window geometry: second set");
                },
                Err(err) => tracing::error!(%err, window_id =% self.window_id, "failed to position window"),
            }
        }

        (is_above, is_clipped)
    }

    #[allow(clippy::only_used_in_recursion)]
    #[allow(clippy::too_many_arguments)]
    #[allow(unused_variables)]
    pub fn handle(
        &self,
        event: WindowEvent,
        figterm_state: &FigtermState,
        platform_state: &PlatformState,
        notifications_state: &WebviewNotificationsState,
        window_target: &EventLoopWindowTarget,
        api_tx: &UnboundedSender<(WindowId, String)>,
    ) {
        match event {
            WindowEvent::UpdateWindowGeometry {
                position,
                size,
                anchor,
                dry_run,
                tx,
            } => {
                let (is_above, is_clipped) =
                    self.update_window_geometry(position, size, anchor, platform_state, dry_run);
                if let Some(tx) = tx {
                    if let Err(err) = tx.send(WindowGeometryResult { is_above, is_clipped }) {
                        tracing::error!(%err, "failed to send window geometry update result");
                    }
                }
            },
            WindowEvent::Hide => {
                if !self.window.is_visible() {
                    return;
                }
                self.window.set_visible(false);

                if self.window_id == AUTOCOMPLETE_ID {
                    for session in figterm_state.inner.lock().linked_sessions.values_mut() {
                        let _ = session
                            .sender
                            .send(FigtermCommand::InterceptFigJSVisible { visible: false });
                    }

                    // TODO: why are we setting then unsetting resizable?
                    #[cfg(not(target_os = "linux"))]
                    self.window.set_resizable(true);

                    #[cfg(not(target_os = "linux"))]
                    self.window.set_resizable(false);
                }

                #[cfg(target_os = "macos")]
                if self.window_id == DASHBOARD_ID {
                    use tao::platform::macos::{
                        ActivationPolicy,
                        EventLoopWindowTargetExtMacOS,
                    };

                    let mut policy_lock = platform::ACTIVATION_POLICY.lock().unwrap();
                    if *policy_lock != ActivationPolicy::Accessory {
                        *policy_lock = ActivationPolicy::Accessory;
                        window_target.set_activation_policy_at_runtime(ActivationPolicy::Accessory);
                    }
                }
            },
            WindowEvent::Show => {
                if self.window_id == AUTOCOMPLETE_ID {
                    if platform::autocomplete_active() {
                        for session in figterm_state.inner.lock().linked_sessions.values_mut() {
                            let _ = session
                                .sender
                                .send(FigtermCommand::InterceptFigJSVisible { visible: true });
                        }

                        self.window.set_visible(true);
                        cfg_if::cfg_if!(
                            if #[cfg(target_os = "macos")] {
                                // We handle setting window level on focus changed on MacOS
                                // TODO: pull this out into platform code.
                            } else if #[cfg(target_os = "windows")] {
                                self.window.set_always_on_top(false);
                            } else {
                                self.window.set_always_on_top(true);
                            }
                        );
                    }
                } else {
                    #[cfg(target_os = "macos")]
                    if self.window_id == DASHBOARD_ID {
                        use tao::platform::macos::{
                            ActivationPolicy,
                            EventLoopWindowTargetExtMacOS,
                        };

                        let mut policy_lock = platform::ACTIVATION_POLICY.lock().unwrap();
                        if *policy_lock != ActivationPolicy::Regular {
                            *policy_lock = ActivationPolicy::Regular;
                            window_target.set_activation_policy_at_runtime(ActivationPolicy::Regular);
                        }
                    }

                    self.window.set_visible(true);
                    self.window.set_focus();
                }
            },
            WindowEvent::NavigateAbsolute { url } => {
                self.webview
                    .evaluate_script(&format!("window.location.href = '{url}';"))
                    .unwrap();
                *self.url.lock() = url;
            },
            WindowEvent::NavigateRelative { path } => {
                let event_name = "dashboard.navigate";
                let payload = serde_json::json!({ "path": path });

                self.notification(notifications_state, &NotificationType::NotifyOnEvent, Notification {
                    r#type: Some(NotificationEnum::EventNotification(EventNotification {
                        event_name: Some(event_name.to_string()),
                        payload: Some(payload.to_string()),
                    })),
                });
            },
            WindowEvent::NavigateForward => {
                self.webview.evaluate_script("window.history.forward();").unwrap();
            },
            WindowEvent::NavigateBack => {
                self.webview.evaluate_script("window.history.back();").unwrap();
            },
            WindowEvent::Event { event_name, payload } => {
                self.notification(notifications_state, &NotificationType::NotifyOnEvent, Notification {
                    r#type: Some(NotificationEnum::EventNotification(EventNotification {
                        event_name: Some(event_name.into_owned()),
                        payload: payload.map(|s| s.into_owned()),
                    })),
                });
            },
            WindowEvent::Reload => {
                info!(%self.window_id, "Reloading window");

                let url = serde_json::json!(self.url.lock().clone());

                self.webview
                    .evaluate_script(&format!(
                        "if (window.location.href === 'about:blank') {{\
                            console.log('Reloading window to', {url});\
                            window.location.href = {url};\
                        }} else {{\
                            console.log('Reloading window');\
                            window.location.reload();\
                        }}"
                    ))
                    .unwrap();
            },
            WindowEvent::Emit { event_name, payload } => {
                self.emit(event_name, payload);
            },
            WindowEvent::Api { payload } => {
                api_tx.send((self.window_id.clone(), payload)).unwrap();
            },
            WindowEvent::Devtools => {
                if self.webview.is_devtools_open() {
                    self.webview.close_devtools();
                } else {
                    self.webview.open_devtools();
                }
            },
            WindowEvent::DebugMode(debug_mode) => {
                // Macos does not support setting the webview background color so we have
                // to set the css background root color to see the window
                cfg_if::cfg_if! {
                    if #[cfg(target_os = "macos")] {
                        self.webview
                            .evaluate_script(if debug_mode {
                                "document.querySelector(':root').style.setProperty('background-color', 'red');"
                            } else {
                                "document.querySelector(':root').style.removeProperty('background-color');"
                            })
                            .unwrap();
                    } else {
                        self.webview
                            .set_background_color(if debug_mode {
                                (0xff, 0, 0, 0xff)
                            } else {
                                (0, 0, 0, 0) }
                            ).unwrap();
                    }

                }
            },
            WindowEvent::SetEnabled(enabled) => self.set_enabled(enabled),
            WindowEvent::SetTheme(theme) => self.set_theme(theme),
            WindowEvent::Drag => {
                if let Err(err) = self.window.drag_window() {
                    error!(%err, "Failed to drag window");
                }
            },
            WindowEvent::Batch(events) => {
                for event in events {
                    self.handle(
                        event,
                        figterm_state,
                        platform_state,
                        notifications_state,
                        window_target,
                        api_tx,
                    );
                }
            },
        }
    }

    pub fn emit(&self, event_name: impl Display, payload: impl Into<serde_json::Value>) {
        let payload = payload.into();
        self.webview
            .evaluate_script(&format!(
                "document.dispatchEvent(new CustomEvent('{event_name}', {{'detail': {payload}}}));"
            ))
            .unwrap();
    }

    pub fn notification(
        &self,
        notifications_state: &WebviewNotificationsState,
        notification_type: &NotificationType,
        notification: Notification,
    ) {
        let window_id = &self.window_id;
        if let Some(notifications) = notifications_state.subscriptions.get(window_id) {
            if let Some(message_id) = notifications.get(notification_type) {
                let message = ServerOriginatedMessage {
                    id: Some(*message_id),
                    submessage: Some(ServerOriginatedSubMessage::Notification(notification)),
                };

                let mut encoded = BytesMut::new();

                match message.encode(&mut encoded) {
                    Ok(_) => {
                        debug!(?notification_type, %window_id, "Sending notification");
                        self.emit(EmitEventName::ProtoMessageReceived, BASE64_STANDARD.encode(encoded));
                    },
                    Err(err) => error!(%err, "Failed to encode notification"),
                }
            } else {
                warn!(?notification_type, %window_id, "No subscription for notification type");
            }
        } else {
            warn!(?notification_type, %window_id, "No subscriptions for window");
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.webview
            .evaluate_script(format!("window.fig.enabled = {enabled};").as_str())
            .unwrap();
        self.enabled.store(enabled, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn set_theme(&self, theme: Option<Theme>) {
        self.window.set_theme(theme.and_then(to_tao_theme));
    }
}
