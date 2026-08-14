const metricsWithLabel = Object.freeze({ kind: 'metrics-with-label' });
const allMetrics = Object.freeze({ kind: 'all-metrics' });
const onlyMetrics = (...metrics) =>
  Object.freeze({ kind: 'only-metrics', metrics: Object.freeze(metrics) });

function metricVariable(
  name,
  displayLabel,
  prometheusLabel,
  metric,
  description,
  scope = metricsWithLabel
) {
  return Object.freeze({
    name,
    displayLabel,
    prometheusLabel,
    metric,
    description,
    scope,
    source: 'metric-label',
  });
}

const instanceVariable = Object.freeze({
  name: 'instance',
  displayLabel: 'Instance / Collector',
  prometheusLabel: 'instance',
  description: 'Prometheus scrape target for the local collector.',
  scope: allMetrics,
  source: 'scrape-target',
});

export function variableAppliesToMetric(variable, metric) {
  if (variable.scope.kind === 'all-metrics') return true;
  if (variable.scope.kind === 'metrics-with-label') return true;
  return variable.scope.metrics.includes(metric);
}

export const detailedVariables = Object.freeze([
  metricVariable(
    'version_full',
    'Version',
    'version_full',
    'kiro_cli_daily_heartbeat_total',
    'Exact stable, nightly, or beta CLI version.'
  ),
  metricVariable(
    'agent_engine',
    'Engine',
    'agent_engine',
    'kiro_cli_run_started_total',
    'Agent implementation: v1, v2, v3, or unknown.'
  ),
  metricVariable(
    'slash_command',
    'Slash command',
    'slash_command',
    'kiro_cli_slash_command_invoked_total',
    'Canonical first-party command or bounded custom bucket.',
    onlyMetrics('kiro_cli_slash_command_invoked_total')
  ),
  metricVariable(
    'session_interface',
    'Interface',
    'session_interface',
    'kiro_cli_run_started_total',
    'Interactive CLI, noninteractive CLI, or external ACP.'
  ),
  metricVariable(
    'agent_mode',
    'Agent mode',
    'agent_mode',
    'kiro_cli_chat_session_started_total',
    'Default, plan, spec, autonomous, or custom product mode.'
  ),
  metricVariable(
    'os_type',
    'OS',
    'os_type',
    'kiro_cli_run_started_total',
    'Operating system reported by the active installation.'
  ),
  metricVariable(
    'model',
    'Model',
    'model',
    'kiro_cli_model_invocations_total',
    'Canonical service-provided model identifier.'
  ),
  metricVariable(
    'tool_origin',
    'Tool origin',
    'tool_origin',
    'kiro_cli_tool_call_total',
    'Built-in, MCP, or unknown.'
  ),
  metricVariable(
    'execution_context',
    'Execution context',
    'execution_context',
    'kiro_cli_tool_call_total',
    'Whether the tool ran for the main agent or a subagent.'
  ),
  metricVariable(
    'mcp_server_source',
    'MCP source',
    'mcp_server_source',
    'kiro_cli_mcp_server_init_total',
    'Registry, global, workspace, agent, ACP-injected, or unknown.'
  ),
  metricVariable(
    'process_role',
    'Process role',
    'process_role',
    'kiro_cli_process_memory_rss_bytes',
    'Host, TUI, or KAS subprocess.'
  ),
  instanceVariable,
]);

export const healthVariables = Object.freeze([
  metricVariable(
    'version_full',
    'Version',
    'version_full',
    'kiro_cli_daily_heartbeat_total',
    'Exact stable, nightly, or beta CLI version.'
  ),
  metricVariable(
    'release_channel',
    'Release channel',
    'release_channel',
    'kiro_cli_daily_heartbeat_total',
    'Stable, nightly, beta, or unknown release family.'
  ),
  metricVariable(
    'agent_engine',
    'Engine',
    'agent_engine',
    'kiro_cli_run_started_total',
    'Agent implementation: v1, v2, v3, or unknown.'
  ),
  metricVariable(
    'session_interface',
    'Interface',
    'session_interface',
    'kiro_cli_run_started_total',
    'Interactive CLI, noninteractive CLI, or external ACP.'
  ),
  metricVariable(
    'agent_mode',
    'Agent mode',
    'agent_mode',
    'kiro_cli_chat_session_started_total',
    'Default, plan, spec, autonomous, or custom product mode.'
  ),
  metricVariable(
    'os_type',
    'OS',
    'os_type',
    'kiro_cli_daily_heartbeat_total',
    'Operating system reported by the active installation.'
  ),
  metricVariable(
    'install_method',
    'Install method',
    'install_method',
    'kiro_cli_daily_heartbeat_total',
    'How the active CLI installation was installed.'
  ),
  metricVariable(
    'model',
    'Model',
    'model',
    'kiro_cli_model_invocations_total',
    'Canonical service-provided model identifier.'
  ),
  metricVariable(
    'mcp_server_source',
    'MCP source',
    'mcp_server_source',
    'kiro_cli_mcp_server_init_total',
    'Registry, user configuration, ACP injection, or unknown.'
  ),
  metricVariable(
    'process_role',
    'Process role',
    'process_role',
    'kiro_cli_process_memory_rss_bytes',
    'Host, TUI, or KAS subprocess.'
  ),
  metricVariable(
    'auth_method',
    'Auth method',
    'auth_method',
    'kiro_cli_login_success_total',
    'Authentication method used by the client.'
  ),
  instanceVariable,
]);
