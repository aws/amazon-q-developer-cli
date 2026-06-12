use std::time::Duration;

use kiro_telemetry_schema::{
    MetricKind,
    registry,
};

use crate::TelemetryLogRecord;
use crate::metric::{
    ClientApplication,
    McpServerClass,
    McpServerInit,
    MetricBuildError,
    ModelClass,
    Outcome,
    ResultKind,
    SubagentDepthBucket,
};

#[derive(Clone, Debug)]
pub struct LogBuilder {
    record: TelemetryLogRecord,
}

impl LogBuilder {
    pub fn attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.record = self.record.with_attribute(key, value);
        self
    }

    pub fn optional_attribute(self, key: impl Into<String>, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => self.attribute(key, value),
            None => self,
        }
    }

    pub fn resource_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.record = self.record.with_resource_attribute(key, value);
        self
    }

    pub fn build(self) -> Result<TelemetryLogRecord, MetricBuildError> {
        validate_log_record(&self.record)?;
        Ok(self.record)
    }

    #[track_caller]
    pub fn expect_valid(self) -> TelemetryLogRecord {
        self.build()
            .expect("telemetry log mapping must match the canonical schema")
    }
}

pub fn event(name: impl Into<String>) -> LogBuilder {
    LogBuilder {
        record: TelemetryLogRecord::new(name),
    }
}

