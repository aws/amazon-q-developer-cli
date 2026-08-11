/**
 * OTel-SDK metrics entry point for TUI-originated telemetry (counters,
 * histograms, gauges) over `/v1/metrics`. TUI telemetry is metrics-only — KUTS
 * has no OTLP logs path; export failures are reported through the shared drop
 * counter in otlp-emit.ts (`recordEmitDrop`).
 *
 * Four load-bearing contracts:
 *   1. Fire-and-forget / never-throw: every SDK call is wrapped; on error we
 *      bump the shared drop counter (`recordEmitDrop`) instead of propagating.
 *   2. DELTA temporality (KUTS expects delta). NOTE the SDK enum value is NOT
 *      the raw OTLP wire integer 1 — the SDK maps the enum to the wire value.
 *   3. machine-id on BOTH channels: the `x-kiro-machineid` header AND the
 *      `kiro.machine_id` resource attribute.
 *   4. Endpoint gating: `KIRO_TELEMETRY_OTLP_ENDPOINT` unset/empty, or
 *      `isTelemetryEnabled()` false ⇒ DISABLED (instruments are no-ops).
 */

import {
  AggregationTemporality,
  MeterProvider,
  PeriodicExportingMetricReader,
  type MetricReader,
} from '@opentelemetry/sdk-metrics';
import { OTLPMetricExporter } from '@opentelemetry/exporter-metrics-otlp-http';
import { resourceFromAttributes } from '@opentelemetry/resources';
import type {
  Attributes,
  Meter,
  Counter,
  Histogram,
  ObservableGauge,
  ObservableResult,
} from '@opentelemetry/api';

import {
  getTelemetryIdentity,
  isTelemetryEnabled,
} from './telemetry-identity.js';
import { recordEmitDrop } from './otlp-emit.js';

/** A single metric data point's schema attributes (string/number/boolean values). */
export type MetricAttributes = Record<string, string | number | boolean>;

/**
 * High-cardinality correlation fields carried on an OTLP datapoint without
 * becoming registered metric attributes or CloudWatch dimensions.
 */
export interface MetricLogProperties {
  sessionId?: string;
  requestId?: string;
}

/** Re-export the shared drop counter so callers have one import surface. */
export { getEmitFailedTotal } from './otlp-emit.js';

const DEFAULT_EXPORT_INTERVAL_MS = 60_000;

/**
 * Export cadence; `KIRO_TELEMETRY_EXPORT_INTERVAL_MS` overrides for local
 * testing (watch metrics land without waiting a full minute). Invalid values
 * fall back to the default.
 */
function resolveExportIntervalMs(): number {
  const raw = process.env['KIRO_TELEMETRY_EXPORT_INTERVAL_MS']?.trim();
  if (!raw) return DEFAULT_EXPORT_INTERVAL_MS;
  const parsed = Number(raw);
  return Number.isFinite(parsed) && parsed > 0
    ? parsed
    : DEFAULT_EXPORT_INTERVAL_MS;
}

function stripTrailingSlash(s: string): string {
  return s.replace(/\/+$/, '');
}

/** Resolve the OTLP base endpoint; `''` when unset/empty ⇒ telemetry disabled. */
function resolveOtlpEndpoint(): string {
  const raw = process.env['KIRO_TELEMETRY_OTLP_ENDPOINT']?.trim();
  return raw && raw.length > 0 ? stripTrailingSlash(raw) : '';
}

/** Never let a telemetry error escape; record it as a drop instead. */
function swallow(detail: string, err: unknown): void {
  try {
    recordEmitDrop(`${detail}: ${String(err)}`);
  } catch {
    // recordEmitDrop never throws, but be defensive — a drop must not crash.
  }
}

/** Memoized provider context: `null` (uninitialized), live, or `DISABLED`. */
const DISABLED = Symbol('telemetry-disabled');
type ProviderState =
  | { provider: MeterProvider; meters: Map<string, MeterScope> }
  | typeof DISABLED
  | null;

let state: ProviderState = null;

/**
 * Test-only reader override (via {@link _setReaderForTests}). The endpoint gate
 * and resource attributes are still applied as in production.
 */
let testReader: MetricReader | undefined;

