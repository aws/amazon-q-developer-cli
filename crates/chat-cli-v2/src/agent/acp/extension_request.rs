//! Dispatch for inbound ACP extension requests owned by the V2 agent.

use std::fmt;
use std::sync::Arc;

use agent::protocol::AgentError;
use futures::future::BoxFuture;
use sacp::schema::SessionId;
use sacp::{
    ConnectionTo,
    Dispatch,
    HandleDispatchFrom,
    Handled,
    JsonRpcMessage,
    JsonRpcResponse,
    Responder,
    UntypedMessage,
};
use serde_json::Value;
use tracing::debug;

use super::acp_agent::AcpSessionHandle;
use super::commands::chat;
use super::schema::{
    CommandExecuteRequest,
    CommandExecuteResponse,
    CommandOptionsRequest,
    CommandOptionsResponseWrapper,
    ListSessionsRequest,
    ListSessionsResponse,
    MessageSendRequest,
    MessageSendResponse,
    SessionSpawnRequest,
    SessionSpawnResponse,
    SessionSteerClearRequest,
    SessionSteerClearResponse,
    SessionSteerRequest,
    SessionSteerResponse,
    SettingsListRequest,
    SettingsListResponse,
    SettingsSetRequest,
    SettingsSetResponse,
    TerminateSessionRequest,
    TerminateSessionResponse,
};
use super::session_manager::SessionManagerHandle;
use crate::database::settings::Setting;
use crate::os::Os;
use crate::telemetry::{
    AcpMethodTelemetry,
    RequestObservation,
};

macro_rules! define_extension_requests {
    ($( $variant:ident => $request:ty ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug)]
        enum ExtensionRequestKind {
            $( $variant, )+
        }

        impl ExtensionRequestKind {
            fn recognize(method: &str) -> Option<Self> {
                $( if <$request>::matches_method(method) {
                    return Some(Self::$variant);
                } )+
                None
            }

            fn parse(self, message: &UntypedMessage) -> Result<ExtensionRequest, sacp::Error> {
                match self {
                    $( Self::$variant => <$request>::parse_message(message.method(), message.params())
                        .map(ExtensionRequest::$variant), )+
                }
            }
        }

        #[derive(Debug)]
        enum ExtensionRequest {
            $( $variant($request), )+
        }

        impl ExtensionRequest {
            fn method(&self) -> &str {
                match self {
                    $( Self::$variant(request) => request.method(), )+
                }
            }
        }
    };
}

define_extension_requests! {
    CommandExecute => CommandExecuteRequest,
    CommandOptions => CommandOptionsRequest,
    ListSessions => ListSessionsRequest,
    TerminateSession => TerminateSessionRequest,
    SettingsList => SettingsListRequest,
    SettingsSet => SettingsSetRequest,
    SessionSteer => SessionSteerRequest,
    SessionSteerClear => SessionSteerClearRequest,
    SessionSpawn => SessionSpawnRequest,
    MessageSend => MessageSendRequest,
}

trait ExtensionRequestExecutor: Send + Sync + 'static {
    fn execute(&self, request: ExtensionRequest) -> BoxFuture<'static, Result<Value, sacp::Error>>;
}

#[derive(Clone)]
struct ProductionExecutor {
    session_manager: SessionManagerHandle,
    os: Os,
}

impl ProductionExecutor {
    fn require_session(handle: Option<AcpSessionHandle>) -> Result<AcpSessionHandle, sacp::Error> {
        handle.ok_or_else(|| sacp::Error::invalid_params().data("Unknown session id"))
    }

    async fn session_handle(&self, session_id: String) -> Result<AcpSessionHandle, sacp::Error> {
        Self::require_session(
            self.session_manager
                .find_session_handle(&SessionId::new(session_id))
                .await?,
        )
    }

    fn map_wake_error(error: AgentError) -> sacp::Error {
        let message = error.to_string();
        match error {
            AgentError::NotIdle => sacp::Error::invalid_params().data(message),
            _ => sacp::util::internal_error(message),
        }
    }
}

