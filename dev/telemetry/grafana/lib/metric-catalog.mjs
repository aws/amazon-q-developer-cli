import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { load } from 'js-yaml';

const schemaPath = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../../crates/kiro-telemetry-schema/schema/metrics.yaml'
);
const schema = load(readFileSync(schemaPath, 'utf8'));

function prometheusMetricName(metric) {
  if (metric.kind === 'counter' && !metric.name.endsWith('_total')) {
    return `${metric.name}_total`;
  }
  return metric.name;
}

const metricSpecs = new Map(
  schema.metrics.map((metric) => [
    prometheusMetricName(metric),
    Object.freeze({
      ...metric,
      cloudwatchDimensions: new Set(metric.cloudwatch_dimensions ?? []),
    }),
  ])
);

export function requireMetricSpec(metric) {
  const metricSpec = metricSpecs.get(metric);
  if (!metricSpec) {
    throw new Error(`No schema entry for Prometheus metric ${metric}`);
  }
  return metricSpec;
}

export function requireMetricKind(metric, expectedKind) {
  const metricSpec = requireMetricSpec(metric);
  if (metricSpec.kind !== expectedKind) {
    throw new Error(
      `${metric} is ${metricSpec.kind}, not the required ${expectedKind}`
    );
  }
  return metricSpec;
}

export function assertMetricDimensions(metric, dimensions) {
  const { cloudwatchDimensions } = requireMetricSpec(metric);
  for (const dimension of dimensions) {
    if (!cloudwatchDimensions.has(dimension)) {
      throw new Error(
        `${dimension} is not a CloudWatch dimension of ${metric}`
      );
    }
  }
}

export function catalogMetricNames() {
  const names = new Set();
  for (const { name, kind } of schema.metrics) {
    if (kind === 'counter') {
      names.add(name.endsWith('_total') ? name : `${name}_total`);
    }
    if (kind === 'histogram') {
      names.add(`${name}_bucket`);
      names.add(`${name}_count`);
      names.add(`${name}_sum`);
    }
    if (kind === 'observable_gauge') names.add(name);
  }
  return names;
}
