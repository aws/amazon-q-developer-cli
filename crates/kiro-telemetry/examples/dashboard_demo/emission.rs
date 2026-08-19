use kiro_telemetry::metric::{
    AuthOperation,
    McpInitOutcome,
    ModelRequestOutcome,
    OsType,
    SessionInterface,
    TokenType,
    ToolMetric,
    ToolMetricOrigin,
    record_auth_failure,
    record_chat_session_started,
    record_cloud_session_lifecycle,
    record_cloud_session_ready,
    record_crash,
    record_credits_consumed,
    record_daily_heartbeat,
    record_goal_outcome,
    record_login_success,
    record_mcp_server_init,
    record_mcp_tools_token_count_estimate,
    record_model_invocation,
    record_model_request_duration_seconds,
    record_model_request_failure,
    record_model_time_to_first_content_ms,
    record_process_cpu_utilization_ratio,
    record_process_handle_count,
    record_process_memory_rss_bytes,
    record_process_open_file_descriptor_count,
    record_process_peak_rss_bytes,
    record_process_thread_count,
    record_run_outcome,
    record_run_started,
    record_slash_command,
    record_startup_duration_seconds,
    record_telemetry_export_dropped,
    record_time_to_first_visible_response_ms,
    record_tokens_consumed,
    record_tool_call,
    record_tool_execution_duration_ms,
    record_top_level_command,
    record_tui_event_loop_delay_p99_seconds,
    record_tui_heap_used_bytes,
    record_tui_input_to_render_p95_seconds,
    record_tui_render_duration_seconds,
    record_turn_cancelled,
    record_turn_failure,
    record_ui_mode_session_started,
    record_user_turn,
    record_user_turn_duration_seconds,
};
use kiro_telemetry::{
    MetricRecord,
    MetricValue,
};

use crate::scenarios::{
    DiagnosticPolicy,
    SCENARIOS,
    Scenario,
};

const MEBIBYTE: f64 = 1024.0 * 1024.0;
const SAMPLE_SCALE_STEP: f64 = 0.05;

pub fn activity_records() -> Vec<MetricRecord> {
    let mut records = Vec::new();
    for scenario in SCENARIOS {
        ScenarioEmitter::new(&mut records, *scenario).emit_activity();
    }
    records
}

pub fn heartbeat_records() -> Vec<MetricRecord> {
    SCENARIOS
        .iter()
        .map(|scenario| {
            with_version(
                record_daily_heartbeat(scenario.release_channel, scenario.os_type, scenario.install_method),
                scenario.version,
            )
        })
        .collect()
}

pub fn heartbeat_baselines() -> Vec<MetricRecord> {
    heartbeat_records()
        .into_iter()
        .map(|mut record| {
            record.value = MetricValue::Counter(0);
            record
        })
        .collect()
}

struct ScenarioEmitter<'a> {
    records: &'a mut Vec<MetricRecord>,
    scenario: Scenario,
}

impl<'a> ScenarioEmitter<'a> {
    fn new(records: &'a mut Vec<MetricRecord>, scenario: Scenario) -> Self {
        Self { records, scenario }
    }

    fn emit_activity(&mut self) {
        let scenario = self.scenario;
        self.emit(record_cloud_session_lifecycle(scenario.activity.cloud_event));
        self.emit_some(record_cloud_session_ready(scenario.activity.cloud_ready_seconds));

        if scenario.session_interface == SessionInterface::InteractiveCli {
            self.emit(record_ui_mode_session_started(scenario.ui_mode));
        }

        for sample in 0..scenario.activity.volume {
            self.emit_sample(sample);
        }
        self.emit_diagnostics();
    }

