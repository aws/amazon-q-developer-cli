use wry::application::event_loop::ControlFlow;

use crate::window::WindowId;

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub enum Event {
    WindowEvent {
        window_id: WindowId,
        window_event: WindowEvent,
    },
    ControlFlow(ControlFlow),
    RefreshDebugger,
    NativeEvent(NativeEvent),
}

#[derive(Debug)]
pub enum WindowEvent {
    Reanchor {
        x: i32,
        y: i32,
    },
    Reposition {
        x: i32,
        y: i32,
    },
    Resize {
        width: u32,
        height: u32,
    },
    /// Hides the window
    Hide,
    /// Request to hide the window, may not be respected
    HideSoft,
    Show,
    Emit {
        event: String,
        payload: String,
    },
    Navigate {
        url: url::Url,
    },
    Api {
        payload: String,
    },
    Devtools,
}

#[derive(Debug)]
pub enum NativeEvent {
    #[cfg(target_os = "windows")]
    EditBufferChanged,
}