pub fn validate_log_record(record: &TelemetryLogRecord) -> Result<(), MetricBuildError> {
    let Some(spec) = registry().metric(&record.name) else {
        return Err(MetricBuildError::UnknownMetric(record.name.clone()));
    };

    if spec.kind != MetricKind::LogEvent {
        return Err(MetricBuildError::KindMismatch {
            metric: record.name.clone(),
            expected: spec.kind,
            actual: MetricKind::LogEvent,
        });
    }

    let mut seen = std::collections::HashSet::new();
    for attr in &record.attributes {
        if !seen.insert(attr.key.as_str()) {
            return Err(MetricBuildError::DuplicateAttribute {
                metric: record.name.clone(),
                attribute: attr.key.clone(),
            });
        }

        if !spec.attributes.iter().any(|allowed| allowed == &attr.key) {
            return Err(MetricBuildError::UnsupportedAttribute {
                metric: record.name.clone(),
                attribute: attr.key.clone(),
            });
        }

        if let Some(attr_spec) = registry().attribute(&attr.key)
            && !attr_spec.accepts_value(&attr.value)
        {
            return Err(MetricBuildError::InvalidAttributeValue {
                metric: record.name.clone(),
                attribute: attr.key.clone(),
                value: attr.value.clone(),
            });
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionReason {
    Stop,
    ToolUse,
    MaxTokens,
    Error,
    Cancelled,
    Other,
}

impl CompletionReason {
    pub fn from_name(value: &str) -> Self {
        match value {
            "stop" => Self::Stop,
            "tool_use" => Self::ToolUse,
            "max_tokens" => Self::MaxTokens,
            "error" => Self::Error,
            "cancelled" => Self::Cancelled,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::ToolUse => "tool_use",
            Self::MaxTokens => "max_tokens",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
            Self::Other => "_other_",
        }
    }
}

pub fn conversation_completed(
    session_id: impl Into<String>,
    conversation_id: impl Into<String>,
    completion_reason: CompletionReason,
) -> TelemetryLogRecord {
    event("kiro_cli_conversation_completed")
        .attribute("conversation_id", conversation_id)
        .attribute("session_id", session_id)
        .attribute("completion_reason", completion_reason.as_str())
        .expect_valid()
}

pub fn metering_event(
    request_id: Option<&str>,
    model_class: Option<ModelClass>,
    client_application: Option<ClientApplication>,
    usage: f64,
    unit: &str,
    unit_plural: &str,
) -> TelemetryLogRecord {
    event("kiro_cli_metering_event")
        .attribute("metering_usage", usage.to_string())
        .attribute("metering_unit", unit)
        .attribute("metering_unit_plural", unit_plural)
        .optional_attribute("request_id", request_id)
        .optional_attribute("model_class", model_class.map(|value| value.as_str()))
        .optional_attribute("client_application", client_application.map(|value| value.as_str()))
        .expect_valid()
}

pub fn metering_event_from_names(
    request_id: Option<&str>,
    model_id: Option<&str>,
    client_application: Option<&str>,
    usage: f64,
    unit: &str,
    unit_plural: &str,
) -> TelemetryLogRecord {
    metering_event_record(MeteringEventLog::from_names(
        request_id,
        model_id,
        client_application,
        usage,
        unit,
        unit_plural,
    ))
}

#[derive(Clone, Copy, Debug)]
pub struct MeteringEventLog<'a> {
    pub request_id: Option<&'a str>,
    pub model_class: Option<ModelClass>,
    pub client_application: Option<ClientApplication>,
    pub usage: f64,
    pub unit: &'a str,
    pub unit_plural: &'a str,
}

impl<'a> MeteringEventLog<'a> {
    pub const fn new(usage: f64, unit: &'a str, unit_plural: &'a str) -> Self {
        Self {
            request_id: None,
            model_class: None,
            client_application: None,
            usage,
            unit,
            unit_plural,
        }
    }

    pub fn from_names(
        request_id: Option<&'a str>,
        model_id: Option<&'a str>,
        client_application: Option<&'a str>,
        usage: f64,
        unit: &'a str,
        unit_plural: &'a str,
    ) -> Self {
        Self {
            request_id,
            model_class: model_id.map(|model| ModelClass::from_model_id(Some(model))),
            client_application: client_application.map(|value| ClientApplication::from_name(Some(value))),
            usage,
            unit,
            unit_plural,
        }
    }
}

pub fn metering_event_record(input: MeteringEventLog<'_>) -> TelemetryLogRecord {
    metering_event(
        input.request_id,
        input.model_class,
        input.client_application,
        input.usage,
        input.unit,
        input.unit_plural,
    )
}

pub fn tool_invoked(
    tool_use_id: Option<&str>,
    tool_name: Option<&str>,
    mcp_server_name: Option<&str>,
    is_success: Option<bool>,
    model_class: Option<ModelClass>,
    execution_duration_ms: Option<f64>,
) -> TelemetryLogRecord {
    event("kiro_cli_tool_invoked")
        .optional_attribute("tool_use_id", tool_use_id)
        .optional_attribute("tool_name", tool_name)
        .optional_attribute("mcp_server_name", mcp_server_name)
        .optional_attribute("is_success", is_success.map(|value| value.to_string()))
        .optional_attribute("model_class", model_class.map(|value| value.as_str()))
        .optional_attribute(
            "execution_duration_ms",
            execution_duration_ms.map(|value| format!("{value:.3}")),
        )
        .expect_valid()
}

pub fn tool_invoked_from_names(
    tool_use_id: Option<&str>,
    tool_name: Option<&str>,
    mcp_server_name: Option<&str>,
    is_success: Option<bool>,
    model_id: Option<&str>,
    execution_duration_ms: Option<f64>,
) -> TelemetryLogRecord {
    tool_invoked(
        tool_use_id,
        tool_name,
        mcp_server_name,
        is_success,
        model_id.map(|model| ModelClass::from_model_id(Some(model))),
        execution_duration_ms,
    )
}

#[derive(Clone, Copy, Debug)]
pub struct ToolInvokedLog<'a> {
    pub tool_use_id: Option<&'a str>,
    pub tool_name: Option<&'a str>,
    pub mcp_server_name: Option<&'a str>,
    pub is_success: Option<bool>,
    pub model_class: Option<ModelClass>,
    pub execution_duration: Option<Duration>,
}

impl<'a> ToolInvokedLog<'a> {
    pub const fn new() -> Self {
        Self {
            tool_use_id: None,
            tool_name: None,
            mcp_server_name: None,
            is_success: None,
            model_class: None,
            execution_duration: None,
        }
    }

    pub fn from_names(
        tool_use_id: Option<&'a str>,
        tool_name: Option<&'a str>,
        mcp_server_name: Option<&'a str>,
        is_success: Option<bool>,
        model_id: Option<&'a str>,
        execution_duration: Option<Duration>,
    ) -> Self {
        Self {
            tool_use_id,
            tool_name,
            mcp_server_name,
            is_success,
            model_class: model_id.map(|model| ModelClass::from_model_id(Some(model))),
            execution_duration,
        }
    }
}

impl<'a> Default for ToolInvokedLog<'a> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn tool_invoked_record(input: ToolInvokedLog<'_>) -> TelemetryLogRecord {
    let execution_duration_ms = input
        .execution_duration
        .filter(|duration| !duration.is_zero())
        .map(|duration| duration.as_secs_f64() * 1000.0);

    tool_invoked(
        input.tool_use_id,
        input.tool_name,
        input.mcp_server_name,
        input.is_success,
        input.model_class,
        execution_duration_ms,
    )
}

pub fn mcp_server_init(server_name: &str, server_class: McpServerClass, outcome: Outcome) -> TelemetryLogRecord {
    event("kiro_cli_mcp_server_init")
        .attribute("mcp_server_name", server_name)
        .attribute("mcp_server_class", server_class.as_str())
        .attribute("outcome", outcome.as_str())
        .expect_valid()
}

pub fn mcp_server_init_record(input: McpServerInit<'_>) -> TelemetryLogRecord {
    mcp_server_init(input.server_name, input.server_class, input.outcome)
}

pub fn mcp_server_init_from_name(server_name: &str, init_failure_reason: Option<&str>) -> TelemetryLogRecord {
    mcp_server_init_record(McpServerInit::from_name(server_name, init_failure_reason))
}

#[derive(Clone, Debug)]
pub struct SubagentInvokedBuilder {
    builder: LogBuilder,
}

impl SubagentInvokedBuilder {
    pub fn depth_bucket(mut self, depth_bucket: Option<SubagentDepthBucket>) -> Self {
        if let Some(depth_bucket) = depth_bucket {
            self.builder = self.builder.attribute("depth_bucket", depth_bucket.as_str());
        }
        self
    }

    pub fn model_class(mut self, model_class: Option<ModelClass>) -> Self {
        if let Some(model_class) = model_class {
            self.builder = self.builder.attribute("model_class", model_class.as_str());
        }
        self
    }

    pub fn build(self) -> TelemetryLogRecord {
        self.builder.expect_valid()
    }
}

pub fn subagent_invoked(subagent_name: impl Into<String>) -> SubagentInvokedBuilder {
    SubagentInvokedBuilder {
        builder: event("kiro_cli_subagent_invoked").attribute("subagent_name", subagent_name),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UserTurnCompletedLog<'a> {
    pub conversation_id: &'a str,
    pub result: ResultKind,
    pub is_subagent: bool,
    pub user_prompt_length: i64,
    pub assistant_response_length: i64,
    pub user_turn_duration_seconds: i64,
    pub follow_up_count: i64,
    pub request_id: Option<&'a str>,
    pub message_id: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub client_application: Option<&'a str>,
    pub turn_failure_reason: Option<&'a str>,
    pub reason_desc: Option<&'a str>,
    pub status_code: Option<u16>,
    pub time_to_first_chunks_ms: Option<&'a str>,
    pub message_meta_tags: Option<&'a str>,
    pub parent_tool_use_id: Option<&'a str>,
    pub request_attempts: Option<u32>,
    pub total_tokens: Option<i64>,
    pub uncached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_write_input_tokens: Option<i64>,
    pub estimated_cost_usd: Option<f64>,
}

impl<'a> UserTurnCompletedLog<'a> {
    pub const fn new(
        conversation_id: &'a str,
        result: ResultKind,
        is_subagent: bool,
        user_prompt_length: i64,
        assistant_response_length: i64,
        user_turn_duration_seconds: i64,
        follow_up_count: i64,
    ) -> Self {
        Self {
            conversation_id,
            result,
            is_subagent,
            user_prompt_length,
            assistant_response_length,
            user_turn_duration_seconds,
            follow_up_count,
            request_id: None,
            message_id: None,
            model_id: None,
            client_application: None,
            turn_failure_reason: None,
            reason_desc: None,
            status_code: None,
            time_to_first_chunks_ms: None,
            message_meta_tags: None,
            parent_tool_use_id: None,
            request_attempts: None,
            total_tokens: None,
            uncached_input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_write_input_tokens: None,
            estimated_cost_usd: None,
        }
    }

    pub const fn request_id(mut self, request_id: Option<&'a str>) -> Self {
        self.request_id = request_id;
        self
    }

    pub const fn message_id(mut self, message_id: Option<&'a str>) -> Self {
        self.message_id = message_id;
        self
    }

    pub const fn model_id(mut self, model_id: Option<&'a str>) -> Self {
        self.model_id = model_id;
        self
    }

    pub const fn client_application(mut self, client_application: Option<&'a str>) -> Self {
        self.client_application = client_application;
        self
    }

    pub const fn turn_failure_reason(mut self, turn_failure_reason: Option<&'a str>) -> Self {
        self.turn_failure_reason = turn_failure_reason;
        self
    }

    pub const fn reason_desc(mut self, reason_desc: Option<&'a str>) -> Self {
        self.reason_desc = reason_desc;
        self
    }

    pub const fn status_code(mut self, status_code: Option<u16>) -> Self {
        self.status_code = status_code;
        self
    }

    pub const fn time_to_first_chunks_ms(mut self, time_to_first_chunks_ms: Option<&'a str>) -> Self {
        self.time_to_first_chunks_ms = time_to_first_chunks_ms;
        self
    }

    pub const fn message_meta_tags(mut self, message_meta_tags: Option<&'a str>) -> Self {
        self.message_meta_tags = message_meta_tags;
        self
    }

    pub const fn parent_tool_use_id(mut self, parent_tool_use_id: Option<&'a str>) -> Self {
        self.parent_tool_use_id = parent_tool_use_id;
        self
    }

    pub const fn request_attempts(mut self, request_attempts: Option<u32>) -> Self {
        self.request_attempts = request_attempts;
        self
    }

    pub const fn total_tokens(mut self, total_tokens: Option<i64>) -> Self {
        self.total_tokens = total_tokens;
        self
    }

    pub const fn uncached_input_tokens(mut self, uncached_input_tokens: Option<i64>) -> Self {
        self.uncached_input_tokens = uncached_input_tokens;
        self
    }

    pub const fn output_tokens(mut self, output_tokens: Option<i64>) -> Self {
        self.output_tokens = output_tokens;
        self
    }

    pub const fn cache_read_input_tokens(mut self, cache_read_input_tokens: Option<i64>) -> Self {
        self.cache_read_input_tokens = cache_read_input_tokens;
        self
    }

    pub const fn cache_write_input_tokens(mut self, cache_write_input_tokens: Option<i64>) -> Self {
        self.cache_write_input_tokens = cache_write_input_tokens;
        self
    }

    pub const fn estimated_cost_usd(mut self, estimated_cost_usd: Option<f64>) -> Self {
        self.estimated_cost_usd = estimated_cost_usd;
        self
    }
}

#[derive(Clone, Debug)]
pub struct UserTurnCompletedBuilder {
    builder: LogBuilder,
}

impl UserTurnCompletedBuilder {
    pub fn request_id(self, value: Option<impl Into<String>>) -> Self {
        self.optional_attribute("request_id", value)
    }

    pub fn message_id(self, value: Option<impl Into<String>>) -> Self {
        self.optional_attribute("message_id", value)
    }

    pub fn model_class(self, model_class: Option<ModelClass>) -> Self {
        self.optional_attribute("model_class", model_class.map(|value| value.as_str()))
    }

    pub fn model_id(self, value: Option<impl AsRef<str>>) -> Self {
        self.model_class(value.map(|value| ModelClass::from_model_id(Some(value.as_ref()))))
    }

    pub fn client_application(self, client_application: Option<ClientApplication>) -> Self {
        self.optional_attribute("client_application", client_application.map(|value| value.as_str()))
    }

    pub fn client_application_name(self, value: Option<impl AsRef<str>>) -> Self {
        self.client_application(value.map(|value| ClientApplication::from_name(Some(value.as_ref()))))
    }

    pub fn turn_failure_reason(self, value: Option<impl Into<String>>) -> Self {
        self.optional_attribute("turn_failure_reason", value)
    }

    pub fn reason_desc(self, value: Option<impl Into<String>>) -> Self {
        self.optional_attribute("reason_desc", value)
    }

    pub fn status_code(self, value: Option<u16>) -> Self {
        self.optional_attribute("status_code", value.map(|value| value.to_string()))
    }

    pub fn time_to_first_chunks_ms(self, value: Option<impl Into<String>>) -> Self {
        self.optional_attribute("time_to_first_chunks_ms", value)
    }

    pub fn message_meta_tags(self, value: Option<impl Into<String>>) -> Self {
        self.optional_attribute("message_meta_tags", value)
    }

    pub fn parent_tool_use_id(self, value: Option<impl Into<String>>) -> Self {
        self.optional_attribute("parent_tool_use_id", value)
    }

    pub fn request_attempts(self, value: Option<u32>) -> Self {
        self.optional_attribute("request_attempts", value.map(|value| value.to_string()))
    }

    pub fn total_tokens(self, value: Option<i64>) -> Self {
        self.optional_attribute("total_tokens", value.map(|value| value.to_string()))
    }

    pub fn uncached_input_tokens(self, value: Option<i64>) -> Self {
        self.optional_attribute("uncached_input_tokens", value.map(|value| value.to_string()))
    }

    pub fn output_tokens(self, value: Option<i64>) -> Self {
        self.optional_attribute("output_tokens", value.map(|value| value.to_string()))
    }

    pub fn cache_read_input_tokens(self, value: Option<i64>) -> Self {
        self.optional_attribute("cache_read_input_tokens", value.map(|value| value.to_string()))
    }

    pub fn cache_write_input_tokens(self, value: Option<i64>) -> Self {
        self.optional_attribute("cache_write_input_tokens", value.map(|value| value.to_string()))
    }

    pub fn estimated_cost_usd(self, value: Option<f64>) -> Self {
        self.optional_attribute("estimated_cost_usd", value.map(|value| format!("{value:.9}")))
    }

    pub fn build(self) -> TelemetryLogRecord {
        self.builder.expect_valid()
    }

    fn optional_attribute(mut self, key: &str, value: Option<impl Into<String>>) -> Self {
        if let Some(value) = value {
            self.builder = self.builder.attribute(key, value);
        }
        self
    }
}

pub fn user_turn_completed(
    conversation_id: impl Into<String>,
    result: ResultKind,
    is_subagent: bool,
    user_prompt_length: i64,
    assistant_response_length: i64,
    user_turn_duration_seconds: i64,
    follow_up_count: i64,
) -> UserTurnCompletedBuilder {
    UserTurnCompletedBuilder {
        builder: event("kiro_cli_user_turn_completed")
            .attribute("conversation_id", conversation_id)
            .attribute("result", result.as_str())
            .attribute("is_subagent", is_subagent.to_string())
            .attribute("user_prompt_length", user_prompt_length.to_string())
            .attribute("assistant_response_length", assistant_response_length.to_string())
            .attribute("user_turn_duration_seconds", user_turn_duration_seconds.to_string())
            .attribute("follow_up_count", follow_up_count.to_string()),
    }
}

pub fn user_turn_completed_record(input: UserTurnCompletedLog<'_>) -> TelemetryLogRecord {
    user_turn_completed(
        input.conversation_id,
        input.result,
        input.is_subagent,
        input.user_prompt_length,
        input.assistant_response_length,
        input.user_turn_duration_seconds,
        input.follow_up_count,
    )
    .request_id(input.request_id)
    .message_id(input.message_id)
    .model_id(input.model_id)
    .client_application_name(input.client_application)
    .turn_failure_reason(input.turn_failure_reason)
    .reason_desc(input.reason_desc)
    .status_code(input.status_code)
    .time_to_first_chunks_ms(input.time_to_first_chunks_ms)
    .message_meta_tags(input.message_meta_tags)
    .parent_tool_use_id(input.parent_tool_use_id)
    .request_attempts(input.request_attempts)
    .total_tokens(input.total_tokens)
    .uncached_input_tokens(input.uncached_input_tokens)
    .output_tokens(input.output_tokens)
    .cache_read_input_tokens(input.cache_read_input_tokens)
    .cache_write_input_tokens(input.cache_write_input_tokens)
    .estimated_cost_usd(input.estimated_cost_usd)
    .build()
}

/// §5.1 dim table joining the anonymous client id to install metadata. All
/// fields are log-only (never metric dimensions).
pub fn client_identity(
    anonymous_client_id: impl Into<String>,
    install_method: impl Into<String>,
    install_date_epoch_day: u32,
    is_internal_amazon: bool,
) -> TelemetryLogRecord {
    event("kiro_cli_client_identity")
        .attribute("anonymous_client_id", anonymous_client_id)
        .attribute("install_method", install_method)
        .attribute("install_date_epoch_day", install_date_epoch_day.to_string())
        .attribute("is_internal_amazon", is_internal_amazon.to_string())
        .expect_valid()
}

/// §5.2 activation funnel fact row: the first time a client used a feature.
pub fn feature_first_use(
    anonymous_client_id: impl Into<String>,
    feature_name: impl Into<String>,
    session_id: impl Into<String>,
    trigger: impl Into<String>,
) -> TelemetryLogRecord {
    event("kiro_cli_feature_first_use")
        .attribute("anonymous_client_id", anonymous_client_id)
        .attribute("feature_name", feature_name)
        .attribute("session_id", session_id)
        .attribute("trigger", trigger)
        .expect_valid()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::expect_log_attrs;

    fn assert_log_shape(record: TelemetryLogRecord, name: &str, attributes: &[(&str, &str)]) {
        assert_eq!(record.name, name);
        expect_log_attrs(&record, attributes);
    }

    #[test]
    fn log_builder_validates_schema_kind_and_attributes() {
        let record = event("kiro_cli_conversation_completed")
            .attribute("session_id", "session-1")
            .attribute("conversation_id", "conversation-1")
            .attribute("completion_reason", "stop")
            .build()
            .expect("valid log event");
        assert_eq!(record.name, "kiro_cli_conversation_completed");

        let error = event("chat_cli.session.completed")
            .build()
            .expect_err("metric is not a log event");
        assert_eq!(error, MetricBuildError::KindMismatch {
            metric: "chat_cli.session.completed".to_string(),
            expected: MetricKind::Counter,
            actual: MetricKind::LogEvent,
        });

        let error = event("kiro_cli_conversation_completed")
            .attribute("exit_reason", "clean")
            .build()
            .expect_err("unsupported log attribute");
        assert_eq!(error, MetricBuildError::UnsupportedAttribute {
            metric: "kiro_cli_conversation_completed".to_string(),
            attribute: "exit_reason".to_string(),
        });

        let error = event("kiro_cli_conversation_completed")
            .attribute("session_id", "session-1")
            .attribute("session_id", "session-2")
            .build()
            .expect_err("duplicate log attribute");
        assert_eq!(error, MetricBuildError::DuplicateAttribute {
            metric: "kiro_cli_conversation_completed".to_string(),
            attribute: "session_id".to_string(),
        });

        let error = event("kiro_cli_conversation_completed")
            .attribute("completion_reason", "not_a_reason")
            .build()
            .expect_err("invalid closed enum value");
        assert_eq!(error, MetricBuildError::InvalidAttributeValue {
            metric: "kiro_cli_conversation_completed".to_string(),
            attribute: "completion_reason".to_string(),
            value: "not_a_reason".to_string(),
        });
    }

    #[test]
    fn conversation_completed_builds_fact_log() {
        let record = conversation_completed("session-1", "conversation-1", CompletionReason::Stop);

        assert_log_shape(record, "kiro_cli_conversation_completed", &[
            ("session_id", "session-1"),
            ("conversation_id", "conversation-1"),
            ("completion_reason", "stop"),
        ]);
    }

    #[test]
    fn client_identity_builds_fact_log() {
        let record = client_identity("client-uuid", "brew", 20_000, true);

        assert_log_shape(record, "kiro_cli_client_identity", &[
            ("anonymous_client_id", "client-uuid"),
            ("install_method", "brew"),
            ("install_date_epoch_day", "20000"),
            ("is_internal_amazon", "true"),
        ]);
    }

    #[test]
    fn feature_first_use_builds_fact_log() {
        let record = feature_first_use("client-uuid", "tangent_mode", "session-1", "slash_command");

        assert_log_shape(record, "kiro_cli_feature_first_use", &[
            ("anonymous_client_id", "client-uuid"),
            ("feature_name", "tangent_mode"),
            ("session_id", "session-1"),
            ("trigger", "slash_command"),
        ]);
    }

    #[test]
    fn metering_event_builds_fact_log() {
        let record = metering_event(
            Some("request-1"),
            Some(ModelClass::AnthropicSonnet),
            Some(ClientApplication::ChatCliV2),
            42.0,
            "token",
            "tokens",
        );

        assert_log_shape(record, "kiro_cli_metering_event", &[
            ("request_id", "request-1"),
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v2"),
            ("metering_usage", "42"),
            ("metering_unit", "token"),
            ("metering_unit_plural", "tokens"),
        ]);

        let record = metering_event_from_names(
            Some("request-1"),
            Some("claude-4-sonnet"),
            Some("chat_cli_v2"),
            42.0,
            "token",
            "tokens",
        );
        assert_log_shape(record, "kiro_cli_metering_event", &[
            ("request_id", "request-1"),
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v2"),
            ("metering_usage", "42"),
            ("metering_unit", "token"),
            ("metering_unit_plural", "tokens"),
        ]);

        let record = metering_event_record(MeteringEventLog::from_names(
            Some("request-1"),
            Some("claude-4-sonnet"),
            Some("kas"),
            42.0,
            "token",
            "tokens",
        ));
        assert_log_shape(record, "kiro_cli_metering_event", &[
            ("request_id", "request-1"),
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v3"),
            ("metering_usage", "42"),
            ("metering_unit", "token"),
            ("metering_unit_plural", "tokens"),
        ]);
    }

    #[test]
    fn tool_invoked_builds_fact_log() {
        let record = tool_invoked(
            Some("tool-1"),
            Some("fs_read"),
            Some("filesystem"),
            Some(true),
            Some(ModelClass::AnthropicHaiku),
            Some(12.3456),
        );

        assert_log_shape(record, "kiro_cli_tool_invoked", &[
            ("tool_use_id", "tool-1"),
            ("tool_name", "fs_read"),
            ("mcp_server_name", "filesystem"),
            ("is_success", "true"),
            ("model_class", "anthropic_haiku"),
            ("execution_duration_ms", "12.346"),
        ]);

        let record = tool_invoked_from_names(
            Some("tool-1"),
            Some("fs_read"),
            Some("filesystem"),
            Some(true),
            Some("claude-4-haiku"),
            Some(12.3456),
        );
        assert_log_shape(record, "kiro_cli_tool_invoked", &[
            ("tool_use_id", "tool-1"),
            ("tool_name", "fs_read"),
            ("mcp_server_name", "filesystem"),
            ("is_success", "true"),
            ("model_class", "anthropic_haiku"),
            ("execution_duration_ms", "12.346"),
        ]);

        let record = tool_invoked_record(ToolInvokedLog::from_names(
            Some("tool-1"),
            Some("fs_read"),
            Some("filesystem"),
            Some(true),
            Some("claude-4-haiku"),
            Some(Duration::from_micros(12_346)),
        ));
        assert_log_shape(record, "kiro_cli_tool_invoked", &[
            ("tool_use_id", "tool-1"),
            ("tool_name", "fs_read"),
            ("mcp_server_name", "filesystem"),
            ("is_success", "true"),
            ("model_class", "anthropic_haiku"),
            ("execution_duration_ms", "12.346"),
        ]);

        let record = tool_invoked_record(ToolInvokedLog {
            tool_name: Some("fs_read"),
            execution_duration: Some(Duration::ZERO),
            ..ToolInvokedLog::new()
        });
        assert_log_shape(record, "kiro_cli_tool_invoked", &[("tool_name", "fs_read")]);
    }

    #[test]
    fn mcp_server_init_builds_fact_log() {
        let record = mcp_server_init("awslabs.tools", McpServerClass::OfficialThirdParty, Outcome::Success);

        assert_log_shape(record, "kiro_cli_mcp_server_init", &[
            ("mcp_server_name", "awslabs.tools"),
            ("mcp_server_class", "official_third_party"),
            ("outcome", "success"),
        ]);

        let record = mcp_server_init_from_name("local-server", Some("request timed out"));
        assert_log_shape(record, "kiro_cli_mcp_server_init", &[
            ("mcp_server_name", "local-server"),
            ("mcp_server_class", "user_defined"),
            ("outcome", "timeout"),
        ]);

        let record = mcp_server_init_record(McpServerInit::from_name("amzn-internal", Some("auth denied")));
        assert_log_shape(record, "kiro_cli_mcp_server_init", &[
            ("mcp_server_name", "amzn-internal"),
            ("mcp_server_class", "internal_amazon"),
            ("outcome", "auth"),
        ]);
    }

    #[test]
    fn subagent_invoked_builder_adds_schema_attributes() {
        let record = subagent_invoked("code-review")
            .depth_bucket(Some(SubagentDepthBucket::ThreePlus))
            .model_class(Some(ModelClass::AnthropicSonnet))
            .build();

        assert_log_shape(record, "kiro_cli_subagent_invoked", &[
            ("subagent_name", "code-review"),
            ("depth_bucket", "3+"),
            ("model_class", "anthropic_sonnet"),
        ]);
    }

    #[test]
    fn user_turn_completed_builder_adds_optional_fact_fields() {
        let record = user_turn_completed("conversation-1", ResultKind::Success, false, 12, 34, 56, 2)
            .request_id(Some("request-1"))
            .message_id(Some("message-1"))
            .model_id(Some("gpt-5-codex"))
            .client_application_name(Some("chat_cli_v2"))
            .status_code(Some(200))
            .estimated_cost_usd(Some(0.000_123_456))
            .build();

        assert_log_shape(record, "kiro_cli_user_turn_completed", &[
            ("conversation_id", "conversation-1"),
            ("result", "success"),
            ("is_subagent", "false"),
            ("user_prompt_length", "12"),
            ("assistant_response_length", "34"),
            ("user_turn_duration_seconds", "56"),
            ("follow_up_count", "2"),
            ("request_id", "request-1"),
            ("message_id", "message-1"),
            ("model_class", "openai_gpt5"),
            ("client_application", "chat_cli_v2"),
            ("status_code", "200"),
            ("estimated_cost_usd", "0.000123456"),
        ]);
    }

    #[test]
    fn user_turn_completed_record_builds_from_typed_input() {
        let input = UserTurnCompletedLog::new("conversation-1", ResultKind::Failed, true, 12, 34, 56, 2)
            .request_id(Some("request-1,request-2"))
            .message_id(Some("message-1"))
            .model_id(Some("claude-4-sonnet"))
            .client_application(Some("kas"))
            .turn_failure_reason(Some("ServiceFailure"))
            .reason_desc(Some("redacted"))
            .status_code(Some(500))
            .time_to_first_chunks_ms(Some("25.000,null"))
            .message_meta_tags(Some("Compact"))
            .parent_tool_use_id(Some("parent-tool"))
            .request_attempts(Some(2))
            .total_tokens(Some(17))
            .uncached_input_tokens(Some(10))
            .output_tokens(Some(5))
            .cache_read_input_tokens(Some(2))
            .cache_write_input_tokens(Some(3))
            .estimated_cost_usd(Some(0.000_123_456));

        let record = user_turn_completed_record(input);

        assert_log_shape(record, "kiro_cli_user_turn_completed", &[
            ("conversation_id", "conversation-1"),
            ("result", "failed"),
            ("is_subagent", "true"),
            ("user_prompt_length", "12"),
            ("assistant_response_length", "34"),
            ("user_turn_duration_seconds", "56"),
            ("follow_up_count", "2"),
            ("request_id", "request-1,request-2"),
            ("message_id", "message-1"),
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v3"),
            ("turn_failure_reason", "ServiceFailure"),
            ("reason_desc", "redacted"),
            ("status_code", "500"),
            ("time_to_first_chunks_ms", "25.000,null"),
            ("message_meta_tags", "Compact"),
            ("parent_tool_use_id", "parent-tool"),
            ("request_attempts", "2"),
            ("total_tokens", "17"),
            ("uncached_input_tokens", "10"),
            ("output_tokens", "5"),
            ("cache_read_input_tokens", "2"),
            ("cache_write_input_tokens", "3"),
            ("estimated_cost_usd", "0.000123456"),
        ]);
    }
}
