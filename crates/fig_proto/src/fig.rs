pub use crate::proto::fig::*;

mod internal {
    use std::fmt::Display;

    use crate::proto::fig::result::Result as FigResultEnum;
    use crate::proto::fig::{
        NotificationType,
        Result as FigResult,
    };

    impl serde::Serialize for NotificationType {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_str(match self {
                NotificationType::All => "all",
                NotificationType::NotifyOnEditbuffferChange => "editbuffer_change",
                NotificationType::NotifyOnSettingsChange => "settings_change",
                NotificationType::NotifyOnPrompt => "prompt",
                NotificationType::NotifyOnLocationChange => "location_change",
                NotificationType::NotifyOnProcessChanged => "process_change",
                NotificationType::NotifyOnKeybindingPressed => "keybinding_pressed",
                NotificationType::NotifyOnFocusChanged => "focus_change",
                NotificationType::NotifyOnHistoryUpdated => "history_update",
                NotificationType::NotifyOnApplicationUpdateAvailable => "application_update_available",
                NotificationType::NotifyOnLocalStateChanged => "local_state_change",
                NotificationType::NotifyOnEvent => "event",
                NotificationType::NotifyOnAccessibilityChange => "accessibility_change",
            })
        }
    }

    impl<E> From<Result<(), E>> for FigResult
    where
        E: Display,
    {
        fn from(value: Result<(), E>) -> Self {
            match value {
                Ok(()) => FigResult {
                    result: FigResultEnum::Ok.into(),
                    error: None,
                },
                Err(e) => FigResult {
                    result: FigResultEnum::Error.into(),
                    error: Some(e.to_string()),
                },
            }
        }
    }
}
