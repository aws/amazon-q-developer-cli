use anyhow::Result;
use fig_proto::local::{
    CaretPositionHook,
    ClearAutocompleteCacheHook,
    EventHook,
    FileChangedHook,
    FocusedWindowDataHook,
};
use tao::dpi::{
    LogicalPosition,
    LogicalSize,
};

use crate::event::{
    WindowEvent,
    WindowPosition,
};
use crate::platform::PlatformState;
use crate::webview::WindowId;
use crate::{
    AUTOCOMPLETE_ID,
    Event,
    EventLoopProxy,
};

pub async fn caret_position(
    hook @ CaretPositionHook {
        x, y, width, height, ..
    }: CaretPositionHook,
    proxy: &EventLoopProxy,
) -> Result<()> {
    proxy
        .send_event(Event::WindowEvent {
            window_id: AUTOCOMPLETE_ID,
            window_event: WindowEvent::UpdateWindowGeometry {
                position: Some(WindowPosition::RelativeToCaret {
                    caret_position: LogicalPosition::new(x, y).into(),
                    caret_size: LogicalSize::new(width, height).into(),
                    origin: hook.origin(),
                }),
                size: None,
                anchor: None,
                tx: None,
                dry_run: false,
            },
        })
        .ok();

    Ok(())
}

pub async fn focus_change(proxy: &EventLoopProxy) -> Result<()> {
    proxy
        .send_event(Event::WindowEvent {
            window_id: AUTOCOMPLETE_ID.clone(),
            window_event: WindowEvent::Hide,
        })
        .unwrap();

    Ok(())
}

pub async fn file_changed(_file_changed_hook: FileChangedHook) -> Result<()> {
    Ok(())
}

pub async fn focused_window_data(
    hook: FocusedWindowDataHook,
    platform_state: &PlatformState,
    proxy: &EventLoopProxy,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    return crate::platform::integrations::from_hook(hook, platform_state, proxy);
    #[cfg(not(target_os = "linux"))]
    {
        let (_hook, _platform_state, _proxy) = (hook, platform_state, proxy);
        Ok(())
    }
}

pub async fn event(hook: EventHook, proxy: &EventLoopProxy) -> Result<()> {
    let window_event = WindowEvent::Event {
        event_name: hook.event_name.into(),
        payload: hook.payload.map(|s| s.into()),
    };

    if hook.apps.is_empty() {
        proxy.send_event(Event::WindowEventAll { window_event }).unwrap();
    } else {
        for app in hook.apps {
            proxy
                .send_event(Event::WindowEvent {
                    window_id: WindowId(app.into()),
                    window_event: window_event.clone(),
                })
                .unwrap();
        }
    }

    Ok(())
}

pub async fn clear_autocomplete_cache(hook: ClearAutocompleteCacheHook, proxy: &EventLoopProxy) -> Result<()> {
    proxy.send_event(Event::WindowEvent {
        window_id: AUTOCOMPLETE_ID,
        window_event: WindowEvent::Event {
            event_name: "clear-cache".into(),
            payload: Some(serde_json::to_string(&hook.clis)?.into()),
        },
    })?;

    Ok(())
}
