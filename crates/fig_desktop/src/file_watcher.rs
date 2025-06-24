use std::sync::Arc;

use fig_proto::fig::notification::Type as NotificationEnum;
use fig_proto::fig::{
    NotificationType,
    SettingsChangedNotification,
};
use fig_settings::JsonStore;
use fig_util::directories;
use notify::event::ModifyKind;
use notify::{
    EventKind,
    RecursiveMode,
    Watcher,
};
use serde_json::{
    Map,
    Value,
};
use tracing::{
    debug,
    error,
    trace,
};

use crate::EventLoopProxy;
use crate::notification_bus::NOTIFICATION_BUS;
use crate::webview::notification::WebviewNotificationsState;

pub async fn setup_listeners(notifications_state: Arc<WebviewNotificationsState>, proxy: EventLoopProxy) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let mut watcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => {
            if let Err(err) = tx.send(event) {
                error!(%err, "failed to send notify event");
            }
        },
        Err(err) => error!(%err, "notify watcher"),
    })
    .unwrap();

    let settings_path = match directories::settings_path() {
        Ok(settings_path) => match settings_path.parent() {
            Some(settings_dir) => match watcher.watch(settings_dir, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    trace!("watching settings file at {settings_dir:?}");
                    Some(settings_path)
                },
                Err(err) => {
                    error!(%err, "failed to watch settings dir");
                    None
                },
            },
            None => {
                error!("failed to get settings file dir");
                None
            },
        },
        Err(err) => {
            error!(%err, "failed to get settings file path");
            None
        },
    };

    let midway_path = match directories::midway_cookie_path() {
        Ok(macos_utils) => match macos_utils.parent() {
            Some(midway_dir) => match watcher.watch(midway_dir, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    trace!("watching midway file at {midway_dir:?}");
                    Some(macos_utils)
                },
                Err(err) => {
                    error!(%err, "failed to watch midway dir");
                    None
                },
            },
            None => {
                error!("failed to get midway file dir");
                None
            },
        },
        Err(err) => {
            error!(%err, "failed to get midway file path");
            None
        },
    };

    tokio::spawn(async move {
        let _watcher = watcher;

        let mut prev_settings = match fig_settings::OldSettings::load_from_file() {
            Ok(map) => map,
            Err(err) => {
                error!(?err, "failed to initialize settings");
                Map::new()
            },
        };

        #[cfg(target_os = "linux")]
        {
            use crate::Event;
            use crate::event::WindowEvent;
            use crate::webview::AUTOCOMPLETE_ID;
            proxy
                .send_event(Event::WindowEvent {
                    window_id: AUTOCOMPLETE_ID,
                    window_event: WindowEvent::SetEnabled(
                        !prev_settings
                            .get("autocomplete.disable")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    ),
                })
                .map_err(|err| error!(?err, "failed initializing autocomplete.disable state"))
                .ok();
        }

        while let Some(event) = rx.recv().await {
            trace!(?event, "Settings event");

            if let Some(settings_path) = &settings_path {
                if event.paths.contains(settings_path) {
                    if let EventKind::Create(_) | EventKind::Modify(_) = event.kind {
                        match fig_settings::OldSettings::load_from_file() {
                            Ok(settings) => {
                                debug!("Settings file changed");

                                notifications_state
                                    .broadcast_notification_all(
                                        &NotificationType::NotifyOnSettingsChange,
                                        fig_proto::fig::Notification {
                                            r#type: Some(NotificationEnum::SettingsChangedNotification(
                                                SettingsChangedNotification {
                                                    json_blob: serde_json::to_string(&settings).ok(),
                                                },
                                            )),
                                        },
                                        &proxy,
                                    )
                                    .await
                                    .unwrap();

                                json_map_diff(
                                    &prev_settings,
                                    &settings,
                                    |key, value| {
                                        debug!(%key, %value, "Setting added");
                                        NOTIFICATION_BUS.send_settings_new(key, value);
                                    },
                                    |key, old, new| {
                                        debug!(%key, %old, %new, "Setting change");
                                        NOTIFICATION_BUS.send_settings_changed(key, old, new);
                                    },
                                    |key, value| {
                                        debug!(%key, %value, "Setting removed");
                                        NOTIFICATION_BUS.send_settings_remove(key, value);
                                    },
                                );

                                prev_settings = settings;
                            },
                            Err(err) => error!(%err, "Failed to get settings"),
                        }
                    }
                }
            }

            if let Some(midway_path) = &midway_path {
                if event.paths.contains(midway_path)
                    && matches!(
                        event.kind,
                        EventKind::Create(_)
                            | EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Name(_))
                    )
                {
                    debug!("Midway file changed");
                    NOTIFICATION_BUS.send_midway();
                }
            }
        }
    });
}

// Diffs the old and new settings and calls the appropriate callbacks
fn json_map_diff(
    map_a: &Map<String, Value>,
    map_b: &Map<String, Value>,
    on_new: impl Fn(&str, &Value),
    on_changed: impl Fn(&str, &Value, &Value),
    on_removed: impl Fn(&str, &Value),
) {
    for (key, value) in map_a {
        if let Some(other_value) = map_b.get(key) {
            if value != other_value {
                on_changed(key, value, other_value);
            }
        } else {
            on_removed(key, value);
        }
    }

    for (key, value) in map_b {
        if !map_a.contains_key(key) {
            on_new(key, value);
        }
    }
}
