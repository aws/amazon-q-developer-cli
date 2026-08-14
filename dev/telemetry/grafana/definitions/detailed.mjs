import { createDashboard, target } from '../lib/grafana.mjs';
import { createQueryApi } from '../lib/promql.mjs';
import { detailedVariables } from '../lib/variables.mjs';

export function makeDetailedDashboard() {
  const {
    counterSeries,
    counterTotal,
    gauge,
    histogramMean,
    histogramQuantile,
    percentage,
  } = createQueryApi(detailedVariables);
  const dashboard = createDashboard({
    uid: 'kiro-telemetry-local',
    title: 'Kiro CLI Local Telemetry',
    description:
      'Local Prometheus analogue of the KUTS KiroCLI dashboard, aligned to the reviewed PR #3682 metric and dimension contract.',
    from: 'now-3h',
    linkedUid: 'kiro-telemetry-health-local',
    linkedTitle: 'Open local health dashboard',
    variables: detailedVariables,
  });

  dashboard.row('Usage & Adoption');
  dashboard.add(
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
      title: 'User turns by interface',
      targets: [
        target(
          counterSeries('kiro_cli_user_turns_total', ['session_interface']),
          '{{session_interface}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'User turns by engine',
      targets: [
        target(
          counterSeries('kiro_cli_user_turns_total', ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Chat session share by agent mode',
      targets: [
        target(
          counterTotal('kiro_cli_chat_session_started_total', ['agent_mode']),
          '{{agent_mode}}'
        ),
      ],
      type: 'piechart',
      unit: 'short',
    },
    {
      title: 'User turn share by agent mode',
      targets: [
        target(
          counterTotal('kiro_cli_user_turns_total', ['agent_mode']),
          '{{agent_mode}}'
        ),
      ],
      type: 'piechart',
      unit: 'short',
    },
    {
      title: 'Interactive UI launch share',
      targets: [
        target(
          counterTotal('kiro_cli_ui_mode_session_started_total', ['ui_mode']),
          '{{ui_mode}}'
        ),
      ],
      type: 'piechart',
      unit: 'short',
    },
    {
      title: 'Explicit goal outcomes',
      targets: [
        target(
          counterSeries('kiro_cli_goal_outcome_total', [
            'agent_engine',
            'goal_outcome',
          ]),
          '{{agent_engine}} {{goal_outcome}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Slash commands invoked',
      targets: [
        target(
          counterSeries('kiro_cli_slash_command_invoked_total', [
            'agent_engine',
            'slash_command',
          ]),
          '{{agent_engine}} {{slash_command}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Top-level commands invoked',
      targets: [
        target(
          counterSeries('kiro_cli_top_level_command_invoked_total', [
            'top_level_command',
          ]),
          '{{top_level_command}}'
        ),
      ],
      unit: 'short',
    }
  );

  const cacheRead = counterTotal(
    'kiro_cli_tokens_consumed_total',
    ['agent_engine'],
    ['token_type="input_cache_read"']
  );
  const inputTokens = counterTotal(
    'kiro_cli_tokens_consumed_total',
    ['agent_engine'],
    ['token_type=~"input_cache_read|input_uncached"']
  );

  dashboard.row('Models, Cost & Latency');
  dashboard.add(
    {
      title: 'Model invocations by model and engine',
      targets: [
        target(
          counterSeries('kiro_cli_model_invocations_total', [
            'agent_engine',
            'model',
          ]),
          '{{agent_engine}} {{model}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Tokens consumed by type',
      targets: [
        target(
          counterSeries('kiro_cli_tokens_consumed_total', [
            'agent_engine',
            'token_type',
          ]),
          '{{agent_engine}} {{token_type}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Credits consumed by model',
      targets: [
        target(
          counterSeries('kiro_cli_credits_consumed_total', ['model']),
          '{{model}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Input token cache-hit share (%)',
      targets: [
        target(
          percentage(cacheRead, inputTokens, ['agent_engine']),
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
      title: 'Model first content p95 by engine and model',
      targets: [
        target(
          histogramQuantile('kiro_cli_model_time_to_first_content_ms', 0.95, [
            'agent_engine',
            'model',
          ]),
          '{{agent_engine}} {{model}} p95'
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
      title: 'Model request duration p95 by outcome',
      targets: [
        target(
          histogramQuantile('kiro_cli_model_request_duration_seconds', 0.95, [
            'agent_engine',
            'model_request_outcome',
          ]),
          '{{agent_engine}} {{model_request_outcome}} p95'
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
    }
  );

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

  dashboard.row('Tools & MCP');
  dashboard.add(
    {
      title: 'Tool calls by outcome',
      targets: [
        target(
          counterSeries('kiro_cli_tool_call_total', ['agent_engine', 'tool_outcome']),
          '{{agent_engine}} {{tool_outcome}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Tool calls by origin',
      targets: [
        target(
          counterSeries('kiro_cli_tool_call_total', ['agent_engine', 'tool_origin']),
          '{{agent_engine}} {{tool_origin}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Tool calls by execution context',
      targets: [
        target(
          counterSeries('kiro_cli_tool_call_total', [
            'agent_engine',
            'execution_context',
          ]),
          '{{agent_engine}} {{execution_context}}'
        ),
      ],
      unit: 'short',
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
      title: 'Tool execution duration mean by engine',
      targets: [
        target(
          histogramMean('kiro_cli_tool_execution_duration_ms', [
            'agent_engine',
          ]),
          '{{agent_engine}} mean'
        ),
      ],
      unit: 'ms',
      min: 0,
    },
    {
      title: 'Tool execution duration p95 by outcome',
      targets: [
        target(
          histogramQuantile('kiro_cli_tool_execution_duration_ms', 0.95, [
            'agent_engine',
            'tool_outcome',
          ]),
          '{{agent_engine}} {{tool_outcome}} p95'
        ),
      ],
      unit: 'ms',
      min: 0,
    },
    {
      title: 'Subagent tool calls by engine',
      targets: [
        target(
          counterSeries(
            'kiro_cli_tool_call_total',
            ['agent_engine'],
            ['execution_context="subagent"']
          ),
          '{{agent_engine}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'MCP server initialization by outcome',
      targets: [
        target(
          counterSeries('kiro_cli_mcp_server_init_total', [
            'agent_engine',
            'mcp_init_outcome',
          ]),
          '{{agent_engine}} {{mcp_init_outcome}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'MCP server initialization by source',
      targets: [
        target(
          counterSeries('kiro_cli_mcp_server_init_total', [
            'agent_engine',
            'mcp_server_source',
          ]),
          '{{agent_engine}} {{mcp_server_source}}'
        ),
      ],
      unit: 'short',
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
    }
  );

  dashboard.row('Process Health & Reliability');
  dashboard.add(
    {
      title: 'Run outcomes',
      targets: [
        target(
          counterSeries('kiro_cli_run_outcome_total', [
            'agent_engine',
            'run_outcome',
          ]),
          '{{agent_engine}} {{run_outcome}}'
        ),
      ],
      unit: 'short',
    },
    {
      title: 'Crashes by process role and kind',
      targets: [
        target(
          counterSeries('kiro_cli_crash_total', [
            'agent_engine',
            'process_role',
            'crash_kind',
          ]),
          '{{agent_engine}} {{process_role}} {{crash_kind}}'
        ),
      ],
      unit: 'short',
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
      title: 'Startup duration p95 by engine',
      targets: [
        target(
          histogramQuantile('kiro_cli_startup_duration_seconds', 0.95, [
            'agent_engine',
          ]),
          '{{agent_engine}} p95'
        ),
      ],
      unit: 's',
      min: 0,
    },
    {
      title: 'MCP tool schema token estimate mean by engine and source',
      targets: [
        target(
          histogramMean('kiro_cli_mcp_tools_token_count_estimate', [
            'agent_engine',
            'mcp_server_source',
          ]),
          '{{agent_engine}} {{mcp_server_source}} mean'
        ),
      ],
      unit: 'short',
      min: 0,
    },
    {
      title: 'MCP tool schema token estimate p95 by engine and source',
      targets: [
        target(
          histogramQuantile('kiro_cli_mcp_tools_token_count_estimate', 0.95, [
            'agent_engine',
            'mcp_server_source',
          ]),
          '{{agent_engine}} {{mcp_server_source}} p95'
        ),
      ],
      unit: 'short',
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
    },
    {
      title: 'Peak RSS p95 by engine and role',
      targets: [
        target(
          histogramQuantile('kiro_cli_process_peak_rss_bytes', 0.95, [
            'agent_engine',
            'process_role',
          ]),
          '{{agent_engine}} {{process_role}} p95'
        ),
      ],
      unit: 'bytes',
      min: 0,
    },
    {
      title: 'CPU utilization mean by engine and role',
      targets: [
        target(
          histogramMean('kiro_cli_process_cpu_utilization_ratio', [
            'agent_engine',
            'process_role',
          ]),
          '{{agent_engine}} {{process_role}}'
        ),
      ],
      unit: 'percentunit',
      min: 0,
    },
    {
      title: 'Process thread count by engine and role',
      targets: [
        target(
          gauge('kiro_cli_process_thread_count', [
            'agent_engine',
            'process_role',
          ]),
          '{{agent_engine}} {{process_role}}'
        ),
      ],
      unit: 'short',
      min: 0,
    },
    {
      title: 'Open file descriptors by engine and role',
      targets: [
        target(
          gauge('kiro_cli_process_open_file_descriptor_count', [
            'agent_engine',
            'process_role',
          ]),
          '{{agent_engine}} {{process_role}}'
        ),
      ],
      unit: 'short',
      min: 0,
    },
    {
      title: 'Process handles by engine and role',
      targets: [
        target(
          gauge('kiro_cli_process_handle_count', [
            'agent_engine',
            'process_role',
          ]),
          '{{agent_engine}} {{process_role}}'
        ),
      ],
      unit: 'short',
      min: 0,
    },
    {
      title: 'TUI heap used by engine',
      targets: [
        target(
          gauge('kiro_cli_tui_heap_used_bytes', ['agent_engine']),
          '{{agent_engine}}'
        ),
      ],
      unit: 'bytes',
      min: 0,
    },
    {
      title: 'TUI event-loop delay p99 mean by engine',
      targets: [
        target(
          histogramMean('kiro_cli_tui_event_loop_delay_p99_seconds', [
            'agent_engine',
          ]),
          '{{agent_engine}}'
        ),
      ],
      unit: 's',
      min: 0,
    },
    {
      title: 'TUI input-to-render p95 mean by engine',
      targets: [
        target(
          histogramMean('kiro_cli_tui_input_to_render_p95_seconds', [
            'agent_engine',
          ]),
          '{{agent_engine}}'
        ),
      ],
      unit: 's',
      min: 0,
    },
    {
      title: 'TUI render duration p95 by engine and kind',
      targets: [
        target(
          histogramQuantile('kiro_cli_tui_render_duration_seconds', 0.95, [
            'agent_engine',
            'render_kind',
          ]),
          '{{agent_engine}} {{render_kind}} p95'
        ),
      ],
      unit: 's',
      min: 0,
    }
  );

  return dashboard.build();
}
