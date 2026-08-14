import {
  assertMetricDimensions,
  requireMetricKind,
} from './metric-catalog.mjs';
import { variableAppliesToMetric } from './variables.mjs';

function matcherLabel(metric, matcher) {
  const match = matcher.match(/^\s*([a-z][a-z0-9_]*)\s*(?:=~|!~|!=|=)/);
  if (!match) throw new Error(`Cannot parse matcher for ${metric}: ${matcher}`);
  return match[1];
}

function vectorMatch(groups) {
  return `on (${groups.join(', ')})`;
}

export function createQueryApi(variableSpecs) {
  const variables = Object.freeze([...variableSpecs]);

  function selector(metric, suffix = '', extra = [], options = {}) {
    const metricSpec =
      suffix === '' ? undefined : requireMetricKind(metric, 'histogram');
    const spec = metricSpec ?? requireMetricKind(metric, options.kind);
    const matcherDimensions = extra.map((matcher) =>
      matcherLabel(metric, matcher)
    );
    assertMetricDimensions(metric, matcherDimensions);

    const filters = ['job="otel-collector"'];
    for (const variable of variables) {
      if (variable.name === 'instance') continue;
      if (
        variable.prometheusLabel === 'version_full' &&
        options.version === false
      ) {
        continue;
      }
      if (
        variable.prometheusLabel === 'agent_engine' &&
        options.engine === false
      ) {
        continue;
      }
      if (
        variableAppliesToMetric(variable, metric) &&
        spec.cloudwatchDimensions.has(variable.prometheusLabel)
      ) {
        filters.push(`${variable.prometheusLabel}=~"$${variable.name}"`);
      }
    }
    filters.push('instance=~"$instance"', ...extra);
    return `${metric}${suffix}{${filters.join(', ')}}`;
  }

  function aggregate(operation, expression, groups) {
    const by = groups.length > 0 ? ` by (${groups.join(', ')})` : '';
    return `${operation}${by} (${expression})`;
  }

  function counterIncrease(metric, groups, extra, window, options) {
    requireMetricKind(metric, 'counter');
    assertMetricDimensions(metric, groups);
    const samples = selector(metric, '', extra, {
      ...options,
      kind: 'counter',
    });
    return aggregate('sum', `increase(${samples}[${window}])`, groups);
  }

  function counterTotal(metric, groups = [], extra = [], options = {}) {
    return counterIncrease(metric, groups, extra, '$__range', options);
  }

  function counterSeries(metric, groups = [], extra = [], options = {}) {
    return counterIncrease(
      metric,
      groups,
      extra,
      '$__rate_interval',
      options
    );
  }

  function gauge(metric, groups = [], extra = [], operation = 'avg') {
    requireMetricKind(metric, 'observable_gauge');
    assertMetricDimensions(metric, groups);
    return aggregate(
      operation,
      selector(metric, '', extra, { kind: 'observable_gauge' }),
      groups
    );
  }

  function histogramSeries(metric, suffix, groups, extra) {
    requireMetricKind(metric, 'histogram');
    assertMetricDimensions(metric, groups);
    const samples = selector(metric, suffix, extra);
    return aggregate('sum', `rate(${samples}[$__rate_interval])`, groups);
  }

  function histogramCount(metric, groups = [], extra = []) {
    return histogramSeries(metric, '_count', groups, extra);
  }

  function histogramTotal(metric, groups = [], extra = []) {
    requireMetricKind(metric, 'histogram');
    assertMetricDimensions(metric, groups);
    const samples = selector(metric, '_count', extra);
    return aggregate('sum', `increase(${samples}[$__range])`, groups);
  }

  function histogramMean(metric, groups = [], extra = []) {
    const total = histogramSeries(metric, '_sum', groups, extra);
    const count = histogramSeries(metric, '_count', groups, extra);
    return `((${total}) / (${count})) and ${vectorMatch(groups)} ((${count}) > 0)`;
  }

  function histogramQuantile(metric, quantile, groups = [], extra = []) {
    requireMetricKind(metric, 'histogram');
    assertMetricDimensions(metric, groups);
    const by = [...groups, 'le'];
    const buckets = selector(metric, '_bucket', extra);
    const quantileExpression =
      `histogram_quantile(${quantile}, sum by (${by.join(', ')}) (` +
      `rate(${buckets}[$__rate_interval])))`;
    const count = histogramSeries(metric, '_count', groups, extra);
    return `(${quantileExpression}) and ${vectorMatch(groups)} ((${count}) > 0)`;
  }

  function zeroFill(numerator, denominator, groups = []) {
    return `((${numerator}) or ${vectorMatch(groups)} (0 * (${denominator})))`;
  }

  function add(left, right, groups = []) {
    return `${zeroFill(left, right, groups)} + ${zeroFill(
      right,
      left,
      groups
    )}`;
  }

  function percentage(numerator, denominator, groups = [], complement = false) {
    const alignedNumerator = zeroFill(numerator, denominator, groups);
    const fraction = complement
      ? `((${denominator}) - ${alignedNumerator}) / (${denominator})`
      : `${alignedNumerator} / (${denominator})`;
    return `100 * ((${fraction}) and ${vectorMatch(groups)} ((${denominator}) > 0))`;
  }

  function percentageOfTotal(numerator, denominator) {
    return `100 * (((${numerator}) / scalar(${denominator})) and on () ((${denominator}) > 0))`;
  }

  return Object.freeze({
    add,
    counterSeries,
    counterTotal,
    gauge,
    histogramCount,
    histogramMean,
    histogramTotal,
    histogramQuantile,
    percentage,
    percentageOfTotal,
    zeroFill,
  });
}
