use std::sync::LazyLock;

use dashmap::DashMap;
use fnv::FnvBuildHasher;
use serde_json::Value;
use tokio::sync::broadcast::{
    self,
    Receiver,
    Sender,
};

const CHANNEL_SIZE: usize = 8;

pub static NOTIFICATION_BUS: LazyLock<NotificationBus> = LazyLock::new(NotificationBus::new);

#[derive(Debug, Clone)]
pub enum JsonNotification {
    NewValue { new: Value },
    ChangedValue { old: Value, new: Value },
    RemoveValue { old: Value },
}

impl JsonNotification {
    pub fn value(self) -> Option<Value> {
        match self {
            JsonNotification::NewValue { new } => Some(new),
            JsonNotification::ChangedValue { new, .. } => Some(new),
            JsonNotification::RemoveValue { .. } => None,
        }
    }

    pub fn as_bool(self) -> Option<bool> {
        self.value().and_then(|value| value.as_bool())
    }

    pub fn as_string(self) -> Option<String> {
        self.value().and_then(|value| value.as_str().map(|s| s.into()))
    }
}

#[derive(Debug)]
pub struct NotificationBus {
    state_channels: DashMap<String, Sender<JsonNotification>, FnvBuildHasher>,
    settings_channels: DashMap<String, Sender<JsonNotification>, FnvBuildHasher>,
    midway_channel: Sender<()>,
}

impl std::default::Default for NotificationBus {
    fn default() -> Self {
        Self {
            state_channels: DashMap::default(),
            settings_channels: DashMap::default(),
            midway_channel: broadcast::channel(CHANNEL_SIZE).0,
        }
    }
}

impl NotificationBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe_state(&self, key: String) -> Receiver<JsonNotification> {
        self.state_channels
            .entry(key)
            .or_insert_with(|| {
                let (tx, _) = tokio::sync::broadcast::channel(CHANNEL_SIZE);
                tx
            })
            .subscribe()
    }

    pub fn subscribe_settings(&self, key: String) -> Receiver<JsonNotification> {
        self.settings_channels
            .entry(key)
            .or_insert_with(|| {
                let (tx, _) = tokio::sync::broadcast::channel(CHANNEL_SIZE);
                tx
            })
            .subscribe()
    }

    pub fn subscribe_midway(&self) -> Receiver<()> {
        self.midway_channel.subscribe()
    }

    pub fn send_state(&self, key: impl AsRef<str>, value: JsonNotification) {
        if let Some(tx) = self.state_channels.get(key.as_ref()) {
            tx.send(value).ok();
        }
    }

    pub fn send_settings(&self, key: impl AsRef<str>, value: JsonNotification) {
        if let Some(tx) = self.settings_channels.get(key.as_ref()) {
            tx.send(value).ok();
        }
    }

    pub fn send_midway(&self) {
        self.midway_channel.send(()).ok();
    }

    pub fn send_state_new(&self, key: impl AsRef<str>, value: &Value) {
        if let Some(tx) = self.state_channels.get(key.as_ref()) {
            tx.send(JsonNotification::NewValue { new: value.clone() }).ok();
        }
    }

    pub fn send_state_remove(&self, key: impl AsRef<str>, value: &Value) {
        if let Some(tx) = self.state_channels.get(key.as_ref()) {
            tx.send(JsonNotification::RemoveValue { old: value.clone() }).ok();
        }
    }

    pub fn send_state_changed(&self, key: impl AsRef<str>, old: &Value, new: &Value) {
        if let Some(tx) = self.state_channels.get(key.as_ref()) {
            tx.send(JsonNotification::ChangedValue {
                old: old.clone(),
                new: new.clone(),
            })
            .ok();
        }
    }

    pub fn send_settings_new(&self, key: impl AsRef<str>, value: &Value) {
        if let Some(tx) = self.settings_channels.get(key.as_ref()) {
            tx.send(JsonNotification::NewValue { new: value.clone() }).ok();
        }
    }

    pub fn send_settings_remove(&self, key: impl AsRef<str>, value: &Value) {
        if let Some(tx) = self.settings_channels.get(key.as_ref()) {
            tx.send(JsonNotification::RemoveValue { old: value.clone() }).ok();
        }
    }

    pub fn send_settings_changed(&self, key: impl AsRef<str>, old: &Value, new: &Value) {
        if let Some(tx) = self.settings_channels.get(key.as_ref()) {
            tx.send(JsonNotification::ChangedValue {
                old: old.clone(),
                new: new.clone(),
            })
            .ok();
        }
    }
}
