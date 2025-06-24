use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::Result;
use dbus::connect_to_ibus_daemon;
use fig_os_shim::Context;
use fig_proto::local::caret_position_hook::Origin;
use fig_util::terminal::PositioningKind;
use tao::dpi::{
    LogicalPosition,
    LogicalSize,
    PhysicalPosition,
    PhysicalSize,
    Position,
    Size,
};
use tracing::{
    debug,
    error,
};
use zbus::export::futures_util::TryStreamExt;
use zbus::fdo::DBusProxy;
use zbus::{
    MatchRule,
    MessageStream,
};

use super::PlatformStateImpl;
use crate::event::{
    Event,
    WindowEvent,
    WindowPosition,
};
use crate::platform::ActiveWindowData;
use crate::{
    AUTOCOMPLETE_ID,
    EventLoopProxy,
};

/// Connects to the `ibus-daemon` D-Bus service if not already connected, launching a task to
/// continually listen on InputContext signals.
pub async fn launch_ibus_connection(proxy: EventLoopProxy, platform_state: Arc<PlatformStateImpl>) -> Result<()> {
    if platform_state.ibus_connected.load(Ordering::SeqCst) {
        return Ok(());
    }

    let ibus_connection = connect_to_ibus_daemon(&Context::new()).await?;
    debug!("Connected to ibus: {:?}", ibus_connection);
    platform_state.ibus_connected.store(true, Ordering::SeqCst);
    DBusProxy::new(&ibus_connection)
        .await?
        .add_match_rule(
            MatchRule::builder()
                .interface("org.freedesktop.IBus.InputContext")?
                .build(),
        )
        .await?;
    debug!("Added eavesdrop to ibus proxy");
    let mut stream = MessageStream::from(ibus_connection);
    tokio::spawn(async move {
        // TODO: wezterm only emits "FocusIn" on the first launch on X11, test if it's safe for the
        // logic to be updated to not require focus in/out events.
        let mut active_input_contexts = HashSet::new();
        loop {
            match stream.try_next().await {
                Ok(Some(msg)) => {
                    let header = msg.header();
                    if let (Some(member), Some(interface), Some(path)) =
                        (header.member(), header.interface(), header.path())
                    {
                        if interface.as_str() == "org.freedesktop.IBus.InputContext" {
                            match member.as_str() {
                                "FocusIn" => {
                                    debug!("FocusIn on {}", path.as_str());
                                    active_input_contexts.insert(path.as_str().to_owned());
                                },
                                "FocusOut" => {
                                    debug!("FocusOut on {}", path.as_str());
                                    active_input_contexts.remove(path.as_str());
                                },
                                "SetCursorLocation" => {
                                    if !active_input_contexts.contains(path.as_str())
                                        || platform_state.active_terminal.lock().is_none()
                                    {
                                        debug!("SetCursorLocation rejected on {}", path.as_str());
                                        continue;
                                    }

                                    let body = match msg.body().deserialize::<(i32, i32, i32, i32)>() {
                                        Ok(body) => body,
                                        Err(err) => {
                                            error!(%err, "Failed deserializing message body");
                                            continue;
                                        },
                                    };
                                    if body == (0, 0, 0, 0) {
                                        debug!("null SetCursorLocation on {}", path.as_str());
                                    } else {
                                        debug!(
                                            "SetCursorLocation{{x: {}, y: {}}} on {}",
                                            body.0,
                                            body.1,
                                            path.as_str()
                                        );
                                        let positioning_kind = platform_state
                                            .active_terminal
                                            .lock()
                                            .as_ref()
                                            .map_or(PositioningKind::Physical, |x| x.positioning_kind());

                                        let (caret_position, caret_size): (Position, Size) = match positioning_kind {
                                            PositioningKind::Logical => (
                                                LogicalPosition::new(body.0 as f64, body.1 as f64).into(),
                                                LogicalSize::new(body.2 as f64, body.3 as f64).into(),
                                            ),
                                            PositioningKind::Physical => (
                                                PhysicalPosition::new(body.0, body.1).into(),
                                                PhysicalSize::new(body.2, body.3).into(),
                                            ),
                                        };

                                        proxy
                                            .send_event(Event::WindowEvent {
                                                window_id: AUTOCOMPLETE_ID.clone(),
                                                window_event: WindowEvent::UpdateWindowGeometry {
                                                    position: Some(WindowPosition::RelativeToCaret {
                                                        caret_position,
                                                        caret_size,
                                                        origin: Origin::TopLeft,
                                                    }),
                                                    size: None,
                                                    anchor: None,
                                                    tx: None,
                                                    dry_run: false,
                                                },
                                            })
                                            .unwrap();
                                    }
                                },
                                "SetCursorLocationRelative" => {
                                    if !active_input_contexts.contains(path.as_str()) {
                                        debug!("SetCursorLocationRelative rejected on {}", path.as_str());
                                        continue;
                                    }
                                    let body = match msg.body().deserialize::<(i32, i32, i32, i32)>() {
                                        Ok(body) => body,
                                        Err(err) => {
                                            error!(%err, "Failed deserializing message body");
                                            continue;
                                        },
                                    };
                                    debug!(
                                        "SetCursorLocationRelative{{x: {}, y: {}, h: {}}} on {}",
                                        body.0,
                                        body.1,
                                        body.3,
                                        path.as_str()
                                    );
                                    let abs: (i32, i32) = {
                                        let handle = platform_state.active_window_data.lock();
                                        match *handle {
                                            Some(ActiveWindowData {
                                                outer_x,
                                                outer_y,
                                                scale,
                                                ..
                                            }) => (
                                                (body.0 as f32 / scale).round() as i32 + outer_x,
                                                (body.1 as f32 / scale).round() as i32 + outer_y
                                                    - (body.3 as f32 / scale).round() as i32,
                                            ),
                                            None => continue,
                                        }
                                    };
                                    debug!("resolved cursor to {{x: {}, y: {}}}", abs.0, abs.1,);
                                    proxy
                                        .send_event(Event::WindowEvent {
                                            window_id: AUTOCOMPLETE_ID.clone(),
                                            window_event: WindowEvent::UpdateWindowGeometry {
                                                position: Some(WindowPosition::RelativeToCaret {
                                                    caret_position: LogicalPosition {
                                                        x: abs.0 as f64,
                                                        y: abs.1 as f64,
                                                    }
                                                    .into(),
                                                    caret_size: LogicalSize {
                                                        width: body.2 as f64,
                                                        height: body.3 as f64,
                                                    }
                                                    .into(),
                                                    origin: Origin::TopLeft,
                                                }),
                                                size: None,
                                                anchor: None,
                                                tx: None,
                                                dry_run: false,
                                            },
                                        })
                                        .unwrap();
                                },
                                _ => {},
                            }
                        }
                    }
                },
                Ok(None) => {
                    debug!("Received end from ibus");
                    platform_state.ibus_connected.store(false, Ordering::SeqCst);
                    break;
                },
                Err(err) => {
                    error!(%err, "Failed receiving message from stream");
                    platform_state.ibus_connected.store(false, Ordering::SeqCst);
                    break;
                },
            }
        }
    });

    Ok(())
}
