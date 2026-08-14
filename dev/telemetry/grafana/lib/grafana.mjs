import { catalogMetricNames, requireMetricSpec } from './metric-catalog.mjs';
import { variableAppliesToMetric } from './variables.mjs';

export const datasource = Object.freeze({
  type: 'prometheus',
  uid: 'prometheus',
});

export function target(expr, legendFormat, refId = 'A') {
  return { datasource, expr, legendFormat, refId };
}

function variable(spec, query) {
  return {
    allValue: '.*',
    current: { selected: true, text: ['All'], value: ['$__all'] },
    datasource,
    definition: query,
    description: spec.description,
    hide: 0,
    includeAll: true,
    label: spec.displayLabel,
    multi: true,
    name: spec.name,
    options: [],
    query: {
      qryType: 1,
      query,
      refId: `PrometheusVariableQueryEditor-${spec.name}`,
    },
    refresh: 2,
    regex: '',
    skipUrlSync: false,
    sort: 1,
    type: 'query',
  };
}

function buildVariable(spec, priorVariables) {
  if (spec.source === 'scrape-target') {
    return variable(spec, 'label_values(up{job="otel-collector"}, instance)');
  }

  const metricSpec = requireMetricSpec(spec.metric);
  if (!metricSpec.cloudwatchDimensions.has(spec.prometheusLabel)) {
    throw new Error(
      `Variable ${spec.name} maps to ${spec.prometheusLabel}, which is not a CloudWatch dimension of ${spec.metric}`
    );
  }
  const filters = ['job="otel-collector"'];
  for (const prior of priorVariables) {
    if (
      prior.source !== 'scrape-target' &&
      variableAppliesToMetric(prior, spec.metric) &&
      metricSpec.cloudwatchDimensions.has(prior.prometheusLabel)
    ) {
      filters.push(`${prior.prometheusLabel}=~"$${prior.name}"`);
    }
  }
  return variable(
    spec,
    `label_values(${spec.metric}{${filters.join(', ')}}, ${spec.prometheusLabel})`
  );
}

function panelOptions(type) {
  if (type === 'piechart') {
    return {
      displayLabels: ['name', 'percent'],
      legend: { displayMode: 'list', placement: 'right', showLegend: true },
      pieType: 'pie',
      reduceOptions: { calcs: ['lastNotNull'], fields: '', values: false },
      tooltip: { mode: 'single', sort: 'none' },
    };
  }
  if (type === 'stat') {
    return {
      colorMode: 'value',
      graphMode: 'area',
      justifyMode: 'auto',
      orientation: 'horizontal',
      reduceOptions: { calcs: ['lastNotNull'], fields: '', values: false },
      showPercentChange: false,
      textMode: 'auto',
      wideLayout: true,
    };
  }
  if (type === 'bargauge') {
    return {
      displayMode: 'gradient',
      maxVizHeight: 300,
      minVizHeight: 16,
      minVizWidth: 8,
      namePlacement: 'auto',
      orientation: 'horizontal',
      reduceOptions: { calcs: ['lastNotNull'], fields: '', values: false },
      showUnfilled: true,
      sizing: 'auto',
      valueMode: 'color',
    };
  }
  if (type === 'table') {
    return {
      cellHeight: 'sm',
      footer: { countRows: false, fields: '', reducer: ['sum'], show: false },
      showHeader: true,
    };
  }
  return {
    legend: {
      calcs: ['lastNotNull'],
      displayMode: 'table',
      placement: 'bottom',
      showLegend: true,
    },
    tooltip: { hideZeros: false, mode: 'multi', sort: 'desc' },
  };
}

const polarityPolicy = Object.freeze({
  'higher-is-better': Object.freeze({
    colorMode: 'continuous-RdYlGr',
    steps: [
      { color: 'red', value: null },
      { color: 'yellow', value: 80 },
      { color: 'green', value: 95 },
    ],
  }),
  'lower-is-better': Object.freeze({
    colorMode: 'continuous-GrYlRd',
    steps: [
      { color: 'green', value: null },
      { color: 'yellow', value: 1 },
      { color: 'red', value: 5 },
    ],
  }),
  neutral: Object.freeze({
    colorMode: 'continuous-BlPu',
    steps: [{ color: 'blue', value: null }],
  }),
});

function panelFieldConfig(spec, type) {
  const policy =
    type === 'bargauge'
      ? polarityPolicy[spec.polarity]
      : {
          colorMode: 'palette-classic',
          steps: [{ color: 'green', value: null }],
        };
  if (type === 'bargauge' && !policy) {
    throw new Error(
      `${spec.title}: bar gauges require a valid polarity policy`
    );
  }
  const defaults = {
    color: { mode: policy.colorMode },
    mappings: [],
    thresholds: { mode: 'absolute', steps: policy.steps },
  };
  if (spec.unit) defaults.unit = spec.unit;
  if (spec.min !== undefined) defaults.min = spec.min;
  if (spec.max !== undefined) defaults.max = spec.max;
  if (type === 'timeseries') {
    defaults.custom = {
      axisCenteredZero: false,
      axisColorMode: 'text',
      axisLabel: '',
      axisPlacement: 'auto',
      barAlignment: 0,
      drawStyle: 'line',
      fillOpacity: 12,
      gradientMode: 'none',
      hideFrom: { legend: false, tooltip: false, viz: false },
      insertNulls: false,
      lineInterpolation: 'linear',
      lineWidth: 2,
      pointSize: 5,
      scaleDistribution: { type: 'linear' },
      showPoints: 'never',
      spanNulls: false,
      stacking: { group: 'A', mode: 'none' },
      thresholdsStyle: { mode: 'off' },
    };
  }
  return { defaults, overrides: [] };
}

