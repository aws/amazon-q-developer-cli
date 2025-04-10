use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    DragWindowRequest,
    FocusAction,
    PositionWindowRequest,
    PositionWindowResponse,
    WindowFocusRequest,
};
use fig_remote_ipc::figterm::FigtermState;
use tao::dpi::LogicalSize;
use tracing::debug;

use super::{
    RequestResult,
    RequestResultImpl,
};
use crate::event::{
    Event,
    WindowEvent,
    WindowGeometryResult,
};
use crate::webview::WindowId;
use crate::{
    AUTOCOMPLETE_ID,
    EventLoopProxy,
};

pub async fn position_window(
    request: PositionWindowRequest,
    window_id: WindowId,
    figterm_state: &FigtermState,
    proxy: &EventLoopProxy,
) -> RequestResult {
    debug!(?request, %window_id, "Position Window Request");

    let size = request.size.as_ref().ok_or("PositionWindowRequest must have a size")?;
    let is_hide = size.width == 1.0 || size.height == 1.0;

    if window_id == AUTOCOMPLETE_ID
        && !is_hide
        && figterm_state
            .most_recent()
            .and_then(|session| session.context.as_ref().map(|context| context.preexec()))
            .unwrap_or(false)
    {
        return Err("Cannot position autocomplete window while preexec is active".into());
    }

    let dry_run = request.dryrun.unwrap_or(false);
    let anchor = request
        .anchor
        .as_ref()
        .ok_or("PositionWindowRequest must have an anchor")?;
    let autocomplete_padding = 5.0;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let mut events = vec![WindowEvent::UpdateWindowGeometry {
        position: None,
        size: Some(LogicalSize::new(size.width.into(), size.height.into())),
        anchor: Some(LogicalSize::new(
            anchor.x.into(),
            (anchor.y + autocomplete_padding).into(),
        )),
        tx: Some(tx),
        dry_run,
    }];

    if !dry_run {
        events.push(
            // Workaround to nonapplicably zero sized windows
            if is_hide { WindowEvent::Hide } else { WindowEvent::Show },
        );
    }

    proxy
        .send_event(Event::WindowEvent {
            window_id,
            window_event: WindowEvent::Batch(events),
        })
        .unwrap();

    match rx.recv().await {
        Some(WindowGeometryResult { is_above, is_clipped }) => RequestResult::Ok(Box::new(
            ServerOriginatedSubMessage::PositionWindowResponse(PositionWindowResponse {
                is_above: Some(is_above),
                is_clipped: Some(is_clipped),
            }),
        )),
        None => Err("unable to determine is_above and is_clipped".into()),
    }
}

pub async fn focus(request: WindowFocusRequest, window_id: WindowId, proxy: &EventLoopProxy) -> RequestResult {
    debug!(?request, %window_id, "Window Focus Request");
    match request.r#type() {
        FocusAction::TakeFocus => {
            proxy
                .send_event(Event::WindowEvent {
                    window_id,
                    window_event: WindowEvent::Show,
                })
                .unwrap();
        },
        FocusAction::ReturnFocus => return Err("ReturnFocus not implemented".into()),
    }

    RequestResult::success()
}

pub async fn drag(request: DragWindowRequest, window_id: WindowId, proxy: &EventLoopProxy) -> RequestResult {
    debug!(?request, %window_id, "Window Drag Request");
    proxy
        .send_event(Event::WindowEvent {
            window_id,
            window_event: WindowEvent::Drag,
        })
        .unwrap();

    RequestResult::success()
}
