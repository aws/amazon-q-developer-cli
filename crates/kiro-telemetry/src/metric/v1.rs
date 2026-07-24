use super::*;

fn for_engine(record: MetricRecord, engine: Engine) -> MetricRecord {
    super::with_engine(record, engine)
}

pub fn user_logged_in_for_engine(
    client_application: ClientApplication,
    credential_kind: CredentialKind,
    install_method: InstallSource,
    engine: Engine,
) -> MetricRecord {
    for_engine(
        user_logged_in(client_application, credential_kind).with_attribute("install_method", install_method.as_str()),
        engine,
    )
}

pub fn daily_heartbeat_for_engine(
    client_application: ClientApplication,
    install_method: InstallSource,
    credential_kind: CredentialKind,
    engine: Engine,
) -> MetricRecord {
    for_engine(
        daily_heartbeat(client_application, install_method).with_attribute("credential_kind", credential_kind.as_str()),
        engine,
    )
}

pub fn auth_credential_failure_for_engine(
    auth_provider: AuthProvider,
    auth_flow: AuthFlow,
    error_code: &str,
    operation: Operation,
    partition: Partition,
    result: ResultKind,
    engine: Engine,
) -> MetricRecord {
    for_engine(
        auth_credential_failure(auth_provider, error_code, operation, partition)
            .with_attribute("auth_flow", auth_flow.as_str())
            .with_attribute("result", result.as_str()),
        engine,
    )
}

pub fn feature_used_for_engine(
    feature: &str,
    subcommand: Option<&str>,
    result: ResultKind,
    engine: Engine,
) -> MetricRecord {
    for_engine(
        feature_used(feature)
            .with_attribute("subcommand", normalized_dynamic_name(subcommand.unwrap_or("none")))
            .with_attribute("result", result.as_str()),
        engine,
    )
}

pub fn slash_command_invoked_for_engine(
    command: &str,
    subcommand: Option<&str>,
    result: ResultKind,
    engine: Engine,
) -> MetricRecord {
    for_engine(
        slash_command_invoked(command)
            .with_attribute("subcommand", normalized_dynamic_name(subcommand.unwrap_or("none")))
            .with_attribute("result", result.as_str()),
        engine,
    )
}

