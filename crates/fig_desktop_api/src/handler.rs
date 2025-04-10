use std::time::Duration;

use base64::prelude::*;
use fig_os_shim::{
    ContextArcProvider,
    ContextProvider,
    EnvProvider,
    FsProvider,
};
pub use fig_proto::fig::client_originated_message::Submessage as ClientOriginatedSubMessage;
pub use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
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
use fig_proto::prost::Message;
use fig_settings::settings::SettingsProvider;
use fig_settings::state::StateProvider;
use tracing::warn;

use crate::error::Result;
use crate::kv::KVStore;
use crate::requests::{
    self,
    RequestResult,
    RequestResultImpl,
};

pub struct Wrapped<Ctx, Req> {
    pub message_id: i64,
    pub context: Ctx,
    pub request: Req,
}

#[async_trait::async_trait]
pub trait EventHandler {
    type Ctx: KVStore + EnvProvider + FsProvider + Sync + Send;

    async fn notification(&self, request: Wrapped<Self::Ctx, NotificationRequest>) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    async fn insert_text(&self, request: Wrapped<Self::Ctx, InsertTextRequest>) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    async fn aggregate_session_metric_action(
        &self,
        request: Wrapped<Self::Ctx, AggregateSessionMetricActionRequest>,
    ) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    async fn position_window(&self, request: Wrapped<Self::Ctx, PositionWindowRequest>) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    async fn window_focus(&self, request: Wrapped<Self::Ctx, WindowFocusRequest>) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    async fn onboarding(&self, request: Wrapped<Self::Ctx, OnboardingRequest>) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    async fn run_process(&self, request: Wrapped<Self::Ctx, RunProcessRequest>) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    async fn update_application_properties(
        &self,
        request: Wrapped<Self::Ctx, UpdateApplicationPropertiesRequest>,
    ) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    // TODO: rename EventHandler to RequestHandler, and move this callback out
    // to a separate trait.
    async fn user_logged_in_callback(&self, _context: Self::Ctx) {}

    async fn user_logout(&self, request: Wrapped<Self::Ctx, UserLogoutRequest>) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }

    async fn drag_window(&self, request: Wrapped<Self::Ctx, DragWindowRequest>) -> RequestResult {
        RequestResult::unimplemented(request.request)
    }
}

pub fn request_from_b64(request_b64: &str) -> Result<ClientOriginatedMessage> {
    let data = BASE64_STANDARD.decode(request_b64)?;
    Ok(ClientOriginatedMessage::decode(data.as_slice())?)
}

pub fn response_to_b64(response_message: ServerOriginatedMessage) -> String {
    BASE64_STANDARD.encode(response_message.encode_to_vec())
}

pub async fn api_request<Ctx, E>(
    event_handler: E,
    ctx: Ctx,
    request: ClientOriginatedMessage,
) -> Result<ServerOriginatedMessage>
where
    Ctx: KVStore + SettingsProvider + StateProvider + ContextProvider + ContextArcProvider + Send + Sync,
    E: EventHandler<Ctx = Ctx> + Sync,
{
    let request_id = match request.id {
        Some(request_id) => request_id,
        None => return Err(crate::error::Error::NoMessageId),
    };

    let response = match tokio::time::timeout(
        Duration::from_secs(60),
        handle_request(event_handler, ctx, request_id, request),
    )
    .await
    {
        Ok(response) => response,
        Err(_) => return Err(crate::error::Error::Timeout),
    };

    Ok(ServerOriginatedMessage {
        id: Some(request_id),
        submessage: Some(match response {
            Ok(msg) => *msg,
            Err(msg) => ServerOriginatedSubMessage::Error(msg.to_string()),
        }),
    })
}

