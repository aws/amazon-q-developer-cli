import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const SMOKE_DIR = join(import.meta.dir, '../smoke');

function collectRefs(node: unknown, found: string[] = []): string[] {
  if (Array.isArray(node)) {
    for (const item of node) collectRefs(item, found);
    return found;
  }
  if (node && typeof node === 'object') {
    for (const [key, value] of Object.entries(node)) {
      if (key === '$ref' && typeof value === 'string') found.push(value);
      else collectRefs(value, found);
    }
  }
  return found;
}

function resolvePointer(document: unknown, pointer: string): unknown {
  return pointer
    .split('/')
    .filter((segment) => segment && segment !== '#')
    .reduce<unknown>(
      (node, segment) => (node as Record<string, unknown> | undefined)?.[segment],
      document
    );
}

// Nothing validates these schemas at build time, so a $ref into a sibling file
// would otherwise rot in silence the next time that file is regenerated.
describe('scenario schema references', () => {
  it('resolves every $ref into another schema file', () => {
    const schema = JSON.parse(
      readFileSync(join(SMOKE_DIR, 'scenarios.schema.json'), 'utf8')
    );
    const external = collectRefs(schema).filter((ref) => !ref.startsWith('#'));
    expect(external.length).toBeGreaterThan(0);

    for (const ref of external) {
      const [file, pointer] = ref.split('#');
      const target = JSON.parse(readFileSync(join(SMOKE_DIR, file!), 'utf8'));
      expect(resolvePointer(target, pointer ?? '')).toBeDefined();
    }
  });
});
