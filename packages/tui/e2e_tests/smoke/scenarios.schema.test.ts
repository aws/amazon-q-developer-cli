/**
 * Corpus-wide constraint checks the loader does not make.
 *
 * `readManifest` rejects a scenario missing an id/name/category/steps/verify and
 * refuses duplicate ids across manifests, but nothing checks the corpus against
 * the schema's enums. A category or priority the schema does not declare would
 * otherwise reach CI as a lane that silently matches nothing.
 *
 * Written without a JSON-schema validator on purpose: the constraints asserted
 * here are the ones the corpus is expected to hold, so a missing dev dependency
 * cannot quietly disable them.
 */
import { describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as path from 'node:path';

const SMOKE_DIR = import.meta.dir;
const SCENARIOS_ROOT = path.join(SMOKE_DIR, 'scenarios');
const SCHEMA_PATH = path.join(SMOKE_DIR, 'scenarios.schema.json');

interface Scenario {
  id: string;
  category?: string;
  priority?: string;
  [key: string]: unknown;
}

function manifestPaths(): string[] {
  return fs
    .readdirSync(SCENARIOS_ROOT, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .flatMap((dir) =>
      fs
        .readdirSync(path.join(SCENARIOS_ROOT, dir.name))
        .filter((name) => name.endsWith('.json'))
        .map((name) => path.join(SCENARIOS_ROOT, dir.name, name))
    )
    .sort();
}

const manifests = manifestPaths().map((file) => ({
  file: path.relative(SMOKE_DIR, file),
  scenarios: JSON.parse(fs.readFileSync(file, 'utf8')).scenarios as Scenario[],
}));
const corpus = manifests.flatMap(({ file, scenarios }) =>
  scenarios.map((scenario) => ({ file, scenario }))
);

const schema = JSON.parse(fs.readFileSync(SCHEMA_PATH, 'utf8'));
const scenarioSchema = schema.$defs.scenario;
const requiredFields: string[] = scenarioSchema.required;
const categoryEnum: string[] = scenarioSchema.properties.category.enum;
const priorityEnum: string[] = scenarioSchema.properties.priority.enum;

describe('scenario corpus', () => {
  it('has the schema constraints wired', () => {
    expect(corpus.length).toBeGreaterThan(0);
    expect(requiredFields).toContain('priority');
    expect(categoryEnum.length).toBe(18);
  });

  it('carries every schema-required field on every scenario', () => {
    const offenders = corpus
      .filter(({ scenario }) =>
        requiredFields.some((field) => {
          const value = scenario[field];
          return Array.isArray(value) ? value.length === 0 : !value;
        })
      )
      .map(({ file, scenario }) => `${file}:${scenario.id}`);
    expect(offenders).toEqual([]);
  });

  it('keeps every category within the schema enum', () => {
    const offenders = corpus
      .filter(({ scenario }) => !categoryEnum.includes(scenario.category!))
      .map(({ file, scenario }) => `${file}:${scenario.id}=${scenario.category}`);
    expect(offenders).toEqual([]);
  });

  it('keeps every priority within the schema enum', () => {
    const offenders = corpus
      .filter(({ scenario }) => !priorityEnum.includes(scenario.priority!))
      .map(({ file, scenario }) => `${file}:${scenario.id}=${scenario.priority}`);
    expect(offenders).toEqual([]);
  });

  it('has unique scenario ids across manifests', () => {
    const seen = new Map<string, string>();
    const dupes: string[] = [];
    for (const { file, scenario } of corpus) {
      const previous = seen.get(scenario.id);
      if (previous) dupes.push(`${scenario.id} (${previous} + ${file})`);
      else seen.set(scenario.id, file);
    }
    expect(dupes).toEqual([]);
  });
});
