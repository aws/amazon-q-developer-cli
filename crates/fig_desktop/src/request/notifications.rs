use fig_proto::fig::{
    NotificationRequest,
    NotificationType,
};

use super::{
    RequestResult,
    RequestResultImpl,
};
use crate::WebviewNotificationsState;
use crate::webview::WindowId;

pub async fn handle_request(
    request: NotificationRequest,
    window_id: WindowId,
    message_id: i64,
    state: &WebviewNotificationsState,
) -> RequestResult {
    let notification_type = NotificationType::try_from(request.r#type.unwrap()).unwrap();

    if request.subscribe.unwrap_or(true) {
        subscribe(window_id, message_id, notification_type, state)
    } else {
        unsubscribe(&window_id, notification_type, state)
    }
}

fn subscribe(
    window_id: WindowId,
    channel: i64,
    notification_type: NotificationType,
    state: &WebviewNotificationsState,
) -> RequestResult {
    if notification_type == NotificationType::All {
        return RequestResult::error("Cannot subscribe to All notification type");
    }

    let entry = state.subscriptions.entry(window_id).or_default();
    if entry.contains_key(&notification_type) {
        return RequestResult::error(format!("Already subscribed to notification type {notification_type:?}",));
    }

    entry.insert(notification_type, channel);

    RequestResult::success()
}

fn unsubscribe(
    window_id: &WindowId,
    notification_type: NotificationType,
    state: &WebviewNotificationsState,
) -> RequestResult {
    if notification_type == NotificationType::All {
        return unsubscribe_all(window_id, state);
    }

    match state.subscriptions.get(window_id) {
        Some(subscriptions) if !subscriptions.contains_key(&notification_type) => {
            return RequestResult::error(format!("Not subscribed notification type {notification_type:?}",));
        },
        None => {
            return RequestResult::error(format!("Not subscribed notification type {notification_type:?}",));
        },
        Some(subscriptions) => {
            subscriptions.remove(&notification_type);
        },
    }

    RequestResult::success()
}

fn unsubscribe_all(window_id: &WindowId, state: &WebviewNotificationsState) -> RequestResult {
    if let Some(subscriptions) = state.subscriptions.get(window_id) {
        subscriptions.clear();
    }

    RequestResult::success()
}
