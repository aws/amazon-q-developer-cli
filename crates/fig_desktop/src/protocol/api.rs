use std::borrow::Cow;
use std::sync::Arc;

use bytes::Bytes;
use fig_os_shim::Context as OsContext;
use fig_proto::fig::ClientOriginatedMessage;
use fig_proto::prost::Message;
use fig_settings::{
    Settings,
    State,
};
use wry::http::header::CONTENT_TYPE;
use wry::http::{
    HeaderValue,
    Method,
    Request,
    Response,
    StatusCode,
};

use crate::request::{
    Context,
    EventHandler,
};
use crate::webview::{
    DASH_KV_STORE,
    FIGTERM_STATE,
    GLOBAL_PROXY,
    INTERCEPT_STATE,
    NOTIFICATIONS_STATE,
    WindowId,
};

static APPLICATION_FIG_API: HeaderValue = HeaderValue::from_static("application/fig-api");

pub async fn handle(
    _ctx: Arc<OsContext>,
    request: Request<Vec<u8>>,
    window_id: WindowId,
) -> anyhow::Result<Response<Cow<'static, [u8]>>> {
    if request.method() != Method::POST {
        let body = format!("Unsupported method: {}", request.method());
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Cow::Owned(body.into()))?);
    }

    let unsupported_media_body = match request.headers().get(CONTENT_TYPE) {
        Some(val) if val == APPLICATION_FIG_API => None,
        Some(val) => Some(format!("Unsupported content type: {val:?}")),
        None => Some("Missing content type".into()),
    };

    if let Some(body) = unsupported_media_body {
        return Ok(Response::builder()
            .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            .body(Cow::Owned(body.into()))?);
    }

    let client_message = ClientOriginatedMessage::decode(Bytes::from(request.into_body()))?;
    let server_message = fig_desktop_api::handler::api_request(
        EventHandler::default(),
        Context {
            window_id: &window_id,
            figterm_state: FIGTERM_STATE.get().unwrap().as_ref(),
            intercept_state: INTERCEPT_STATE.get().unwrap().as_ref(),
            notifications_state: NOTIFICATIONS_STATE.get().unwrap().as_ref(),
            proxy: GLOBAL_PROXY.get().unwrap(),
            dash_kv_store: DASH_KV_STORE.get().unwrap().as_ref(),
            settings: &Settings::new(),
            state: &State::new(),
            ctx: fig_os_shim::Context::new(),
        },
        client_message,
    )
    .await?;

    let body = server_message.encode_to_vec().into();

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, &APPLICATION_FIG_API)
        .body(body)?)
}

#[cfg(test)]
mod tests {
    use fig_desktop_api::handler::ClientOriginatedSubMessage;
    use fig_proto::fig::{
        ClientOriginatedMessage,
        PingRequest,
    };

    use super::*;
    use crate::AUTOCOMPLETE_ID;

    // TODO: add success case, this fails currently due to the
    // globals (FIGTERM_STATE, INTERCEPT_STATE, etc) not being set in tests

    #[tokio::test]
    async fn test_handle_errors() {
        let ctx = OsContext::new_fake();
        let id = AUTOCOMPLETE_ID;
        let body = ClientOriginatedMessage {
            id: Some(0),
            submessage: Some(ClientOriginatedSubMessage::PingRequest(PingRequest {})),
        }
        .encode_to_vec();

        let request = Request::builder().method(Method::GET).body(body.clone()).unwrap();
        let response = handle(ctx.clone(), request, id.clone()).await.unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        let request = Request::builder()
            .method(Method::POST)
            .header(CONTENT_TYPE, "text/plain")
            .body(body)
            .unwrap();
        let response = handle(ctx, request, id).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}