pub fn tangent_duration_seconds(seconds: f64, result: ResultKind, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_tangent_duration_seconds", seconds)
        .attribute("result", result.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn tangent_entries_removed(entries: f64, result: ResultKind, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_tangent_entries_removed", entries)
        .attribute("result", result.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

#[derive(Clone, Copy, Debug)]
pub struct TangentSessionMetrics {
    pub result: ResultKind,
    pub duration_seconds: i64,
    pub is_forget: bool,
    pub entries_removed: Option<i64>,
    pub engine: Engine,
}

pub fn tangent_session_records(input: TangentSessionMetrics) -> Vec<MetricRecord> {
    let mut records = vec![slash_command_invoked_for_engine(
        "tangent",
        Some(if input.is_forget { "forget" } else { "exit" }),
        input.result,
        input.engine,
    )];
    if input.is_forget {
        if let Some(entries_removed) = input.entries_removed {
            records.push(tangent_entries_removed(
                entries_removed.max(0) as f64,
                input.result,
                input.engine,
            ));
        }
    } else {
        records.push(tangent_duration_seconds(
            input.duration_seconds.max(0) as f64,
            input.result,
            input.engine,
        ));
    }
    records
}

pub fn chat_session_started_for_engine(
    mode: Mode,
    client_application: ClientApplication,
    session_start_kind: SessionStartKind,
    model: Option<&str>,
    engine: Engine,
) -> MetricRecord {
    for_engine(
        chat_session_started(mode, client_application)
            .with_attribute("session_start_kind", session_start_kind.as_str())
            .with_attribute("model", model_attr(model)),
        engine,
    )
}

pub fn conversation_completed_total(model: Option<&str>, engine: Engine) -> MetricRecord {
    counter("kiro_cli_conversation_completed_total", 1)
        .attribute("model", model_attr(model))
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn chat_messages_total(
    model: Option<&str>,
    result: ResultKind,
    message_kind: MessageKind,
    engine: Engine,
) -> MetricRecord {
    counter("kiro_cli_chat_messages_total", 1)
        .attribute("model", model_attr(model))
        .attribute("result", result.as_str())
        .attribute("message_kind", message_kind.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn chat_content_length(bytes: f64, role: ContentRole, model: Option<&str>, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_chat_content_length", bytes)
        .attribute("content_role", role.as_str())
        .attribute("model", model_attr(model))
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn chat_message_tag(tag: MessageTag, engine: Engine) -> MetricRecord {
    counter("kiro_cli_chat_message_tags_total", 1)
        .attribute("message_tag", tag.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn bedrock_request_error_for_engine(
    model: Option<&str>,
    operation: Operation,
    error_kind: ErrorKind,
    status_class: StatusClass,
    failure_reason_code: Option<&str>,
    engine: Engine,
) -> MetricRecord {
    for_engine(
        bedrock_request_error(model, operation, error_kind, status_class, failure_reason_code),
        engine,
    )
}

pub fn user_turn_prompt_length(bytes: f64, model: Option<&str>, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_user_turn_prompt_length", bytes)
        .attribute("model", model_attr(model))
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn user_turn_response_length(bytes: f64, model: Option<&str>, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_user_turn_response_length", bytes)
        .attribute("model", model_attr(model))
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn user_turn_follow_up_count(count: f64, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_user_turn_follow_up_count", count)
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn user_turn_request_attempts(count: f64, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_user_turn_request_attempts", count)
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn user_turn_time_to_first_chunk_ms(milliseconds: f64, model: Option<&str>, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_user_turn_time_to_first_chunk_ms", milliseconds)
        .attribute("model", model_attr(model))
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn request_error_context_length(
    bytes: f64,
    model: Option<&str>,
    error_kind: ErrorKind,
    engine: Engine,
) -> MetricRecord {
    histogram("kiro_cli_request_error_context_length", bytes)
        .attribute("model", model_attr(model))
        .attribute("error_kind", error_kind.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

#[derive(Clone, Copy, Debug)]
pub struct V1ToolCall<'a> {
    pub invocation: ToolInvocation<'a>,
    pub builtin_tool_name: Option<&'a str>,
    pub model: Option<&'a str>,
    pub is_trusted: bool,
}

fn bounded_tool_invocation<'a>(
    invocation: ToolInvocation<'a>,
    builtin_tool_name: Option<&'a str>,
) -> ToolInvocation<'a> {
    if matches!(invocation.identity, ToolIdentity::Builtin { .. }) {
        ToolInvocation {
            identity: ToolIdentity::from_origin(builtin_tool_name, invocation.origin()),
            ..invocation
        }
    } else {
        invocation
    }
}

pub fn v1_tool_call_total(input: V1ToolCall<'_>) -> MetricRecord {
    let invocation = bounded_tool_invocation(input.invocation, input.builtin_tool_name);
    let mut builder = counter("kiro_cli_tool_call_total", 1)
        .attribute("outcome", invocation.outcome().as_str())
        .attribute("tool_origin", invocation.origin().as_str())
        .attribute("engine", Engine::V1.as_str())
        .attribute("model", model_attr(input.model))
        .attribute("is_accepted", invocation.is_accepted.to_string())
        .attribute("is_trusted", input.is_trusted.to_string());
    if let ToolIdentity::Builtin { name } = invocation.identity {
        builder = builder.attribute("builtin_tool_name", builtin_tool_name_value(name));
    }
    if let Some(is_valid) = input.invocation.is_valid {
        builder = builder.attribute("is_valid", is_valid.to_string());
    }
    if let Some(is_success) = input.invocation.is_success {
        builder = builder.attribute("is_success", is_success.to_string());
    }
    builder.expect_valid()
}

pub fn v1_tool_invocations(invocation: ToolInvocation<'_>) -> MetricRecord {
    let invocation = bounded_tool_invocation(invocation, None);
    counter("kiro_cli_tool_invocations", 1)
        .attribute("outcome", invocation.outcome().as_str())
        .attribute("tool_origin", invocation.origin().as_str())
        .attribute("engine", Engine::V1.as_str())
        .expect_valid()
}

pub fn v1_tool_execution_duration_ms(duration_ms: f64, invocation: ToolInvocation<'_>) -> Option<MetricRecord> {
    let invocation = bounded_tool_invocation(invocation, None);
    let is_success = invocation.is_success?;
    if !duration_ms.is_finite() || duration_ms <= 0.0 {
        return None;
    }
    Some(
        histogram("kiro_cli_tool_execution_duration_ms", duration_ms)
            .attribute("is_success", is_success.to_string())
            .attribute("tool_origin", invocation.origin().as_str())
            .attribute("engine", Engine::V1.as_str())
            .expect_valid(),
    )
}

pub fn tool_token_size(value: f64, role: ContentRole, tool_origin: ToolOrigin, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_tool_token_size", value)
        .attribute("content_role", role.as_str())
        .attribute("tool_origin", tool_origin.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn tool_duration(seconds: f64, stage: DurationStage, tool_origin: ToolOrigin, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_tool_duration", seconds)
        .attribute("duration_stage", stage.as_str())
        .attribute("tool_origin", tool_origin.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn mcp_tool_count(count: f64, server_class: McpServerClass, count_kind: CountKind, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_mcp_tool_count", count)
        .attribute("mcp_server_class", server_class.as_str())
        .attribute("count_kind", count_kind.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn agent_contribution_total(engine: Engine) -> MetricRecord {
    counter("kiro_cli_agent_contribution_total", 1)
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn agent_contribution_lines_total(
    value: u64,
    source: ContributionSource,
    change: ContributionChange,
    engine: Engine,
) -> MetricRecord {
    counter("kiro_cli_agent_contribution_lines_total", value)
        .attribute("contribution_source", source.as_str())
        .attribute("contribution_change", change.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

#[derive(Clone, Copy, Debug)]
pub struct AgentContributionMetrics {
    pub lines_by_agent: Option<isize>,
    pub lines_by_user: Option<isize>,
    pub engine: Engine,
}

pub fn agent_contribution_records(input: AgentContributionMetrics) -> Vec<MetricRecord> {
    let mut records = vec![agent_contribution_total(input.engine)];
    for (value, source) in [
        (input.lines_by_agent, ContributionSource::Agent),
        (input.lines_by_user, ContributionSource::User),
    ] {
        if let Some(value) = value {
            records.push(agent_contribution_lines_total(
                value.unsigned_abs() as u64,
                source,
                if value < 0 {
                    ContributionChange::Removed
                } else {
                    ContributionChange::Added
                },
                input.engine,
            ));
        }
    }
    records
}

pub fn subagent_tool_uses_total(value: u64, count_kind: CountKind, engine: Engine) -> MetricRecord {
    counter("kiro_cli_subagent_tool_uses_total", value)
        .attribute("count_kind", count_kind.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn agent_config_init_total(result: ResultKind, migration_executed: bool, engine: Engine) -> MetricRecord {
    counter("kiro_cli_agent_config_init_total", 1)
        .attribute("result", result.as_str())
        .attribute("migration_executed", migration_executed.to_string())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn agent_config_count(count: f64, count_kind: CountKind, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_agent_config_count", count)
        .attribute("count_kind", count_kind.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

#[derive(Clone, Copy, Debug)]
pub struct AgentConfigMetrics {
    pub agents_loaded_count: i64,
    pub agents_loaded_failed_count: i64,
    pub migration_executed: bool,
    pub migrated_count: i64,
    pub engine: Engine,
}

pub fn agent_config_records(input: AgentConfigMetrics) -> Vec<MetricRecord> {
    let result = if input.agents_loaded_failed_count > 0 {
        ResultKind::Failed
    } else {
        ResultKind::Success
    };
    vec![
        agent_config_init_total(result, input.migration_executed, input.engine),
        agent_config_count(
            input.agents_loaded_count.max(0) as f64,
            CountKind::AgentsLoaded,
            input.engine,
        ),
        agent_config_count(
            input.agents_loaded_failed_count.max(0) as f64,
            CountKind::AgentsFailed,
            input.engine,
        ),
        agent_config_count(input.migrated_count.max(0) as f64, CountKind::Migrated, input.engine),
    ]
}

pub fn profile_selection_total(
    source: ProfileSource,
    profile_region: RegionClass,
    sso_region: RegionClass,
    result: ResultKind,
    engine: Engine,
) -> MetricRecord {
    counter("kiro_cli_profile_selection_total", 1)
        .attribute("profile_source", source.as_str())
        .attribute("profile_region", profile_region.as_str())
        .attribute("sso_region", sso_region.as_str())
        .attribute("result", result.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn profile_state_total(
    source: ProfileSource,
    profile_region: RegionClass,
    sso_region: RegionClass,
    result: ResultKind,
    engine: Engine,
) -> MetricRecord {
    counter("kiro_cli_profile_state_total", 1)
        .attribute("profile_source", source.as_str())
        .attribute("profile_region", profile_region.as_str())
        .attribute("sso_region", sso_region.as_str())
        .attribute("result", result.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn profile_count(count: f64, source: ProfileSource, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_profile_count", count)
        .attribute("profile_source", source.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

#[derive(Clone, Copy, Debug)]
pub struct ProfileSelectionMetrics {
    pub source: ProfileSource,
    pub profile_region: RegionClass,
    pub sso_region: RegionClass,
    pub result: ResultKind,
    pub profile_count: Option<i64>,
    pub engine: Engine,
}

pub fn profile_selection_records(input: ProfileSelectionMetrics) -> Vec<MetricRecord> {
    let mut records = vec![profile_selection_total(
        input.source,
        input.profile_region,
        input.sso_region,
        input.result,
        input.engine,
    )];
    if let Some(count) = input.profile_count {
        records.push(profile_count(count.max(0) as f64, input.source, input.engine));
    }
    records
}

pub fn voice_input_total(
    backend: VoiceBackend,
    input_method: VoiceInputMethod,
    model_size: VoiceModelSize,
    auto_submit: Option<bool>,
    result: ResultKind,
    engine: Engine,
) -> MetricRecord {
    let mut record = counter("kiro_cli_voice_input_total", 1)
        .attribute("voice_backend", backend.as_str())
        .attribute("voice_input_method", input_method.as_str())
        .attribute("voice_model_size", model_size.as_str())
        .attribute("result", result.as_str())
        .attribute("engine", engine.as_str());
    if let Some(auto_submit) = auto_submit {
        record = record.attribute("auto_submit", auto_submit.to_string());
    }
    record.expect_valid()
}

pub fn voice_duration(milliseconds: f64, stage: DurationStage, backend: VoiceBackend, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_voice_duration", milliseconds)
        .attribute("duration_stage", stage.as_str())
        .attribute("voice_backend", backend.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

pub fn voice_text_length(characters: f64, backend: VoiceBackend, engine: Engine) -> MetricRecord {
    histogram("kiro_cli_voice_text_length", characters)
        .attribute("voice_backend", backend.as_str())
        .attribute("engine", engine.as_str())
        .expect_valid()
}

#[derive(Clone, Copy, Debug)]
pub struct VoiceInputMetrics {
    pub result: ResultKind,
    pub backend: VoiceBackend,
    pub input_method: VoiceInputMethod,
    pub recording_duration_ms: Option<i64>,
    pub transcription_duration_ms: Option<i64>,
    pub text_length: Option<i64>,
    pub model_size: VoiceModelSize,
    pub auto_submit: Option<bool>,
    pub engine: Engine,
}

pub fn voice_input_records(input: VoiceInputMetrics) -> Vec<MetricRecord> {
    let mut records = vec![voice_input_total(
        input.backend,
        input.input_method,
        input.model_size,
        input.auto_submit,
        input.result,
        input.engine,
    )];
    for (value, stage) in [
        (input.recording_duration_ms, DurationStage::Recording),
        (input.transcription_duration_ms, DurationStage::Transcription),
    ] {
        if let Some(value) = value.filter(|value| *value >= 0) {
            records.push(voice_duration(value as f64, stage, input.backend, input.engine));
        }
    }
    if let Some(value) = input.text_length.filter(|value| *value >= 0) {
        records.push(voice_text_length(value as f64, input.backend, input.engine));
    }
    records
}

pub fn process_memory_rss_for_engine(
    bytes: f64,
    agent_kind: AgentKind,
    engine: Engine,
    process_role: ProcessRole,
) -> MetricRecord {
    gauge("kiro_cli.process.memory.rss", bytes)
        .attribute("agent_kind", agent_kind.as_str())
        .attribute("engine", engine.as_str())
        .attribute("process_role", process_role.as_str())
        .expect_valid()
}

pub fn process_memory_peak_rss_for_agent(
    bytes: f64,
    agent_kind: AgentKind,
    engine: Engine,
    process_role: ProcessRole,
) -> MetricRecord {
    gauge("kiro_cli.process.memory.peak_rss", bytes)
        .attribute("agent_kind", agent_kind.as_str())
        .attribute("engine", engine.as_str())
        .attribute("process_role", process_role.as_str())
        .expect_valid()
}

pub fn process_cpu_utilization_for_engine(
    utilization: f64,
    agent_kind: AgentKind,
    state: ProcessState,
    engine: Engine,
    process_role: ProcessRole,
) -> MetricRecord {
    histogram("kiro_cli.process.cpu.utilization", utilization)
        .attribute("agent_kind", agent_kind.as_str())
        .attribute("state", state.as_str())
        .attribute("engine", engine.as_str())
        .attribute("process_role", process_role.as_str())
        .expect_valid()
}
