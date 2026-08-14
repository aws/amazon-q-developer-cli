import { createDashboard, target } from '../lib/grafana.mjs';
import { createQueryApi } from '../lib/promql.mjs';
import { healthVariables } from '../lib/variables.mjs';

export function makeHealthDashboard() {
  const {
    add,
    counterSeries,
    counterTotal,
    gauge,
    histogramMean,
    histogramTotal,
    histogramQuantile,
    percentage,
    percentageOfTotal,
    zeroFill,
  } = createQueryApi(healthVariables);
  const dashboard = createDashboard({
    uid: 'kiro-telemetry-health-local',
    title: 'Kiro CLI Local Health',
    description:
      'Local Prometheus analogue of the KUTS KiroCLI-Health dashboard, aligned to the reviewed PR #3682 metric and dimension contract.',
    from: 'now-6h',
    timezone: 'utc',
    linkedUid: 'kiro-telemetry-local',
    linkedTitle: 'Open local detailed dashboard',
    variables: healthVariables,
  });

  const dailyHeartbeat = (groups = [], options = {}) =>
    counterTotal('kiro_cli_daily_heartbeat_total', groups, [], options);
  const heartbeatSelected = dailyHeartbeat(['version_full']);
  const heartbeatAll = dailyHeartbeat([], { version: false });

  dashboard.row('Volume (P1 - Product Health)');
  dashboard.add(
    {
      title: 'Daily active installations for selected version',
      timeFrom: 'now/d',
      description:
        'Counts active installation-version days represented by daily heartbeats, not unique people.',
      targets: [target(heartbeatSelected, '{{version_full}}')],
      type: 'stat',
      unit: 'short',
    },
    {
      title: 'Selected version share of daily active installations (%)',
      timeFrom: 'now/d',
      targets: [
        target(
          percentageOfTotal(heartbeatSelected, heartbeatAll),
          '{{version_full}}'
        ),
      ],
      type: 'bargauge',
      polarity: 'neutral',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'Active installation share by OS',
      timeFrom: 'now/d',
      targets: [target(dailyHeartbeat(['os_type']), '{{os_type}}')],
      type: 'piechart',
      unit: 'short',
    },
    {
      title: 'Active installation share by install method',
      timeFrom: 'now/d',
      targets: [
        target(dailyHeartbeat(['install_method']), '{{install_method}}'),
      ],
      type: 'piechart',
      unit: 'short',
    },
    {
      title: 'Runs started by interface',
      targets: [
        target(
          counterSeries('kiro_cli_run_started_total', ['session_interface']),
          '{{session_interface}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Runs started by engine',
      targets: [
        target(
          counterSeries('kiro_cli_run_started_total', ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Chat sessions by interface',
      targets: [
        target(
          counterSeries('kiro_cli_chat_session_started_total', ['session_interface']),
          '{{session_interface}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Chat sessions by engine',
      targets: [
        target(
          counterSeries('kiro_cli_chat_session_started_total', ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Completed user turns by interface',
      targets: [
        target(
          counterSeries('kiro_cli_user_turns_total', ['session_interface']),
          '{{session_interface}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Completed user turns by engine',
      targets: [
        target(
          counterSeries('kiro_cli_user_turns_total', ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Model invocations by engine',
      targets: [
        target(
          counterSeries('kiro_cli_model_invocations_total', ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Tool calls by engine',
      targets: [
        target(
          counterSeries('kiro_cli_tool_call_total', ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      unit: 'short',
    }
  );

  const turns = counterTotal('kiro_cli_user_turns_total', ['agent_engine']);
  const cancellations = counterTotal('kiro_cli_turn_cancelled_total', [
    'agent_engine',
  ]);
  const eligibleTurns = `(${turns}) - ${zeroFill(cancellations, turns, ['agent_engine'])}`;
  const turnFailures = counterTotal('kiro_cli_turn_failure_total', ['agent_engine']);
  const runs = counterTotal('kiro_cli_run_started_total', ['agent_engine']);
  const startupSuccesses = histogramTotal(
    'kiro_cli_startup_duration_seconds',
    ['agent_engine']
  );
  const loginSuccess = counterTotal('kiro_cli_login_success_total');
  const loginFailure = counterTotal(
    'kiro_cli_auth_failure_total',
    [],
    ['auth_operation="login"']
  );
  const loginAttempts = add(loginSuccess, loginFailure);
  const modelRequests = counterTotal('kiro_cli_model_invocations_total', [
    'agent_engine',
  ]);
  const modelFailures = counterTotal('kiro_cli_model_request_failure_total', [
    'agent_engine',
  ]);
  const toolCalls = counterTotal('kiro_cli_tool_call_total', ['agent_engine']);
  const toolErrors = counterTotal(
    'kiro_cli_tool_call_total',
    ['agent_engine'],
    ['tool_outcome="error"']
  );
  const mcpTotal = counterTotal('kiro_cli_mcp_server_init_total', ['agent_engine']);
  const mcpSuccesses = counterTotal(
    'kiro_cli_mcp_server_init_total',
    ['agent_engine'],
    ['mcp_init_outcome="success"']
  );
  const crashes = counterTotal('kiro_cli_crash_total', ['agent_engine']);

  dashboard.row('Availability (P0 - Operational Health)');
  dashboard.add(
    {
      title: 'Client-turn availability by engine (%)',
      targets: [
        target(
          percentage(turnFailures, eligibleTurns, ['agent_engine'], true),
          '{{agent_engine}}'
        ),
      ],
      type: 'bargauge',
      polarity: 'higher-is-better',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'Turn failure incidence by engine (%)',
      targets: [
        target(
          percentage(turnFailures, turns, ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      type: 'bargauge',
      polarity: 'lower-is-better',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'Startup availability by engine (%)',
      targets: [
        target(
          percentage(startupSuccesses, runs, ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      type: 'bargauge',
      polarity: 'higher-is-better',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'Login availability (%)',
      targets: [target(percentage(loginSuccess, loginAttempts), 'login')],
      type: 'bargauge',
      polarity: 'higher-is-better',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'Model-request failure incidence by engine (%)',
      targets: [
        target(
          percentage(modelFailures, modelRequests, ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      type: 'bargauge',
      polarity: 'lower-is-better',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'Tool error rate by engine (%)',
      targets: [
        target(
          percentage(toolErrors, toolCalls, ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      type: 'bargauge',
      polarity: 'lower-is-better',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'MCP availability (%)',
      targets: [
        target(
          percentage(mcpSuccesses, mcpTotal, ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      type: 'bargauge',
      polarity: 'higher-is-better',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'Crash incidence per run by engine (%)',
      targets: [
        target(percentage(crashes, runs, ['agent_engine']), '{{agent_engine}}'),
      ],
      type: 'bargauge',
      polarity: 'lower-is-better',
      unit: 'percent',
      min: 0,
      max: 100,
    },
    {
      title: 'Turn failures by reason',
      targets: [
        target(
          counterSeries('kiro_cli_turn_failure_total', [
            'agent_engine',
            'turn_failure_reason',
          ]),
          '{{agent_engine}} {{turn_failure_reason}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Model request failures by error kind',
      targets: [
        target(
          counterSeries('kiro_cli_model_request_failure_total', [
            'agent_engine',
            'error_kind',
          ]),
          '{{agent_engine}} {{error_kind}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Telemetry records dropped by reason',
      targets: [
        target(
          counterSeries('kiro_cli_telemetry_export_dropped_total', ['drop_reason']),
          '{{drop_reason}}'
        ),
      ],
      unit: 'short',
    },
  );

  dashboard.row('Latency (P0 - Operational Health)');
  dashboard.add(
    {
      title: 'Model first content mean by engine',
      targets: [
        target(
          histogramMean('kiro_cli_model_time_to_first_content_ms', [
            'agent_engine',
          ]),
          '{{agent_engine}} mean'
        ),
      ],
      unit: 'ms',
      min: 0,
    },
    {
      title: 'Model first content p95 by engine',
      targets: [
        target(
          histogramQuantile('kiro_cli_model_time_to_first_content_ms', 0.95, [
            'agent_engine',
          ]),
          '{{agent_engine}} p95'
        ),
      ],
      unit: 'ms',
      min: 0,
    },
    {
      title: 'First visible response mean by engine',
      targets: [
        target(
          histogramMean('kiro_cli_time_to_first_visible_response_ms', [
            'agent_engine',
          ]),
          '{{agent_engine}} mean'
        ),
      ],
      unit: 'ms',
      min: 0,
    },
    {
      title: 'First visible response p95 by engine',
      targets: [
        target(
          histogramQuantile(
            'kiro_cli_time_to_first_visible_response_ms',
            0.95,
            ['agent_engine']
          ),
          '{{agent_engine}} p95'
        ),
      ],
      unit: 'ms',
      min: 0,
    },
    {
      title: 'Model request duration mean by engine',
      targets: [
        target(
          histogramMean('kiro_cli_model_request_duration_seconds', [
            'agent_engine',
          ]),
          '{{agent_engine}} mean'
        ),
      ],
      unit: 's',
      min: 0,
    },
    {
      title: 'Model request duration p95 by engine',
      targets: [
        target(
          histogramQuantile('kiro_cli_model_request_duration_seconds', 0.95, [
            'agent_engine',
          ]),
          '{{agent_engine}} p95'
        ),
      ],
      unit: 's',
      min: 0,
    },
    {
      title: 'Successful user turn duration mean by engine',
      targets: [
        target(
          histogramMean('kiro_cli_user_turn_duration_seconds', [
            'agent_engine',
          ]),
          '{{agent_engine}} mean'
        ),
      ],
      unit: 's',
      min: 0,
    },
    {
      title: 'Successful user turn duration p95 by engine',
      targets: [
        target(
          histogramQuantile('kiro_cli_user_turn_duration_seconds', 0.95, [
            'agent_engine',
          ]),
          '{{agent_engine}} p95'
        ),
      ],
      unit: 's',
      min: 0,
    },
    {
      title: 'Startup duration mean by engine',
      targets: [
        target(
          histogramMean('kiro_cli_startup_duration_seconds', ['agent_engine']),
          '{{agent_engine}} mean'
        ),
      ],
      unit: 's',
      min: 0,
    },
    {
      title: 'Process RSS by engine and role',
      targets: [
        target(
          gauge('kiro_cli_process_memory_rss_bytes', [
            'agent_engine',
            'process_role',
          ]),
          '{{agent_engine}} {{process_role}}'
        ),
      ],
      unit: 'bytes',
      min: 0,
    }
  );

  return dashboard.build();
}
