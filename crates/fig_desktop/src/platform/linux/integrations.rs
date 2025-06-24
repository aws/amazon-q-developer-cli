use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::Ordering;

use anyhow::{
    Result,
    anyhow,
};
use fig_proto::local::FocusedWindowDataHook;
use fig_util::Terminal;
use tracing::debug;

use super::WM_REVICED_DATA;
use crate::event::{
    Event,
    WindowEvent,
};
use crate::platform::{
    ActiveWindowData,
    PlatformState,
};
use crate::{
    AUTOCOMPLETE_ID,
    EventLoopProxy,
};

pub static WM_CLASS_ALLOWLIST: LazyLock<HashMap<&'static str, Terminal>> = LazyLock::new(|| {
    let mut allowlist = HashMap::new();
    for terminal in fig_util::terminal::LINUX_TERMINALS {
        if let Some(wm_class) = terminal.wm_class() {
            allowlist.insert(wm_class, terminal.clone());
        }
    }
    allowlist
});

pub static GSE_ALLOWLIST: LazyLock<HashMap<&'static str, Terminal>> = LazyLock::new(|| {
    let mut allowlist = HashMap::new();
    for terminal in fig_util::terminal::LINUX_TERMINALS {
        // Using wm_class_instance here since on Wayland, (most?) terminals set the app_id equal to
        // the WM_CLASS Instance part. To handle Xwayland terminals, we still want to include wm_class as
        // well.
        if let (Some(instance), Some(class)) = (terminal.wm_class_instance(), terminal.wm_class()) {
            allowlist.insert(instance, terminal.clone());
            if class != instance {
                allowlist.insert(class, terminal.clone());
            }
        }
    }
    allowlist
});

fn from_source(from: &str) -> Option<&HashMap<&'static str, Terminal>> {
    match from {
        "wm_class" => Some(&WM_CLASS_ALLOWLIST),
        "gse" => Some(&GSE_ALLOWLIST),
        _ => None,
    }
}

pub fn from_hook(hook: FocusedWindowDataHook, platform_state: &PlatformState, proxy: &EventLoopProxy) -> Result<()> {
    debug!("Received FocusedWindowDataHook: {:?}", hook);
    WM_REVICED_DATA.store(true, Ordering::Relaxed);

    if hook.hide() {
        proxy.send_event(Event::WindowEvent {
            window_id: AUTOCOMPLETE_ID,
            window_event: WindowEvent::Hide,
        })?;
        return Ok(());
    }

    debug!("focus event on {} from {}", hook.id, hook.source);
    if let Some(terminal) = from_source(&hook.source)
        .ok_or_else(|| anyhow!("received invalid focus window data source"))?
        .get(hook.id.as_str())
    {
        *platform_state.0.active_terminal.lock() = Some(terminal.clone());
        let inner = hook.inner.unwrap();
        let outer = hook.outer.unwrap();
        let mut handle = platform_state.0.active_window_data.lock();
        *handle = Some(ActiveWindowData {
            inner_x: inner.x,
            inner_y: inner.y,
            inner_width: inner.width,
            inner_height: inner.height,
            outer_x: outer.x,
            outer_y: outer.y,
            outer_width: outer.width,
            outer_height: outer.height,
            scale: hook.scale,
        });
    } else {
        *platform_state.0.active_terminal.lock() = None;
        proxy.send_event(Event::WindowEvent {
            window_id: AUTOCOMPLETE_ID,
            window_event: WindowEvent::Hide,
        })?;
    }

    Ok(())
}
