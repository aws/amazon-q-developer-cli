import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { describe, expect, test } from 'bun:test';

import { resolveKrsPlan } from './krs-plan';
import type { Scenario } from './types';

function scenario(id = 'tool-use-shell'): Scenario {
  return {
    id,
    name: 'Shell tool',
    category: 'tool-use',
    description: 'runs a shell command',
    steps: ['prompt:run echo hello world using the shell'],
    verify: ['screen.contains:requires approval'],
  } as Scenario;
}

/** A directory holding `<id>.json`. */
function krsDirWith(id: string, body: unknown): string {
  const dir = mkdtempSync(join(tmpdir(), 'krs-turns-'));
  writeFileSync(join(dir, `${id}.json`), JSON.stringify(body));
  return dir;
}

const turns = [
  {
    name: 'from-sidecar',
    match: { userInputContains: 'echo hello world' },
    respond: { events: [{ type: 'text', content: 'hello' }] },
  },
];

describe('resolveKrsPlan', () => {
  test('reads the turns beside the suite, keyed by scenario id', () => {
    const krsDir = krsDirWith('tool-use-shell', { turns });
    const plan = resolveKrsPlan(scenario(), { krsDir });

    expect(plan.scenarioId).toBe('tool-use-shell');
    expect(plan.turns).toEqual(turns);
    expect(plan.sidecarPath).toBe(join(krsDir, 'tool-use-shell.json'));
  });

  test('refuses a scenario with no turns, saying where to write them', () => {
    // Having a file is how a scenario says it belongs to this backend, so a
    // missing one is a statement, not an oversight to paper over.
    const krsDir = mkdtempSync(join(tmpdir(), 'krs-empty-'));
    expect(() => resolveKrsPlan(scenario(), { krsDir })).toThrow(
      /has no KRS turns.*tool-use-shell\.json/s
    );
  });

  test('rejects a file without a turns array, naming it', () => {
    const krsDir = krsDirWith('tool-use-shell', { turn: [] });
    expect(() => resolveKrsPlan(scenario(), { krsDir })).toThrow(/"turns" array/);
  });

  test('rejects an empty turns array', () => {
    // Every call would go unanswered, which reads as a broken mock rather than an
    // unfinished script.
    const krsDir = krsDirWith('tool-use-shell', { turns: [] });
    expect(() => resolveKrsPlan(scenario(), { krsDir })).toThrow(/declare no turns/);
  });

  test('rejects a file that is not valid JSON', () => {
    const krsDir = mkdtempSync(join(tmpdir(), 'krs-turns-'));
    writeFileSync(join(krsDir, 'tool-use-shell.json'), '{ not json');
    expect(() => resolveKrsPlan(scenario(), { krsDir })).toThrow(/not valid JSON/);
  });

  test('finds the committed turns the way the backend calls it', () => {
    // The backend passes no directory, so this is the real lookup. It must not be
    // derived from the runner's --fixtures-dir, which points at acp-wire — a
    // sibling of the krs directory, so joining onto it looks one level too deep.
    const plan = resolveKrsPlan(scenario());
    expect(plan.sidecarPath).toContain(join('smoke', 'fixtures', 'krs', 'tool-use-shell.json'));
    expect(plan.turns).toHaveLength(2);
  });
});