impl ExtensionRequestExecutor for ProductionExecutor {
    fn execute(&self, request: ExtensionRequest) -> BoxFuture<'static, Result<Value, sacp::Error>> {
        let this = self.clone();
        Box::pin(async move {
            let method = request.method().to_owned();
            let response = match request {
                ExtensionRequest::CommandExecute(request) => {
                    let handle = this.session_handle(request.session_id).await?;
                    CommandExecuteResponse::from(handle.execute_command(request.command).await).into_json(&method)
                },
                ExtensionRequest::CommandOptions(request) => {
                    let handle = this.session_handle(request.session_id).await?;
                    CommandOptionsResponseWrapper::from(
                        handle.get_command_options(request.command, request.partial).await,
                    )
                    .into_json(&method)
                },
                ExtensionRequest::ListSessions(request) => {
                    let sessions = chat::list_sessions(&this.session_manager, request.cwd).await?;
                    ListSessionsResponse {
                        sessions,
                        next_cursor: None,
                    }
                    .into_json(&method)
                },
                ExtensionRequest::TerminateSession(request) => {
                    this.session_manager
                        .terminate_session(&SessionId::new(request.session_id))
                        .await;
                    TerminateSessionResponse {}.into_json(&method)
                },
                ExtensionRequest::SettingsList(_) => {
                    SettingsListResponse(this.os.database.settings.map().clone()).into_json(&method)
                },
                ExtensionRequest::SettingsSet(request) => {
                    let key = Setting::try_from(request.key.as_str())
                        .map_err(|error| sacp::Error::invalid_params().data(error.to_string()))?;
                    this.os
                        .database
                        .settings
                        .set(key, request.value, None)
                        .await
                        .map_err(|error| sacp::util::internal_error(error.to_string()))?;
                    SettingsSetResponse {}.into_json(&method)
                },
                ExtensionRequest::SessionSteer(request) => {
                    let handle = this.session_handle(request.session_id).await?;
                    let agent = handle
                        .get_agent_handle()
                        .await
                        .ok_or_else(|| sacp::util::internal_error("agent not available"))?;
                    agent
                        .steer_message(request.message)
                        .await
                        .map_err(|error| sacp::util::internal_error(error.to_string()))?;
                    SessionSteerResponse { queued: true }.into_json(&method)
                },
                ExtensionRequest::SessionSteerClear(request) => {
                    let handle = this.session_handle(request.session_id).await?;
                    let agent = handle
                        .get_agent_handle()
                        .await
                        .ok_or_else(|| sacp::util::internal_error("agent not available"))?;
                    agent
                        .clear_steering()
                        .await
                        .map_err(|error| sacp::util::internal_error(error.to_string()))?;
                    SessionSteerClearResponse { cleared: true }.into_json(&method)
                },
                ExtensionRequest::SessionSpawn(request) => {
                    let session_id = SessionId::new(request.session_id);
                    let result = this
                        .session_manager
                        .spawn_orchestrated_session(
                            &session_id,
                            request.agent_name.unwrap_or_else(|| "kiro_default".to_string()),
                            request.task,
                            request.name,
                            None,
                            None,
                            true,
                        )
                        .await
                        .map_err(|error| sacp::util::internal_error(format!("Spawn failed: {error}")))?;
                    SessionSpawnResponse {
                        session_id: result.session_id,
                        name: result.name,
                    }
                    .into_json(&method)
                },
                ExtensionRequest::MessageSend(request) => {
                    let handle = this.session_handle(request.session_id).await?;
                    handle
                        .wake_session(request.content)
                        .await
                        .map_err(ProductionExecutor::map_wake_error)?;
                    MessageSendResponse { ok: true }.into_json(&method)
                },
            }?;
            Ok(response)
        })
    }
}

/// Claims the explicit V2 extension request set and owns its full response lifecycle.
pub(crate) struct ExtensionRequestHandler {
    executor: Arc<dyn ExtensionRequestExecutor>,
    telemetry: AcpMethodTelemetry,
}

impl ExtensionRequestHandler {
    pub(crate) fn new(session_manager: SessionManagerHandle, os: Os, telemetry: AcpMethodTelemetry) -> Self {
        Self {
            executor: Arc::new(ProductionExecutor { session_manager, os }),
            telemetry,
        }
    }

    #[cfg(test)]
    fn with_executor(executor: impl ExtensionRequestExecutor, telemetry: AcpMethodTelemetry) -> Self {
        Self {
            executor: Arc::new(executor),
            telemetry,
        }
    }
}

impl<Counterpart> HandleDispatchFrom<Counterpart> for ExtensionRequestHandler
where
    Counterpart: sacp::Role,
{
    async fn handle_dispatch_from(
        &mut self,
        dispatch: Dispatch,
        connection: ConnectionTo<Counterpart>,
    ) -> Result<Handled<Dispatch>, sacp::Error> {
        let Dispatch::Request(message, responder) = dispatch else {
            return Ok(Handled::No {
                message: dispatch,
                retry: false,
            });
        };
        let Some(kind) = ExtensionRequestKind::recognize(message.method()) else {
            return Ok(Handled::No {
                message: Dispatch::Request(message, responder),
                retry: false,
            });
        };

        let observation = self.telemetry.observe(message.method(), message.params());
        let request = kind.parse(&message);
        match request {
            Ok(request) => {
                let executor = Arc::clone(&self.executor);
                connection.spawn(async move {
                    respond(observation, responder, executor.execute(request).await);
                    Ok(())
                })?;
            },
            Err(error) => respond(observation, responder, Err(error)),
        }

        Ok(Handled::Yes)
    }

    fn describe_chain(&self) -> impl fmt::Debug {
        "ExtensionRequestHandler"
    }
}

