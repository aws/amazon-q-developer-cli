import { describe, expect, test } from 'bun:test';
import { existsSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { createKrsMockBackend, exchangePath } from './backends/krs-mock';
import type { RunOptions, Scenario } from './types';

function scenario(overrides: Partial<Scenario> = {}): Scenario {
  return {
    id: 'unscripted',
    name: 'Unscripted',
    category: 'basic',
    description: 'a scenario with nothing scripted',
    steps: ['prompt:hello'],
    verify: ['screen.contains:hello'],
    ...overrides,
  };
}

describe('krs-mock backend', () => {
  test('is a kas-only backend', () => {
    const backend = createKrsMockBackend('kas');
    expect(backend.id).toBe('krs-mock');
    expect(backend.engine).toBe('kas');
  });

  test('refuses the v2 engine, which does not talk to KRS', () => {
    expect(() => createKrsMockBackend('v2')).toThrow(/only supports engine "kas"/);
  });

  test('refuses a scenario with no turns before spawning anything', async () => {
    const backend = createKrsMockBackend('kas');
    await expect(
      backend.launch(scenario(), {} as RunOptions)
    ).rejects.toThrow(/has no KRS turns/);
  });

  test('refuses a scenario whose turns are empty', async () => {
    const backend = createKrsMockBackend('kas');
    await expect(
      backend.launch(scenario({ turns: [] }), {} as RunOptions)
    ).rejects.toThrow(/has no KRS turns/);
  });

  test('writes the exchange into the output directory CI collects', () => {
    const outputDir = join(
      mkdtempSync(join(tmpdir(), 'krs-exchange-')),
      'results'
    );

    const path = exchangePath(
      scenario(),
      { outputDir } as RunOptions,
      '/somewhere/test-outputs/run/tui.log'
    );

    expect(path).toBe(join(outputDir, 'krs-exchange-unscripted.json'));
    expect(existsSync(outputDir)).toBe(true);
  });

  test('falls back beside the TUI log when the run has no output directory', () => {
    expect(
      exchangePath(scenario(), {} as RunOptions, '/somewhere/run/tui.log')
    ).toBe(join('/somewhere/run', 'krs-exchange-unscripted.json'));
  });
});
