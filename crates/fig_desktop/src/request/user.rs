use fig_desktop_api::requests::{
    RequestResult,
    RequestResultImpl,
};
use fig_proto::fig::UserLogoutRequest;
use tracing::error;

use crate::event::{
    Event,
    WindowEvent,
};
use crate::webview::LOGIN_PATH;
use crate::{
    DASHBOARD_ID,
    EventLoopProxy,
};

pub async fn logout(_request: UserLogoutRequest, proxy: &EventLoopProxy) -> RequestResult {
    fig_auth::logout().await.ok();

    proxy
        .send_event(Event::WindowEvent {
            window_id: DASHBOARD_ID,
            window_event: WindowEvent::Batch(vec![
                WindowEvent::NavigateRelative {
                    path: LOGIN_PATH.into(),
                },
                WindowEvent::Show,
            ]),
        })
        .map_err(|err| error!(?err))
        .ok();

    proxy
        .send_event(Event::ReloadTray { is_logged_in: false })
        .map_err(|err| error!(?err))
        .ok();

    RequestResult::success()
}