/**
 * Per-Meter (scope) instrument cache. Instruments must be memoized — creating
 * the same instrument twice on one provider is an SDK error.
 */
interface MeterScope {
  meter: Meter;
  counters: Map<string, Counter>;
  histograms: Map<string, Histogram>;
  gauges: Map<string, GaugeEntry>;
}

/** An ObservableGauge plus its last-value store, keyed by attribute set. */
interface GaugeEntry {
  gauge: ObservableGauge;
  values: Map<string, { value: number; attrs: Attributes }>;
}

/** Order-independent key for an attribute set. */
function attrsKey(attrs: MetricAttributes | undefined): string {
  if (!attrs) return '';
  return Object.keys(attrs)
    .sort()
    .map((k) => `${k}=${String(attrs[k])}`)
    .join('');
}

const DEFAULT_SCOPE = 'kiro.tui';

const MAX_LOG_PROPERTY_BYTES = 256;
const CONTROL_CHARACTER = /\p{Cc}/u;

function validatedLogProperty(value: unknown): string | undefined {
  if (
    typeof value !== 'string' ||
    value.trim().length === 0 ||
    Buffer.byteLength(value, 'utf8') > MAX_LOG_PROPERTY_BYTES ||
    CONTROL_CHARACTER.test(value)
  ) {
    return undefined;
  }
  return value;
}

/**
 * Attach queryable identity/correlation fields after call sites have assembled
 * schema attributes. Reserved keys are stripped from caller attributes first,
 * so they can only enter through this validated channel.
 */
function withLogProperties(
  attrs?: MetricAttributes,
  properties?: MetricLogProperties
): MetricAttributes | undefined {
  const result = { ...attrs };
  delete result['user_id'];
  delete result['session_id'];
  delete result['request_id'];

  for (const [key, value] of [
    ['user_id', validatedLogProperty(process.env['KIRO_USER_ID'])],
    ['session_id', validatedLogProperty(properties?.sessionId)],
    ['request_id', validatedLogProperty(properties?.requestId)],
  ] as const) {
    if (value !== undefined) result[key] = value;
  }
  return Object.keys(result).length > 0 ? result : undefined;
}

/**
 * OTLP resource attributes shared by every TUI metric: service.name=kiro-tui
 * plus identity. `kiro.machine_id` is Contract 3's resource-attribute channel.
 */
function buildResource() {
  const identity = getTelemetryIdentity();
  const attrs: Record<string, string> = {
    'service.name': 'kiro-tui',
    'service.version': identity.version,
    'kiro.machine_id': identity.machineId,
  };
  if (identity.userId) attrs['kiro.user_id'] = identity.userId;
  return resourceFromAttributes(attrs);
}

/**
 * Lazily build the single MeterProvider, or return the DISABLED sentinel
 * (Contract 4: `isTelemetryEnabled()` false or endpoint unset/empty). Never
 * throws: any construction failure degrades to DISABLED + a drop.
 */
function ensureProvider(): ProviderState {
  if (state !== null) return state;

  try {
    if (!isTelemetryEnabled()) {
      state = DISABLED;
      return state;
    }
    const endpointBase = resolveOtlpEndpoint();
    if (!endpointBase) {
      state = DISABLED;
      return state;
    }

    const identity = getTelemetryIdentity();
    let reader: MetricReader;
    if (testReader) {
      reader = testReader;
    } else {
      const exporter = new OTLPMetricExporter({
        url: `${endpointBase}/v1/metrics`,
        headers: { 'x-kiro-machineid': identity.machineId },
        // Contract 2: DELTA, not cumulative. SDK maps enum to the wire value.
        temporalityPreference: AggregationTemporality.DELTA,
      });
      reader = new PeriodicExportingMetricReader({
        exporter,
        exportIntervalMillis: resolveExportIntervalMs(),
      });
    }
    const provider = new MeterProvider({
      readers: [reader],
      resource: buildResource(),
    });
    state = { provider, meters: new Map() };
    return state;
  } catch (err) {
    swallow('meter:init', err);
    state = DISABLED;
    return state;
  }
}

function getScope(
  live: { provider: MeterProvider; meters: Map<string, MeterScope> },
  scopeName: string
): MeterScope {
  let scope = live.meters.get(scopeName);
  if (!scope) {
    scope = {
      meter: live.provider.getMeter(scopeName),
      counters: new Map(),
      histograms: new Map(),
      gauges: new Map(),
    };
    live.meters.set(scopeName, scope);
  }
  return scope;
}