fn respond(observation: RequestObservation, responder: Responder<Value>, result: Result<Value, sacp::Error>) {
    observation.finish(&result);
    if let Err(error) = responder.respond_with_result(result) {
        debug!(%error, "failed to send ACP extension response");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use kiro_telemetry::metric::{
        self,
        AcpMethodOutcome,
    };
    use kiro_telemetry_host::{
        Event,
        EventType,
    };
    use sacp::role::UntypedRole;
    use sacp::{
        ConnectTo,
        JsonRpcRequest,
    };
    use serde::{
        Deserialize,
        Serialize,
    };

    use super::*;
    use crate::telemetry::AcpConnectionContext;

    #[test]
    fn production_error_mappings_distinguish_client_and_server_failures() {
        let missing_session = ProductionExecutor::require_session(None).unwrap_err();
        assert_eq!(missing_session.code, sacp::ErrorCode::InvalidParams);

        let busy_session = ProductionExecutor::map_wake_error(AgentError::NotIdle);
        assert_eq!(busy_session.code, sacp::ErrorCode::InvalidParams);

        let closed_agent = ProductionExecutor::map_wake_error(AgentError::Channel);
        assert_eq!(closed_agent.code, sacp::ErrorCode::InternalError);
    }

    #[derive(Clone, Copy)]
    enum TestResult {
        Success,
        Fault,
        Abandon,
    }

    #[derive(Clone)]
    struct TestExecutor {
        result: TestResult,
        methods: Arc<Mutex<Vec<String>>>,
    }

    impl ExtensionRequestExecutor for TestExecutor {
        fn execute(&self, request: ExtensionRequest) -> BoxFuture<'static, Result<Value, sacp::Error>> {
            let result = self.result;
            let methods = Arc::clone(&self.methods);
            Box::pin(async move {
                methods.lock().unwrap().push(request.method().to_owned());
                match result {
                    TestResult::Success => match request {
                        ExtensionRequest::SessionSteerClear(_) => Ok(serde_json::json!({ "cleared": true })),
                        _ => Ok(serde_json::json!({})),
                    },
                    TestResult::Fault => Err(sacp::Error::internal_error()),
                    TestResult::Abandon => std::future::pending().await,
                }
            })
        }
    }

    struct TestServer {
        handler: ExtensionRequestHandler,
    }

    impl ConnectTo<UntypedRole> for TestServer {
        async fn connect_to(self, peer: impl ConnectTo<UntypedRole>) -> Result<(), sacp::Error> {
            UntypedRole
                .builder()
                .with_handler(self.handler)
                .on_receive_request(
                    async |request: StandardRequest, responder, _connection| {
                        responder.respond(StandardResponse { value: request.value })
                    },
                    sacp::on_receive_request!(),
                )
                .connect_to(peer)
                .await
        }
    }

    fn test_handler(
        result: TestResult,
    ) -> (
        TestServer,
        tokio::sync::mpsc::UnboundedReceiver<Event>,
        Arc<Mutex<Vec<String>>>,
    ) {
        let context = AcpConnectionContext::default();
        context.initialize("kiro-tui".to_string(), "1.0.0".to_string());
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = Arc::new(move |event| {
            event_tx.send(event).expect("test event receiver remains open");
        });
        let telemetry = AcpMethodTelemetry::with_event_sink(context, sink);
        let methods = Arc::new(Mutex::new(Vec::new()));
        let executor = TestExecutor {
            result,
            methods: Arc::clone(&methods),
        };
        (
            TestServer {
                handler: ExtensionRequestHandler::with_executor(executor, telemetry),
            },
            event_rx,
            methods,
        )
    }

    #[tokio::test]
    async fn dispatch_records_success_with_client_and_session_attribution() {
        let (server, mut events, methods) = test_handler(TestResult::Success);

        UntypedRole
            .builder()
            .connect_with(server, async |connection| {
                let response = connection
                    .send_request(SessionSteerClearRequest {
                        session_id: "session-123".to_string(),
                    })
                    .block_task()
                    .await?;
                assert!(response.cleared);
                Ok(())
            })
            .await
            .unwrap();

        let invocation = events.recv().await.unwrap();
        assert_eq!(invocation.acp_client_name.as_deref(), Some("kiro-tui"));
        assert_eq!(
            invocation.session_interface,
            Some(metric::SessionInterface::InteractiveCli)
        );
        assert_eq!(invocation.metric_log_properties().session_id(), Some("session-123"));
        assert!(matches!(
            invocation.ty,
            EventType::AcpMethodInvoked { ref method } if method == "_session/steer/clear"
        ));

        let completion = events.recv().await.unwrap();
        assert!(matches!(completion.ty, EventType::AcpMethodCompleted {
            outcome: AcpMethodOutcome::Success,
            ..
        }));
        assert_eq!(methods.lock().unwrap().as_slice(), ["_session/steer/clear"]);
    }

    #[tokio::test]
    async fn dispatch_records_invalid_params_before_execution() {
        let (server, mut events, methods) = test_handler(TestResult::Success);

        UntypedRole
            .builder()
            .connect_with(server, async |connection| {
                let result = connection.send_request(MalformedSettingsSetRequest).block_task().await;
                assert!(result.is_err());
                Ok(())
            })
            .await
            .unwrap();

        assert!(matches!(
            events.recv().await.unwrap().ty,
            EventType::AcpMethodInvoked { .. }
        ));
        assert!(matches!(
            events.recv().await.unwrap().ty,
            EventType::AcpMethodCompleted {
                outcome: AcpMethodOutcome::Error,
                ..
            }
        ));
        assert!(methods.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn dispatch_records_handler_fault() {
        let (server, mut events, _) = test_handler(TestResult::Fault);

        UntypedRole
            .builder()
            .connect_with(server, async |connection| {
                let result = connection.send_request(SettingsListRequest {}).block_task().await;
                assert!(result.is_err());
                Ok(())
            })
            .await
            .unwrap();

        let _ = events.recv().await.unwrap();
        assert!(matches!(
            events.recv().await.unwrap().ty,
            EventType::AcpMethodCompleted {
                outcome: AcpMethodOutcome::Fault,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn connection_teardown_records_abandoned_request() {
        let (server, mut events, _) = test_handler(TestResult::Abandon);

        let connection = UntypedRole.builder().connect_with(server, async |connection| {
            let request = connection.send_request(SettingsListRequest {}).block_task();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(20), request)
                    .await
                    .is_err()
            );
            Ok(())
        });
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), connection).await;

        assert!(matches!(
            events.recv().await.unwrap().ty,
            EventType::AcpMethodInvoked { .. }
        ));
        let completion = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
            .await
            .expect("connection teardown drops the request observation")
            .unwrap();
        assert!(matches!(completion.ty, EventType::AcpMethodCompleted {
            outcome: AcpMethodOutcome::Unknown,
            ..
        }));
    }

    #[tokio::test]
    async fn standard_requests_pass_through_without_telemetry() {
        let (server, mut events, methods) = test_handler(TestResult::Success);

        UntypedRole
            .builder()
            .connect_with(server, async |connection| {
                let response = connection
                    .send_request(StandardRequest {
                        value: "unchanged".to_string(),
                    })
                    .block_task()
                    .await?;
                assert_eq!(response.value, "unchanged");
                Ok(())
            })
            .await
            .unwrap();

        assert!(events.try_recv().is_err());
        assert!(methods.lock().unwrap().is_empty());
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct StandardRequest {
        value: String,
    }

    impl JsonRpcMessage for StandardRequest {
        fn matches_method(method: &str) -> bool {
            method == "session/example"
        }

        fn method(&self) -> &str {
            "session/example"
        }

        fn to_untyped_message(&self) -> Result<UntypedMessage, sacp::Error> {
            Ok(UntypedMessage {
                method: self.method().to_string(),
                params: sacp::util::json_cast(self)?,
            })
        }

        fn parse_message(method: &str, params: &impl Serialize) -> Result<Self, sacp::Error> {
            if !Self::matches_method(method) {
                return Err(sacp::Error::method_not_found());
            }
            sacp::util::json_cast(params)
        }
    }

    impl JsonRpcRequest for StandardRequest {
        type Response = StandardResponse;
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct StandardResponse {
        value: String,
    }

    impl JsonRpcResponse for StandardResponse {
        fn into_json(self, _method: &str) -> Result<Value, sacp::Error> {
            sacp::util::json_cast(self)
        }

        fn from_value(_method: &str, value: Value) -> Result<Self, sacp::Error> {
            sacp::util::json_cast(value)
        }
    }

    #[derive(Debug, Clone)]
    struct MalformedSettingsSetRequest;

    impl JsonRpcMessage for MalformedSettingsSetRequest {
        fn matches_method(method: &str) -> bool {
            SettingsSetRequest::matches_method(method)
        }

        fn method(&self) -> &str {
            "_kiro.dev/settings/set"
        }

        fn to_untyped_message(&self) -> Result<UntypedMessage, sacp::Error> {
            Ok(UntypedMessage {
                method: self.method().to_string(),
                params: serde_json::json!({ "key": 7, "value": true }),
            })
        }

        fn parse_message(_method: &str, _params: &impl Serialize) -> Result<Self, sacp::Error> {
            Ok(Self)
        }
    }

    impl JsonRpcRequest for MalformedSettingsSetRequest {
        type Response = SettingsSetResponse;
    }
}