export function createDashboard({
  uid,
  title,
  description,
  from,
  timezone = 'browser',
  linkedUid,
  linkedTitle,
  variables,
}) {
  let id = 1;
  let y = 0;
  const panels = [];

  function row(rowTitle) {
    panels.push({
      collapsed: false,
      gridPos: { h: 1, w: 24, x: 0, y },
      id: id++,
      panels: [],
      title: rowTitle,
      type: 'row',
    });
    y += 1;
  }

  function add(...specs) {
    for (let index = 0; index < specs.length; index += 2) {
      const pair = specs.slice(index, index + 2);
      const height = Math.max(...pair.map((spec) => spec.height ?? 8));
      pair.forEach((spec, pairIndex) => {
        const type = spec.type ?? 'timeseries';
        if (type === 'bargauge' && spec.polarity === undefined) {
          throw new Error(`${spec.title}: bar gauges require polarity`);
        }
        const width = pair.length === 1 ? 24 : 12;
        const instant =
          spec.instant ??
          ['stat', 'piechart', 'bargauge', 'table'].includes(type);
        const targets = spec.targets.map((entry) => ({
          ...entry,
          instant,
          range: !instant,
        }));
        panels.push({
          datasource,
          description: spec.description ?? '',
          fieldConfig: panelFieldConfig(spec, type),
          gridPos: { h: spec.height ?? 8, w: width, x: pairIndex * 12, y },
          id: id++,
          options: panelOptions(type),
          pluginVersion: '11.4.0',
          targets,
          ...(spec.timeFrom === undefined ? {} : { timeFrom: spec.timeFrom }),
          title: spec.title,
          type,
        });
      });
      y += height;
    }
  }

  function build() {
    return {
      annotations: {
        list: [
          {
            builtIn: 1,
            datasource: { type: 'grafana', uid: '-- Grafana --' },
            enable: true,
            hide: true,
            iconColor: 'rgba(0, 211, 255, 1)',
            name: 'Annotations & Alerts',
            type: 'dashboard',
          },
        ],
      },
      description,
      editable: true,
      fiscalYearStartMonth: 0,
      graphTooltip: 1,
      id: null,
      links: [
        {
          asDropdown: false,
          icon: 'dashboard',
          includeVars: true,
          keepTime: true,
          tags: [],
          targetBlank: false,
          title: linkedTitle,
          type: 'link',
          url: `/d/${linkedUid}`,
        },
      ],
      liveNow: false,
      panels,
      refresh: '5s',
      schemaVersion: 40,
      tags: ['kiro', 'telemetry', 'local', 'reviewed-catalog'],
      templating: {
        list: variables.map((spec, index) =>
          buildVariable(spec, variables.slice(0, index))
        ),
      },
      time: { from, to: 'now' },
      timepicker: {},
      timezone,
      title,
      uid,
      version: 1,
      weekStart: '',
    };
  }

  return Object.freeze({ add, build, row });
}

export function validateDashboard(dashboard) {
  const ids = new Set();
  const titles = new Set();
  const metricNames = catalogMetricNames();
  const variableNames = new Set(
    dashboard.templating.list.map(({ name }) => name)
  );
  for (const panel of dashboard.panels) {
    if (ids.has(panel.id)) {
      throw new Error(`${dashboard.uid}: duplicate panel id ${panel.id}`);
    }
    ids.add(panel.id);
    if (panel.type !== 'row') {
      if (titles.has(panel.title)) {
        throw new Error(
          `${dashboard.uid}: duplicate panel title ${panel.title}`
        );
      }
      titles.add(panel.title);
    }
    for (const entry of panel.targets ?? []) {
      const queryModeCount = Number(entry.instant === true) + Number(entry.range === true);
      if (queryModeCount !== 1) {
        throw new Error(
          `${dashboard.uid}: ${panel.title} must enable exactly one query mode`
        );
      }
      if (entry.range && entry.expr.includes('[$__range]')) {
        throw new Error(
          `${dashboard.uid}: ${panel.title} range targets cannot use $__range`
        );
      }
      if (entry.instant && entry.expr.includes('[$__rate_interval]')) {
        throw new Error(
          `${dashboard.uid}: ${panel.title} instant targets cannot use $__rate_interval`
        );
      }
      for (const match of entry.expr.matchAll(/\$([a-z][a-z0-9_]*)/g)) {
        if (!variableNames.has(match[1])) {
          throw new Error(
            `${dashboard.uid}: ${panel.title} references undefined variable ${match[1]}`
          );
        }
      }
      for (const match of entry.expr.matchAll(/\bkiro_cli_[a-z0-9_]+\b/g)) {
        if (!metricNames.has(match[0])) {
          throw new Error(
            `${dashboard.uid}: ${panel.title} references undeclared metric ${match[0]}`
          );
        }
      }
    }
  }
}