/** Record a counter increment. No-op when disabled. Never throws. */
export function counter(
  name: string,
  value: number,
  attrs?: MetricAttributes,
  scope: string = DEFAULT_SCOPE,
  logProperties?: MetricLogProperties
): void {
  try {
    const s = ensureProvider();
    if (s === DISABLED || s === null) return;
    const sc = getScope(s, scope);
    let inst = sc.counters.get(name);
    if (!inst) {
      inst = sc.meter.createCounter(name);
      sc.counters.set(name, inst);
    }
    inst.add(
      value,
      withLogProperties(attrs, logProperties) as Attributes | undefined
    );
  } catch (err) {
    swallow(`counter:${name}`, err);
  }
}

/**
 * Record a histogram observation. No-op when disabled; never throws. `bounds`
 * are applied on first creation only — the SDK fixes advice at instrument
 * creation time.
 */
export function histogram(
  name: string,
  value: number,
  attrs?: MetricAttributes,
  scope: string = DEFAULT_SCOPE,
  bounds?: number[],
  logProperties?: MetricLogProperties
): void {
  try {
    const s = ensureProvider();
    if (s === DISABLED || s === null) return;
    const sc = getScope(s, scope);
    let inst = sc.histograms.get(name);
    if (!inst) {
      inst = sc.meter.createHistogram(
        name,
        bounds ? { advice: { explicitBucketBoundaries: bounds } } : undefined
      );
      sc.histograms.set(name, inst);
    }
    inst.record(
      value,
      withLogProperties(attrs, logProperties) as Attributes | undefined
    );
  } catch (err) {
    swallow(`histogram:${name}`, err);
  }
}

/**
 * Record a gauge value (last-value). No-op when disabled; never throws. Backed
 * by an ObservableGauge because the SDK pull model fires the callback at export
 * time and re-reads the stored value, keyed by attribute set so distinct label
 * combinations report independently. Session and request properties are
 * excluded because each attribute set is retained for the provider lifetime.
 */
export function gauge(
  name: string,
  value: number,
  attrs?: MetricAttributes,
  scope: string = DEFAULT_SCOPE
): void {
  try {
    const s = ensureProvider();
    if (s === DISABLED || s === null) return;
    const sc = getScope(s, scope);
    let entry = sc.gauges.get(name);
    if (!entry) {
      const g = sc.meter.createObservableGauge(name);
      const values = new Map<string, { value: number; attrs: Attributes }>();
      g.addCallback((res: ObservableResult) => {
        for (const v of values.values()) res.observe(v.value, v.attrs);
      });
      entry = { gauge: g, values };
      sc.gauges.set(name, entry);
    }
    const datapointAttributes = withLogProperties(attrs) ?? {};
    entry.values.set(attrsKey(datapointAttributes), {
      value,
      attrs: datapointAttributes as Attributes,
    });
  } catch (err) {
    swallow(`gauge:${name}`, err);
  }
}

/**
 * Flush batched metrics — call before process exit so the final delta window
 * (and exit-only metrics like peak_rss) is delivered. Best-effort; never throws.
 */
export async function forceFlushMetrics(): Promise<void> {
  try {
    const s = ensureProvider();
    if (s === DISABLED || s === null) return;
    await s.provider.forceFlush();
  } catch (err) {
    swallow('forceFlush', err);
  }
}

/**
 * Shut the provider down. Idempotent: state is reset so a later record()
 * re-initializes. Never throws.
 */
export async function shutdownMetrics(): Promise<void> {
  try {
    const s = ensureProvider();
    if (s === DISABLED || s === null) return;
    await s.provider.shutdown();
  } catch (err) {
    swallow('shutdown', err);
  } finally {
    state = null;
  }
}

/** Reset module state for tests (drop counter lives in otlp-emit.ts). */
export function _resetMeterForTests(): void {
  state = null;
  testReader = undefined;
}

/** Inject a reader for tests (in-memory exporter). Call before the first record(). */
export function _setReaderForTests(reader: MetricReader | undefined): void {
  testReader = reader;
}
