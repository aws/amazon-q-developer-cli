import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import { makeDetailedDashboard } from '../definitions/detailed.mjs';
import { makeHealthDashboard } from '../definitions/health.mjs';
import { createDashboard, target, validateDashboard } from '../lib/grafana.mjs';
import { createQueryApi } from '../lib/promql.mjs';
import { detailedVariables, healthVariables } from '../lib/variables.mjs';

function panel(dashboard, title) {
  const result = dashboard.panels.find(
    (candidate) => candidate.title === title
  );
  assert.ok(result, `missing panel: ${title}`);
  return result;
}

describe('PromQL temporal semantics', () => {
  const query = createQueryApi(detailedVariables);

  test('uses selected-range totals and interval-sized counter series', () => {
    assert.match(
      query.counterTotal('kiro_cli_run_started_total', ['agent_engine']),
      /sum by \(agent_engine\) \(increase\(.+\[\$__range\]\)\)/
    );
    assert.match(
      query.counterSeries('kiro_cli_run_started_total', ['agent_engine']),
      /sum by \(agent_engine\) \(increase\(.+\[\$__rate_interval\]\)\)/
    );
    assert.match(
      query.histogramTotal('kiro_cli_mcp_tools_token_count_estimate', [
        'mcp_server_source',
      ]),
      /sum by \(mcp_server_source\) \(increase\(.+_count.+\[\$__range\]\)\)/
    );
    assert.match(
      query.histogramMean('kiro_cli_startup_duration_seconds'),
      /rate\(.+_sum.+\[\$__rate_interval\]\).+rate\(.+_count.+\[\$__rate_interval\]\)/
    );
    assert.match(
      query.histogramQuantile('kiro_cli_startup_duration_seconds', 0.95),
      /histogram_quantile\(0\.95, sum by \(le\) \(rate\(.+_bucket.+\[\$__rate_interval\]\)\)\)/
    );
    assert.doesNotMatch(
      query.gauge('kiro_cli_process_memory_rss_bytes'),
      /increase|rate/
    );
  });

  test('rejects metric-kind mismatches', () => {
    assert.throws(
      () => query.counterTotal('kiro_cli_process_memory_rss_bytes'),
      /observable_gauge, not the required counter/
    );
    assert.throws(
      () => query.gauge('kiro_cli_run_started_total'),
      /counter, not the required observable_gauge/
    );
  });
});

describe('dashboard query contexts', () => {
  test('uses distinct slash and top-level command dimensions', () => {
    const query = createQueryApi(detailedVariables);
    assert.match(
      query.counterTotal('kiro_cli_slash_command_invoked_total'),
      /slash_command=~"\$slash_command"/
    );
    assert.doesNotMatch(
      query.counterTotal('kiro_cli_top_level_command_invoked_total'),
      /slash_command/
    );

    const slashVariable = makeDetailedDashboard().templating.list.find(
      ({ name }) => name === 'slash_command'
    );
    assert.equal(
      slashVariable.query.query.endsWith(', slash_command)'),
      true
    );
  });

  test('preserves target mode semantics and UTC-day heartbeat overrides', () => {
    const detailed = makeDetailedDashboard();
    const series = panel(detailed, 'Runs started by engine').targets[0];
    const summary = panel(detailed, 'Chat session share by agent mode').targets[0];
    assert.equal(series.range, true);
    assert.equal(series.instant, false);
    assert.match(series.expr, /\[\$__rate_interval\]/);
    assert.equal(summary.instant, true);
    assert.equal(summary.range, false);
    assert.match(summary.expr, /\[\$__range\]/);

    const health = makeHealthDashboard();
    assert.equal(health.timezone, 'utc');
    for (const title of [
      'Daily active installations for selected version',
      'Selected version share of daily active installations (%)',
      'Active installation share by OS',
      'Active installation share by install method',
    ]) {
      const daily = panel(health, title);
      assert.equal(daily.timeFrom, 'now/d');
      assert.ok(daily.targets.every(({ expr }) => expr.includes('[$__range]')));
      assert.ok(daily.targets.every(({ expr }) => !expr.includes('[1d]')));
    }
  });

  test('rejects selected-range windows on range-query targets', () => {
    const invalid = createDashboard({
      uid: 'invalid-range-window',
      title: 'invalid',
      description: '',
      from: 'now-1h',
      linkedUid: 'invalid',
      linkedTitle: 'invalid',
      variables: healthVariables,
    });
    invalid.add({
      title: 'rolling full range',
      targets: [
        target(
          'sum(increase(kiro_cli_run_started_total{job="otel-collector"}[$__range]))',
          'runs'
        ),
      ],
    });
    assert.throws(
      () => validateDashboard(invalid.build()),
      /range targets cannot use \$__range/
    );
  });

  test('rejects interval windows on instant-query targets', () => {
    const invalid = createDashboard({
      uid: 'invalid-instant-window',
      title: 'invalid',
      description: '',
      from: 'now-1h',
      linkedUid: 'invalid',
      linkedTitle: 'invalid',
      variables: healthVariables,
    });
    invalid.add({
      title: 'interval-sized total',
      type: 'stat',
      targets: [
        target(
          'sum(rate(kiro_cli_run_started_total{job="otel-collector"}[$__rate_interval]))',
          'runs'
        ),
      ],
    });
    assert.throws(
      () => validateDashboard(invalid.build()),
      /instant targets cannot use \$__rate_interval/
    );
  });

  test('is independent of dashboard build order', () => {
    const detailedBefore = makeDetailedDashboard();
    const health = makeHealthDashboard();
    const detailedAfter = makeDetailedDashboard();
    assert.deepEqual(detailedAfter, detailedBefore);
    assert.deepEqual(makeHealthDashboard(), health);
  });
});

describe('empty-window behavior', () => {
  test('gates ratios and means on a positive observed denominator', () => {
    const query = createQueryApi(healthVariables);
    const ratio = query.percentage('failures', 'attempts', ['agent_engine']);
    assert.match(ratio, /attempts\) > 0/);
    assert.match(ratio, /or on \(agent_engine\) \(0 \* \(attempts\)\)/);
    assert.doesNotMatch(ratio, /clamp_min|vector\(0\)/);

    const mean = query.histogramMean('kiro_cli_startup_duration_seconds');
    assert.match(mean, /_count.+> 0/);
    assert.doesNotMatch(mean, /clamp_min|vector\(0\)/);
  });
});

describe('bar-gauge polarity', () => {
  test('derives color direction and rejects unclassified gauges', () => {
    const detailed = makeDetailedDashboard();
    assert.equal(
      panel(detailed, 'Input token cache-hit share (%)').fieldConfig.defaults
        .color.mode,
      'continuous-RdYlGr'
    );
    assert.equal(
      panel(detailed, 'Tool error rate by engine (%)').fieldConfig.defaults
        .color.mode,
      'continuous-GrYlRd'
    );
    assert.equal(
      panel(
        makeHealthDashboard(),
        'Selected version share of daily active installations (%)'
      ).fieldConfig.defaults.color.mode,
      'continuous-BlPu'
    );

    const dashboard = createDashboard({
      uid: 'test',
      title: 'test',
      description: '',
      from: 'now-1h',
      linkedUid: 'test',
      linkedTitle: 'test',
      variables: healthVariables,
    });
    assert.throws(
      () =>
        dashboard.add({
          title: 'unclassified',
          targets: [],
          type: 'bargauge',
        }),
      /bar gauges require polarity/
    );
  });
});