async fn handle_request<Ctx, E>(
    event_handler: E,
    ctx: Ctx,
    message_id: i64,
    message: ClientOriginatedMessage,
) -> RequestResult
where
    Ctx: KVStore + SettingsProvider + StateProvider + ContextProvider + ContextArcProvider + Send + Sync,
    E: EventHandler<Ctx = Ctx> + Sync,
{
    macro_rules! request {
        ($request:expr) => {
            Wrapped {
                message_id,
                context: ctx,
                request: $request,
            }
        };
    }

    match message.submessage {
        Some(submessage) => {
            use ClientOriginatedSubMessage::{
                AggregateSessionMetricActionRequest,
                AppendToFileRequest,
                AuthBuilderIdPollCreateTokenRequest,
                AuthBuilderIdStartDeviceAuthorizationRequest,
                AuthCancelPkceAuthorizationRequest,
                AuthFinishPkceAuthorizationRequest,
                AuthStartPkceAuthorizationRequest,
                AuthStatusRequest,
                CheckForUpdatesRequest,
                CodewhispererListCustomizationRequest,
                ContentsOfDirectoryRequest,
                CreateDirectoryRequest,
                DestinationOfSymbolicLinkRequest,
                DragWindowRequest,
                GetLocalStateRequest,
                GetPlatformInfoRequest,
                GetSettingsPropertyRequest,
                HistoryQueryRequest,
                InsertTextRequest,
                InstallRequest,
                NotificationRequest,
                OnboardingRequest,
                OpenInExternalApplicationRequest,
                PingRequest,
                PositionWindowRequest,
                ReadFileRequest,
                RunProcessRequest,
                TelemetryPageRequest,
                TelemetryTrackRequest,
                UpdateApplicationPropertiesRequest,
                UpdateApplicationRequest,
                UpdateLocalStateRequest,
                UpdateSettingsPropertyRequest,
                UserLogoutRequest,
                WindowFocusRequest,
                WriteFileRequest,
            };
            use requests::*;

            match submessage {
                // figterm
                InsertTextRequest(request) => event_handler.insert_text(request!(request)).await,
                // fs
                ReadFileRequest(request) => fs::read_file(request, ctx.env(), ctx.fs()).await,
                WriteFileRequest(request) => fs::write_file(request, ctx.env(), ctx.fs()).await,
                AppendToFileRequest(request) => fs::append_to_file(request, ctx.env()).await,
                DestinationOfSymbolicLinkRequest(request) => {
                    requests::fs::destination_of_symbolic_link(request, ctx.env()).await
                },
                ContentsOfDirectoryRequest(request) => fs::contents_of_directory(request, ctx.env()).await,
                CreateDirectoryRequest(request) => fs::create_directory_request(request, ctx.env(), ctx.fs()).await,
                // notifications
                NotificationRequest(request) => event_handler.notification(request!(request)).await,
                // process
                RunProcessRequest(request) => event_handler.run_process(request!(request)).await,
                // properties
                UpdateApplicationPropertiesRequest(request) => {
                    event_handler.update_application_properties(request!(request)).await
                },
                // state
                GetLocalStateRequest(request) => state::get(request).await,
                UpdateLocalStateRequest(request) => state::update(request).await,
                // settings
                GetSettingsPropertyRequest(request) => settings::get(request).await,
                UpdateSettingsPropertyRequest(request) => settings::update(request).await,
                // telemetry
                TelemetryTrackRequest(request) => telemetry::handle_track_request(request).await,
                TelemetryPageRequest(request) => telemetry::handle_page_request(request).await,
                AggregateSessionMetricActionRequest(request) => {
                    event_handler.aggregate_session_metric_action(request!(request)).await
                },
                // window
                PositionWindowRequest(request) => event_handler.position_window(request!(request)).await,
                WindowFocusRequest(request) => event_handler.window_focus(request!(request)).await,
                DragWindowRequest(request) => event_handler.drag_window(request!(request)).await,
                // onboarding
                OnboardingRequest(request) => event_handler.onboarding(request!(request)).await,
                // install
                InstallRequest(request) => install::install(request, &ctx).await,
                // history
                HistoryQueryRequest(request) => history::query(request).await,
                // auth
                AuthStatusRequest(request) => auth::status(request).await,
                AuthStartPkceAuthorizationRequest(request) => auth::start_pkce_authorization(request).await,
                AuthFinishPkceAuthorizationRequest(request) => {
                    let result = auth::finish_pkce_authorization(request).await;
                    event_handler.user_logged_in_callback(ctx).await;
                    result
                },
                AuthCancelPkceAuthorizationRequest(request) => auth::cancel_pkce_authorization(request).await,
                AuthBuilderIdStartDeviceAuthorizationRequest(request) => {
                    auth::builder_id_start_device_authorization(request, &ctx).await
                },
                AuthBuilderIdPollCreateTokenRequest(request) => auth::builder_id_poll_create_token(request, &ctx).await,
                // codewhisperer api
                CodewhispererListCustomizationRequest(request) => codewhisperer::list_customization(request).await,
                // other
                OpenInExternalApplicationRequest(request) => other::open_in_external_application(request).await,
                PingRequest(request) => other::ping(request),
                UpdateApplicationRequest(request) => update::update_application(request).await,
                CheckForUpdatesRequest(request) => update::check_for_updates(request).await,
                GetPlatformInfoRequest(request) => platform::get_platform_info(request, &ctx).await,
                UserLogoutRequest(request) => event_handler.user_logout(request!(request)).await,
            }
        },
        None => {
            warn!("Missing submessage: {message:?}");
            RequestResult::error("Missing submessage")
        },
    }
}
