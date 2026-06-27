/**
 * Unit tests for the SDK-backed metrics entry point (`meter.ts`).
 *
 * Asserts the load-bearing behaviors of the thin meter wrapper:
 *   (a) telemetry is DISABLED (no provider, instruments no-op) when
 *       KIRO_TELEMETRY_OTLP_ENDPOINT is unset/empty (Contract 4);
 *   (b) instruments are memoized per (scope, name) — recording the same metric
 *       twice does not create a second instrument or throw;
 *   (c) a gauge value is recorded and reaches the exporter at collect time.
 *
 * No real network I/O: we inject an in-memory-exporter-backed reader via the
 * `_setReaderForTests` seam so the full record → reader → exporter path runs
 * with DELTA temporality, exactly as production minus the OTLP HTTP hop.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import {
  AggregationTemporality,
  DataPointType,
  InMemoryMetricExporter,
  PeriodicExportingMetricReader,
  type ResourceMetrics,
} from '@opentelemetry/sdk-metrics';

import {
  counter,
  gauge,
  histogram,
  forceFlushMetrics,
  _resetMeterForTests,
  _setReaderForTests,
} from '../meter.js';

let originalEndpoint: string | undefined;
let originalEnabled: string | undefined;
let originalDisable: string | undefined;

beforeEach(() => {
  originalEndpoint = process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'];
  originalEnabled = process.env['KIRO_TELEMETRY_ENABLED'];
  originalDisable = process.env['KIRO_DISABLE_TELEMETRY'];
  _resetMeterForTests();
});

afterEach(() => {
  _resetMeterForTests();
  if (originalEndpoint === undefined)
    delete process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'];
  else process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'] = originalEndpoint;
  if (originalEnabled === undefined)
    delete process.env['KIRO_TELEMETRY_ENABLED'];
  else process.env['KIRO_TELEMETRY_ENABLED'] = originalEnabled;
  if (originalDisable === undefined)
    delete process.env['KIRO_DISABLE_TELEMETRY'];
  else process.env['KIRO_DISABLE_TELEMETRY'] = originalDisable;
});

/** Build an in-memory DELTA exporter + reader and inject it. */
function injectInMemory(): InMemoryMetricExporter {
  const exporter = new InMemoryMetricExporter(AggregationTemporality.DELTA);
  const reader = new PeriodicExportingMetricReader({
    exporter,
    // Long interval — we drive export explicitly via forceFlushMetrics().
    exportIntervalMillis: 600_000,
  });
  _setReaderForTests(reader);
  return exporter;
}

/** Flatten every collected metric across all exported resource batches. */
function allMetrics(batches: ResourceMetrics[]) {
  return batches.flatMap((b) => b.scopeMetrics.flatMap((sm) => sm.metrics));
}

describe('meter (a) endpoint gating', () => {
  it('is disabled and never throws when the endpoint is unset', () => {
    delete process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'];
    // Inject a reader so that, IF a provider were built, it would capture.
    const exporter = injectInMemory();

    expect(() =>
      counter('kiro_cli_disabled_total', 1, { engine: 'v3' })
    ).not.toThrow();
    expect(() => gauge('kiro_cli_disabled_gauge', 0.5)).not.toThrow();

    // No provider built => exporter never received anything.
    expect(allMetrics(exporter.getMetrics())).toHaveLength(0);
  });

  it('is disabled when the endpoint is empty/whitespace', () => {
    process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'] = '   ';
    const exporter = injectInMemory();
    counter('kiro_cli_disabled_total', 1);
    expect(allMetrics(exporter.getMetrics())).toHaveLength(0);
  });
});

describe('meter (b) instrument memoization', () => {
  it('reuses one instrument across repeated records (no duplicate-create throw)', async () => {
    process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'] = 'http://127.0.0.1:9/x';
    delete process.env['KIRO_DISABLE_TELEMETRY'];
    process.env['KIRO_TELEMETRY_ENABLED'] = 'true';
    const exporter = injectInMemory();

    counter('kiro_cli_user_turns', 1, { engine: 'v3' });
    counter('kiro_cli_user_turns', 1, { engine: 'v3' });
    counter('kiro_cli_user_turns', 1, { engine: 'v2' });

    await forceFlushMetrics();

    const matching = allMetrics(exporter.getMetrics()).filter(
      (m) => m.descriptor.name === 'kiro_cli_user_turns'
    );
    // Memoized => exactly ONE metric stream for the name (not three).
    expect(matching).toHaveLength(1);
    // DELTA: same-attr increments aggregate within the window.
    const v3 = matching[0]!.dataPoints.find(
      (d) => (d.attributes as Record<string, unknown>)['engine'] === 'v3'
    );
    expect(v3?.value).toBe(2);
  });
});

describe('meter (c) gauge recording', () => {
  it('records a gauge value that reaches the exporter at collect time', async () => {
    process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'] = 'http://127.0.0.1:9/x';
    delete process.env['KIRO_DISABLE_TELEMETRY'];
    process.env['KIRO_TELEMETRY_ENABLED'] = 'true';
    const exporter = injectInMemory();

    gauge('kiro_cli_context_usage_percentage', 73, { engine: 'v3' });

    await forceFlushMetrics();

    const g = allMetrics(exporter.getMetrics()).find(
      (m) => m.descriptor.name === 'kiro_cli_context_usage_percentage'
    );
    expect(g).toBeDefined();
    expect(g!.dataPointType).toBe(DataPointType.GAUGE);
    expect(g!.dataPoints[0]!.value).toBe(73);
    expect(
      (g!.dataPoints[0]!.attributes as Record<string, unknown>)['engine']
    ).toBe('v3');
  });

  it('records a histogram observation too', async () => {
    process.env['KIRO_TELEMETRY_OTLP_ENDPOINT'] = 'http://127.0.0.1:9/x';
    delete process.env['KIRO_DISABLE_TELEMETRY'];
    process.env['KIRO_TELEMETRY_ENABLED'] = 'true';
    const exporter = injectInMemory();

    histogram('kiro_cli_tool_execution_duration_ms', 42, { engine: 'v3' });
    await forceFlushMetrics();

    const h = allMetrics(exporter.getMetrics()).find(
      (m) => m.descriptor.name === 'kiro_cli_tool_execution_duration_ms'
    );
    expect(h).toBeDefined();
    expect(h!.dataPointType).toBe(DataPointType.HISTOGRAM);
  });
});