    fn emit_sample(&mut self, sample: u64) {
        let scenario = self.scenario;
        let scale = scenario.activity.sample_scale_base + sample as f64 * SAMPLE_SCALE_STEP;
        let tool = ToolMetric::new(
            scenario.engine,
            scenario.tool_origin,
            scenario.tool_outcome,
            scenario.execution_context,
        );
        let tool = if scenario.tool_origin == ToolMetricOrigin::Builtin {
            tool.builtin_tool_name(Some("fs_read"))
        } else {
            tool
        };

        self.emit(record_run_started(
            scenario.session_interface,
            scenario.engine,
            scenario.os_type,
        ));
        self.emit(record_login_success(scenario.auth_method, scenario.auth_flow));
        self.emit(record_chat_session_started(
            scenario.session_interface,
            scenario.agent_mode,
            scenario.engine,
            scenario.trust_posture,
        ));
        self.emit(record_slash_command(scenario.commands.slash.as_str(), scenario.engine));
        self.emit(record_top_level_command(scenario.commands.top_level.as_str()));
        self.emit(record_tool_call(tool));
        self.emit(record_model_invocation(scenario.engine, Some(scenario.model)));
        self.emit_some(record_model_time_to_first_content_ms(
            120.0 * scale,
            scenario.engine,
            Some(scenario.model),
        ));
        if scenario.session_interface != SessionInterface::ExternalAcp {
            self.emit_some(record_time_to_first_visible_response_ms(
                180.0 * scale,
                scenario.session_interface,
                scenario.agent_mode,
                scenario.engine,
            ));
        }
        self.emit_some(record_model_request_duration_seconds(
            0.9 * scale,
            scenario.engine,
            Some(scenario.model),
            ModelRequestOutcome::Success,
        ));
        self.emit_some(record_user_turn_duration_seconds(
            1.4 * scale,
            scenario.session_interface,
            scenario.agent_mode,
            scenario.engine,
        ));
        self.emit(record_user_turn(
            scenario.session_interface,
            scenario.agent_mode,
            scenario.engine,
        ));
        self.emit(record_run_outcome(
            scenario.session_interface,
            scenario.engine,
            scenario.os_type,
            scenario.activity.run_outcome,
        ));
        self.emit_some(record_startup_duration_seconds(
            0.25 * scale,
            scenario.session_interface,
            scenario.engine,
            scenario.os_type,
        ));
        self.emit_resource_metrics(scale);
        self.emit_token_metrics(sample, scale, tool);
        self.emit(record_mcp_server_init(
            scenario.engine,
            scenario.mcp.source,
            scenario.mcp.outcome,
            Some(scenario.mcp.server.as_str()),
        ));
        if scenario.mcp.outcome == McpInitOutcome::Success {
            self.emit(record_mcp_tools_token_count_estimate(
                128 + sample * 16,
                scenario.engine,
                scenario.mcp.source,
                Some(scenario.mcp.server.as_str()),
            ));
        }
        self.emit(record_goal_outcome(scenario.engine, scenario.activity.goal_outcome));
        self.emit_tui_metrics(scale);
    }

    fn emit_resource_metrics(&mut self, scale: f64) {
        let scenario = self.scenario;
        let resources = scenario.activity.resources;
        self.emit_some(record_process_memory_rss_bytes(
            resources.rss_mebibytes * MEBIBYTE,
            scenario.os_type,
            scenario.engine,
            scenario.process_role,
        ));
        self.emit_some(record_process_cpu_utilization_ratio(
            0.08 * scale,
            scenario.os_type,
            scenario.engine,
            scenario.process_role,
        ));
        match scenario.os_type {
            OsType::Windows => self.emit_some(record_process_handle_count(
                resources.windows_handles,
                scenario.engine,
                scenario.process_role,
            )),
            _ => self.emit_some(record_process_open_file_descriptor_count(
                resources.open_file_descriptors,
                scenario.os_type,
                scenario.engine,
                scenario.process_role,
            )),
        }
        self.emit_some(record_process_thread_count(
            resources.threads,
            scenario.os_type,
            scenario.engine,
            scenario.process_role,
        ));
        self.emit_some(record_process_peak_rss_bytes(
            resources.peak_rss_mebibytes * MEBIBYTE,
            scenario.os_type,
            scenario.engine,
            scenario.process_role,
        ));
    }

    fn emit_token_metrics(&mut self, sample: u64, scale: f64, tool: ToolMetric<'_>) {
        let scenario = self.scenario;
        for (token_type, amount) in [
            (TokenType::InputUncached, 900),
            (TokenType::InputCacheRead, 600),
            (TokenType::Output, 350),
            (TokenType::Reasoning, 120),
        ] {
            self.emit_some(record_tokens_consumed(
                amount + sample * 10,
                scenario.engine,
                Some(scenario.model),
                token_type,
            ));
        }
        self.emit_some(record_credits_consumed(0.4 * scale, Some(scenario.model)));
        self.emit_some(record_tool_execution_duration_ms(35.0 * scale, tool));
    }

