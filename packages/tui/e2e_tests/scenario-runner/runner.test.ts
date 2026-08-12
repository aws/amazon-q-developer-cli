import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { filterScenarios, loadScenarios } from './runner';
import type { RunOptions, Scenario, ScenarioBackend } from './types';

function backend(
  id: ScenarioBackend['id'],
  engine: ScenarioBackend['engine']
): ScenarioBackend {
  return {
    id,
    engine,
    async launch() {
      throw new Error('test backend should not launch');
    },
  };
}

function filterFor(
  scenarios: Scenario[],
  target: ScenarioBackend
): Scenario[] {
  return filterScenarios(scenarios, { backend: target } as RunOptions);
}

describe('scenario filtering', () => {
  let tempDir: string | null = null;

  afterEach(() => {
    if (tempDir) {
      rmSync(tempDir, { recursive: true, force: true });
      tempDir = null;
    }
  });

  it('keeps existing scenarios.json valid under legacy compatibility mode', () => {
    const scenarios = loadScenarios();
    const ids = new Set(
      filterFor(
        scenarios,
        backend('acp-mock', 'kas')
      ).map((scenario) => scenario.id)
    );

    expect(ids.has('slash-help')).toBe(true);
    expect(ids.has('slash-save')).toBe(false);
    expect(ids.has('slash-load')).toBe(false);
  });

  it('filters scenarios by engine', () => {
    const scenarios: Scenario[] = [
      {
        id: 'engine-only',
        name: 'Engine only',
        category: 'basic',
        description: 'engine-filtered scenario',
        steps: ['waitForText:ask a question'],
        verify: ['screen.contains:ask a question'],
        engine: ['kas'],
      },
    ];

    expect(
      filterFor(scenarios, backend('live', 'kas')).map((scenario) => scenario.id)
    ).toEqual(['engine-only']);
    expect(filterFor(scenarios, backend('live', 'v2'))).toHaveLength(0);
  });

  it('filters scenarios by backend', () => {
    const scenarios: Scenario[] = [
      {
        id: 'backend-only',
        name: 'Backend only',
        category: 'basic',
        description: 'backend-filtered scenario',
        steps: ['waitForText:ask a question'],
        verify: ['screen.contains:ask a question'],
        backend: ['acp-mock'],
      },
    ];

    expect(
      filterFor(scenarios, backend('acp-mock', 'kas')).map(
        (scenario) => scenario.id
      )
    ).toEqual(['backend-only']);
    expect(filterFor(scenarios, backend('live', 'kas'))).toHaveLength(0);
  });

  it('applies engine and backend filters together', () => {
    const scenarios: Scenario[] = [
      {
        id: 'legacy',
        name: 'Legacy',
        category: 'basic',
        description: 'legacy scenario',
        steps: ['waitForText:ask a question'],
        verify: ['screen.contains:ask a question'],
        engine: ['v2'],
        backend: ['acp-mock'],
      },
    ];

    expect(
      filterFor(scenarios, backend('acp-mock', 'v2')).map(
        (scenario) => scenario.id
      )
    ).toEqual(['legacy']);
    expect(filterFor(scenarios, backend('live', 'v2'))).toHaveLength(0);
    expect(filterFor(scenarios, backend('acp-mock', 'kas'))).toHaveLength(0);
  });

  it('treats missing engine and backend filters as enabled everywhere', () => {
    const scenarios: Scenario[] = [
      {
        id: 'defaulted',
        name: 'Defaulted',
        category: 'basic',
        description: 'runs everywhere',
        steps: ['waitForText:ask a question'],
        verify: ['screen.contains:ask a question'],
      },
    ];

    expect(
      filterFor(scenarios, backend('live', 'kas')).map((scenario) => scenario.id)
    ).toEqual(['defaulted']);
    expect(
      filterFor(scenarios, backend('acp-mock', 'kas')).map(
        (scenario) => scenario.id
      )
    ).toEqual(['defaulted']);
  });

  it('loads scenarios from disk without target metadata', () => {
    tempDir = mkdtempSync(join(tmpdir(), 'kiro-scenario-filters-'));
    const scenariosPath = join(tempDir, 'scenarios.json');
    writeFileSync(
      scenariosPath,
      JSON.stringify({
        version: '2.0.0',
        scenarios: [
          {
            id: 'disk-scenario',
            name: 'Disk scenario',
            category: 'basic',
            description: 'scenario from disk',
            steps: ['waitForText:ask a question'],
            verify: ['screen.contains:ask a question'],
            engine: ['kas'],
            backend: ['live'],
          },
        ],
      })
    );

    const scenarios = loadScenarios(scenariosPath);
    expect(scenarios).toHaveLength(1);
    expect(
      filterFor(scenarios, backend('live', 'kas')).map((scenario) => scenario.id)
    ).toEqual(['disk-scenario']);
    expect(
      filterFor(scenarios, backend('acp-mock', 'kas'))
    ).toHaveLength(0);
  });
});
