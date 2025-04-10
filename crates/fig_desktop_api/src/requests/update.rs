use fig_install::UpdateOptions;
use fig_os_shim::Context;
use fig_proto::fig::{
    CheckForUpdatesRequest,
    CheckForUpdatesResponse,
    UpdateApplicationRequest,
};

use super::{
    RequestResult,
    RequestResultImpl,
    ServerOriginatedSubMessage,
};

pub async fn update_application(request: UpdateApplicationRequest) -> RequestResult {
    tokio::spawn(fig_install::update(
        Context::new(),
        Some(Box::new(|_| {})),
        UpdateOptions {
            ignore_rollout: request.ignore_rollout.unwrap_or(true),
            interactive: request.interactive.unwrap_or(true),
            relaunch_dashboard: request.relaunch_dashboard.unwrap_or(true),
        },
    ));
    RequestResult::success()
}

pub async fn check_for_updates(_request: CheckForUpdatesRequest) -> RequestResult {
    fig_install::check_for_updates(true)
        .await
        .map(|res| {
            Box::new(ServerOriginatedSubMessage::CheckForUpdatesResponse(
                CheckForUpdatesResponse {
                    is_update_available: Some(res.is_some()),
                    version: res.map(|update| update.version.to_string()),
                },
            ))
        })
        .map_err(|err| format!("Failed to check for updates: {err}").into())
}