    fn emit_tui_metrics(&mut self, scale: f64) {
        let scenario = self.scenario;
        if scenario.session_interface != SessionInterface::InteractiveCli {
            return;
        }

        self.emit_some(record_tui_heap_used_bytes(
            scenario.activity.resources.tui_heap_mebibytes * MEBIBYTE,
            scenario.os_type,
            scenario.engine,
        ));
        self.emit_some(record_tui_event_loop_delay_p99_seconds(
            0.006 * scale,
            scenario.os_type,
            scenario.engine,
        ));
        self.emit_some(record_tui_input_to_render_p95_seconds(
            0.004 * scale,
            scenario.os_type,
            scenario.engine,
        ));
        self.emit_some(record_tui_render_duration_seconds(
            0.003 * scale,
            scenario.os_type,
            scenario.engine,
            scenario.activity.render_kind,
        ));
    }

    fn emit_diagnostics(&mut self) {
        let scenario = self.scenario;
        match scenario.diagnostics {
            DiagnosticPolicy::None => {},
            DiagnosticPolicy::Failure(diagnostics) => {
                self.emit(record_model_request_failure(
                    scenario.engine,
                    Some(scenario.model),
                    diagnostics.error_kind,
                ));
                self.emit_some(record_model_request_duration_seconds(
                    diagnostics.request_duration_seconds,
                    scenario.engine,
                    Some(scenario.model),
                    ModelRequestOutcome::Failure,
                ));
                self.emit(record_turn_failure(
                    scenario.session_interface,
                    scenario.agent_mode,
                    scenario.engine,
                    diagnostics.turn_failure_reason,
                ));
                self.emit(record_crash(
                    scenario.engine,
                    scenario.os_type,
                    scenario.process_role,
                    diagnostics.crash_kind,
                ));
                self.emit(record_auth_failure(
                    scenario.auth_method,
                    scenario.auth_flow,
                    AuthOperation::Login,
                    diagnostics.auth_failure_reason,
                ));
            },
            DiagnosticPolicy::Cancellation {
                request_duration_seconds,
            } => {
                self.emit(record_turn_cancelled(
                    scenario.session_interface,
                    scenario.agent_mode,
                    scenario.engine,
                ));
                self.emit_some(record_model_request_duration_seconds(
                    request_duration_seconds,
                    scenario.engine,
                    Some(scenario.model),
                    ModelRequestOutcome::Cancelled,
                ));
            },
            DiagnosticPolicy::TelemetryProtection {
                dropped_records,
                drop_reason,
            } => {
                self.emit_some(record_telemetry_export_dropped(dropped_records, drop_reason));
            },
        }
    }

    fn emit(&mut self, record: MetricRecord) {
        self.records.push(with_version(record, self.scenario.version));
    }

    fn emit_some(&mut self, record: Option<MetricRecord>) {
        self.emit(record.unwrap_or_else(|| {
            panic!(
                "{} demo values satisfy constructor bounds",
                self.scenario.identity.as_str()
            )
        }));
    }
}

fn with_version(mut record: MetricRecord, version: &str) -> MetricRecord {
    record
        .attributes
        .iter_mut()
        .find(|attribute| attribute.key == "version_full")
        .expect("every dashboard metric has version_full")
        .value = version.to_string();
    record
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_plans_preserve_the_emitted_record_count() {
        assert_eq!(activity_records().len(), 1_062);
        assert_eq!(heartbeat_records().len(), 7);
    }

    #[test]
    fn heartbeat_baselines_preserve_identity_without_incrementing() {
        let increments = heartbeat_records();
        let baselines = heartbeat_baselines();
        assert_eq!(increments.len(), SCENARIOS.len());
        assert_eq!(baselines.len(), increments.len());
        assert!(increments.iter().all(|record| record.value == MetricValue::Counter(1)));
        assert!(baselines.iter().all(|record| record.value == MetricValue::Counter(0)));
        assert!(increments.iter().zip(&baselines).all(|(increment, baseline)| {
            increment.name == baseline.name && increment.attributes == baseline.attributes
        }));
    }

    #[test]
    fn every_record_uses_a_declared_scenario_version() {
        let records = activity_records().into_iter().chain(heartbeat_records());
        assert!(records.into_iter().all(|record| {
            let version = record
                .attributes
                .iter()
                .find(|attribute| attribute.key == "version_full")
                .map(|attribute| attribute.value.as_str());
            SCENARIOS.iter().any(|scenario| Some(scenario.version) == version)
        }));
    }
}
