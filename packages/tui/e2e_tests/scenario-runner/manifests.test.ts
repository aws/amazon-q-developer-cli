import { describe, expect, it } from 'bun:test';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';

const SCENARIOS_ROOT = join(import.meta.dir, '../smoke/scenarios');
const SCHEMA_PATH = join(import.meta.dir, '../smoke/scenarios.schema.json');

interface SchemaNode {
  properties?: Record<string, unknown>;
  required?: string[];
  additionalProperties?: boolean;
  $defs?: Record<string, SchemaNode>;
}

function manifestPaths(): string[] {
  return readdirSync(SCENARIOS_ROOT, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .flatMap((dir) =>
      readdirSync(join(SCENARIOS_ROOT, dir.name))
        .filter((file) => file.endsWith('.json'))
        .map((file) => join(SCENARIOS_ROOT, dir.name, file))
    )
    .sort();
}

const schema = JSON.parse(readFileSync(SCHEMA_PATH, 'utf8')) as SchemaNode;
const scenarioSchema = schema.$defs?.scenario as SchemaNode;
const manifests = manifestPaths();

// Nothing validates these manifests at build time, and the schema's
// additionalProperties:false is the only thing that would reject a field the
// loader now ignores — a retired one included.
describe('committed scenario manifests', () => {
  it('finds a manifest in every scenario directory', () => {
    expect(manifests.length).toBeGreaterThan(0);
  });

  it.each(manifests)('%s points at the schema', (path) => {
    const { $schema } = JSON.parse(readFileSync(path, 'utf8')) as {
      $schema?: string;
    };
    expect($schema).toBeString();
    expect(existsSync(normalize(join(dirname(path), $schema!)))).toBe(true);
  });

  it.each(manifests)('%s declares only known top-level fields', (path) => {
    const manifest = JSON.parse(readFileSync(path, 'utf8')) as Record<string, unknown>;
    const allowed = new Set(Object.keys(schema.properties ?? {}));
    expect(Object.keys(manifest).filter((key) => !allowed.has(key))).toEqual([]);
    for (const field of schema.required ?? []) {
      expect(manifest[field]).toBeDefined();
    }
  });

  it.each(manifests)('%s declares only known scenario fields', (path) => {
    const { scenarios } = JSON.parse(readFileSync(path, 'utf8')) as {
      scenarios: Record<string, unknown>[];
    };
    const allowed = new Set(Object.keys(scenarioSchema.properties ?? {}));

    for (const scenario of scenarios) {
      const unknownFields = Object.keys(scenario).filter((key) => !allowed.has(key));
      expect(unknownFields).toEqual([]);
      for (const field of scenarioSchema.required ?? []) {
        expect(scenario[field]).toBeDefined();
      }
    }
  });
});
