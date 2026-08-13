#!/usr/bin/env bun

import * as fs from 'node:fs';
import * as path from 'node:path';
import { parseArgs } from 'node:util';

type Engine = 'v2' | 'kas';
type ScenarioBackendId = 'live' | 'acp-mock';
type ScenarioPriority = 'p0' | 'p1' | 'p2';

interface Scenario {
  id: string;
  category: string;
  engine?: readonly Engine[];
  backend?: readonly ScenarioBackendId[];
  priority?: ScenarioPriority;
  tags?: readonly string[];
}

export interface ScenarioCategoryFilters {
  backend?: ScenarioBackendId;
  engine?: Engine;
  priorities?: ScenarioPriority[];
  categories?: string[];
  tags?: string[];
}

function loadScenarios(scenariosPath: string): Scenario[] {
  const raw = JSON.parse(fs.readFileSync(scenariosPath, 'utf8')) as {
    scenarios?: Scenario[];
  };
  if (!Array.isArray(raw.scenarios)) {
    throw new Error(`Invalid scenarios file: ${scenariosPath}`);
  }
  return raw.scenarios;
}

function matchesFilters(
  scenario: Scenario,
  filters: ScenarioCategoryFilters
): boolean {
  if (
    filters.engine &&
    Array.isArray(scenario.engine) &&
    scenario.engine.length > 0 &&
    !scenario.engine.includes(filters.engine)
  ) {
    return false;
  }

  if (
    filters.backend &&
    Array.isArray(scenario.backend) &&
    scenario.backend.length > 0 &&
    !scenario.backend.includes(filters.backend)
  ) {
    return false;
  }

  if (
    filters.priorities &&
    filters.priorities.length > 0 &&
    (!scenario.priority || !filters.priorities.includes(scenario.priority))
  ) {
    return false;
  }

  if (
    filters.categories &&
    filters.categories.length > 0 &&
    !filters.categories.includes(scenario.category)
  ) {
    return false;
  }

  if (
    filters.tags &&
    filters.tags.length > 0 &&
    !filters.tags.some((tag) => scenario.tags?.includes(tag))
  ) {
    return false;
  }

  return true;
}

export function listScenarioCategories(
  scenarios: readonly Scenario[],
  filters: ScenarioCategoryFilters = {}
): string[] {
  return [...new Set(scenarios.filter((s) => matchesFilters(s, filters)).map((s) => s.category))].sort();
}

function parseCli(): {
  scenariosPath: string;
  format: 'json' | 'lines';
  filters: ScenarioCategoryFilters;
} {
  const { values } = parseArgs({
    options: {
      scenarios: { type: 'string' },
      backend: { type: 'string' },
      engine: { type: 'string' },
      priority: { type: 'string', multiple: true, default: [] },
      category: { type: 'string', multiple: true, default: [] },
      tag: { type: 'string', multiple: true, default: [] },
      format: { type: 'string', default: 'json' },
    },
    strict: true,
    allowPositionals: false,
  });

  const backend = values.backend as ScenarioBackendId | undefined;
  const engine = values.engine as Engine | undefined;
  const format = values.format as 'json' | 'lines';
  const priorities = (values.priority as ScenarioPriority[]) ?? [];
  const categories = (values.category as string[]) ?? [];
  const tags = (values.tag as string[]) ?? [];

  if (backend && backend !== 'live' && backend !== 'acp-mock') {
    throw new Error(`Unsupported backend: ${backend}`);
  }

  if (engine && engine !== 'v2' && engine !== 'kas') {
    throw new Error(`Unsupported engine: ${engine}`);
  }

  if (format !== 'json' && format !== 'lines') {
    throw new Error(`Unsupported output format: ${format}`);
  }

  const scenariosPath = path.resolve(
    values.scenarios ??
      path.join(import.meta.dir, '../smoke/scenarios.json')
  );

  return {
    scenariosPath,
    format,
    filters: {
      ...(backend ? { backend } : {}),
      ...(engine ? { engine } : {}),
      ...(priorities.length > 0 ? { priorities } : {}),
      ...(categories.length > 0 ? { categories } : {}),
      ...(tags.length > 0 ? { tags } : {}),
    },
  };
}

async function main(): Promise<void> {
  const { scenariosPath, format, filters } = parseCli();
  const categories = listScenarioCategories(loadScenarios(scenariosPath), filters);
  if (format === 'lines') {
    for (const category of categories) {
      console.log(category);
    }
    return;
  }
  process.stdout.write(JSON.stringify(categories));
}

if (import.meta.main) {
  await main();
}
