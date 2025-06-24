mod figterm;
mod notifications;
mod onboarding;
mod process;
mod properties;
mod telemetry;
mod user;
mod window;

use std::marker::PhantomData;
use std::sync::Arc;

use fig_desktop_api::handler::Wrapped;
use fig_desktop_api::kv::{
    DashKVStore,
    KVStore,
};
#[allow(unused_imports)]
pub use fig_desktop_api::requests::{
    Error,
    RequestResult,
    RequestResultImpl,
};
use fig_os_shim::{
    ContextArcProvider,
    ContextProvider,
};
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    AggregateSessionMetricActionRequest,
    ClientOriginatedMessage,
    DragWindowRequest,
    InsertTextRequest,
    NotificationRequest,
    OnboardingRequest,
    PositionWindowRequest,
    RunProcessRequest,
    ServerOriginatedMessage,
    UpdateApplicationPropertiesRequest,
    UserLogoutRequest,
    WindowFocusRequest,
};
use fig_remote_ipc::figterm::FigtermState;
use fig_settings::{
    Settings,
    SettingsProvider,
    State,
    StateProvider,
};
use tracing::{
    error,
    trace,
    warn,
};

use crate::event::{
    EmitEventName,
    Event,
    WindowEvent,
};
use crate::webview::WindowId;
use crate::webview::notification::WebviewNotificationsState;
use crate::{
    EventLoopProxy,
    InterceptState,
};

pub struct Context<'a> {
    pub figterm_state: &'a FigtermState,
    pub intercept_state: &'a InterceptState,
    pub notifications_state: &'a WebviewNotificationsState,
    pub proxy: &'a EventLoopProxy,
    pub window_id: &'a WindowId,
    pub dash_kv_store: &'a DashKVStore,
    pub settings: &'a Settings,
    pub state: &'a State,
    pub ctx: Arc<fig_os_shim::Context>,
}

impl KVStore for Context<'_> {
    fn set_raw(&self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) {
        self.dash_kv_store.set_raw(key, value);
    }

    fn get_raw(&self, key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
        self.dash_kv_store.get_raw(key)
    }
}

impl SettingsProvider for Context<'_> {
    fn settings(&self) -> &Settings {
        self.settings
    }
}

impl StateProvider for Context<'_> {
    fn state(&self) -> &State {
        self.state
    }
}

impl ContextProvider for Context<'_> {
    fn context(&self) -> &fig_os_shim::Context {
        &self.ctx
    }
}

impl ContextArcProvider for Context<'_> {
    fn context_arc(&self) -> Arc<fig_os_shim::Context> {
        Arc::clone(&self.ctx)
    }
}

#[derive(Default)]
pub struct EventHandler<'a> {
    _lifetime: PhantomData<&'a ()>,
}

#[async_trait::async_trait]
impl<'a> fig_desktop_api::handler::EventHandler for EventHandler<'a> {
    type Ctx = Context<'a>;

    async fn notification(&self, request: Wrapped<Self::Ctx, NotificationRequest>) -> RequestResult {
        notifications::handle_request(
            request.request,
            request.context.window_id.clone(),
            request.message_id,
            request.context.notifications_state,
        )
        .await
    }

    async fn insert_text(&self, request: Wrapped<Self::Ctx, InsertTextRequest>) -> RequestResult {
        figterm::insert_text(request.request, request.context.figterm_state).await
    }

    async fn aggregate_session_metric_action(
        &self,
        request: Wrapped<Self::Ctx, AggregateSessionMetricActionRequest>,
    ) -> RequestResult {
        telemetry::handle_aggregate_session_metric_action_request(request.request, request.context.figterm_state)
    }

    async fn position_window(&self, request: Wrapped<Self::Ctx, PositionWindowRequest>) -> RequestResult {
        window::position_window(
            request.request,
            request.context.window_id.clone(),
            request.context.figterm_state,
            request.context.proxy,
        )
        .await
    }

    async fn window_focus(&self, request: Wrapped<Self::Ctx, WindowFocusRequest>) -> RequestResult {
        window::focus(
            request.request,
            request.context.window_id.clone(),
            request.context.proxy,
        )
        .await
    }

    async fn drag_window(&self, request: Wrapped<Self::Ctx, DragWindowRequest>) -> RequestResult {
        window::drag(
            request.request,
            request.context.window_id.clone(),
            request.context.proxy,
        )
        .await
    }

    async fn onboarding(&self, request: Wrapped<Self::Ctx, OnboardingRequest>) -> RequestResult {
        onboarding::onboarding(request.request, request.context.proxy, &request.context).await
    }

    async fn run_process(&self, request: Wrapped<Self::Ctx, RunProcessRequest>) -> RequestResult {
        process::run(request.request, request.context.figterm_state).await
    }

    async fn update_application_properties(
        &self,
        request: Wrapped<Self::Ctx, UpdateApplicationPropertiesRequest>,
    ) -> RequestResult {
        properties::update(
            request.request,
            request.context.figterm_state,
            request.context.intercept_state,
        )
    }

    async fn user_logged_in_callback(&self, context: Self::Ctx) {
        context
            .proxy
            .send_event(Event::ReloadTray { is_logged_in: true })
            .map_err(|err| error!(?err, "Unable to send event on user log in"))
            .ok();
    }

    async fn user_logout(&self, request: Wrapped<Self::Ctx, UserLogoutRequest>) -> RequestResult {
        user::logout(request.request, request.context.proxy).await
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn api_request(
    window_id: WindowId,
    message: fig_desktop_api::error::Result<ClientOriginatedMessage>,
    figterm_state: &FigtermState,
    intercept_state: &InterceptState,
    notifications_state: &WebviewNotificationsState,
    proxy: &EventLoopProxy,
    dash_kv_store: &DashKVStore,
) {
    let response = match message {
        Ok(request) => {
            let id = request.id;
            trace!(?request, %window_id, "Received request");
            match fig_desktop_api::handler::api_request(
                EventHandler::default(),
                Context {
                    figterm_state,
                    intercept_state,
                    notifications_state,
                    proxy,
                    window_id: &window_id,
                    dash_kv_store,
                    settings: &Settings::new(),
                    state: &State::new(),
                    ctx: fig_os_shim::Context::new(),
                },
                request,
            )
            .await
            {
                Ok(response) => response,
                Err(err) => {
                    warn!(?err, ?id, "Error handling request");
                    ServerOriginatedMessage {
                        id,
                        submessage: Some(ServerOriginatedSubMessage::Error(err.to_string())),
                    }
                },
            }
        },
        Err(err) => {
            warn!(?err, "Error decoding message");
            ServerOriginatedMessage {
                id: None,
                submessage: Some(ServerOriginatedSubMessage::Error(err.to_string())),
            }
        },
    };

    proxy
        .send_event(Event::WindowEvent {
            window_id,
            window_event: WindowEvent::Emit {
                event_name: match response.id {
                    Some(_) => EmitEventName::ProtoMessageReceived,
                    None => EmitEventName::GlobalErrorOccurred,
                },
                payload: fig_desktop_api::handler::response_to_b64(response).into(),
            },
        })
        .unwrap();
}
