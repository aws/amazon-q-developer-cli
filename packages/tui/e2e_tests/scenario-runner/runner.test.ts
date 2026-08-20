import { afterEach, describe, expect, it } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { filterScenarios, loadScenarios, resolveScenarioBackend } from './runner';
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
  return filterScenarios(scenarios, {
    backend: target,
    resolveBackend: (id) => backend(id, target.engine),
  } as RunOptions);
}

describe('scenario filtering', () => {
  let tempDir: string | null = null;

  afterEach(() => {
    if (tempDir) {
      rmSync(tempDir, { recursive: true, force: true });
      tempDir = null;
    }
  });

  it('keeps a live-only scenario off an explicitly mocked lane', () => {
    const scenarios = loadScenarios();
    const ids = new Set(
      filterScenarios(scenarios, {
        backend: backend('acp-mock', 'kas'),
        resolveBackend: (id) => backend(id, 'kas'),
        laneOnly: true,
      } as RunOptions).map((scenario) => scenario.id)
    );

    expect(ids.has('slash-help')).toBe(true);
    expect(ids.has('slash-save')).toBe(false);
    expect(ids.has('slash-load')).toBe(false);
  });

  it('runs a live-only scenario under live when no lane was asked for', () => {
    const scenarios = loadScenarios();
    const selected = filterFor(scenarios, backend('acp-mock', 'kas'));
    const save = selected.find((scenario) => scenario.id === 'slash-save');

    expect(save?.sourceBackend).toBe('live');
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

  it('applies the engine filter to a scenario declaring one', () => {
    const scenarios: Scenario[] = [
      {
        id: 'legacy',
        name: 'Legacy',
        category: 'basic',
        description: 'legacy scenario',
        steps: ['waitForText:ask a question'],
        verify: ['screen.contains:ask a question'],
        engine: ['v2'],
      },
    ];

    expect(
      filterFor(scenarios, backend('acp-mock', 'v2')).map(
        (scenario) => scenario.id
      )
    ).toEqual(['legacy']);
    expect(filterFor(scenarios, backend('acp-mock', 'kas'))).toHaveLength(0);
  });

  it('treats a missing engine filter as enabled everywhere', () => {
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

  it('reads a single manifest as portable scenarios', () => {
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
          },
        ],
      })
    );

    const scenarios = loadScenarios(scenariosPath);
    expect(scenarios).toHaveLength(1);
    expect(scenarios[0]?.sourceBackend).toBeUndefined();
    expect(
      filterFor(scenarios, backend('live', 'kas')).map((scenario) => scenario.id)
    ).toEqual(['disk-scenario']);
    expect(filterFor(scenarios, backend('live', 'v2'))).toHaveLength(0);
  });
});

describe('backend derived from scenario location', () => {
  let tempDir: string | null = null;

  afterEach(() => {
    if (tempDir) {
      rmSync(tempDir, { recursive: true, force: true });
      tempDir = null;
    }
  });

  function writeScenario(dir: string, id: string, extra: object = {}): void {
    mkdirSync(join(tempDir!, dir), { recursive: true });
    writeFileSync(
      join(tempDir!, dir, `${id}.json`),
      JSON.stringify({
        version: '2.0.0',
        scenarios: [
          {
            id,
            name: id,
            category: 'basic',
            description: id,
            steps: ['waitForText:ask a question'],
            verify: ['screen.contains:ask a question'],
            ...extra,
          },
        ],
      })
    );
  }

  function root(): string {
    tempDir = mkdtempSync(join(tmpdir(), 'kiro-scenario-dirs-'));
    return tempDir;
  }

  it('tags a scenario with the backend its directory names', () => {
    const dir = root();
    writeScenario('krs-mock', 'pinned');
    writeScenario('shared', 'portable');

    const byId = new Map(
      loadScenarios(dir).map((scenario) => [scenario.id, scenario])
    );
    expect(byId.get('pinned')?.sourceBackend).toBe('krs-mock');
    expect(byId.get('portable')?.sourceBackend).toBeUndefined();
  });

  it('runs a portable scenario under the lane backend', () => {
    const dir = root();
    writeScenario('shared', 'portable');
    const [scenario] = loadScenarios(dir);

    const lane = backend('acp-mock', 'kas');
    expect(resolveScenarioBackend(scenario!, { backend: lane } as RunOptions).id).toBe(
      'acp-mock'
    );
  });

  it('runs a pinned scenario under its own backend, whatever the lane is', () => {
    const dir = root();
    writeScenario('krs-mock', 'pinned');
    const [scenario] = loadScenarios(dir);

    const krsMock = backend('krs-mock', 'kas');
    const resolved = resolveScenarioBackend(scenario!, {
      backend: backend('live', 'kas'),
      resolveBackend: () => krsMock,
    } as RunOptions);
    expect(resolved.id).toBe('krs-mock');
  });

  it('reports a pinned scenario the runner has no backend for', () => {
    const dir = root();
    writeScenario('krs-mock', 'pinned');
    const [scenario] = loadScenarios(dir);

    expect(() =>
      resolveScenarioBackend(scenario!, { backend: backend('live', 'kas') } as RunOptions)
    ).toThrow(/requires backend "krs-mock"/);
  });

  it('filters a pinned scenario on its own backend engine, not the lane one', () => {
    const dir = root();
    writeScenario('krs-mock', 'pinned', {
      engine: ['kas'],
      turns: [{ respond: { events: [{ type: 'text', content: 'hi' }] } }],
    });
    const scenarios = loadScenarios(dir);

    const kept = filterScenarios(scenarios, {
      backend: backend('live', 'v2'),
      resolveBackend: () => backend('krs-mock', 'kas'),
    } as RunOptions);
    expect(kept.map((scenario) => scenario.id)).toEqual(['pinned']);
  });

  it('rejects a directory that names no backend', () => {
    const dir = root();
    writeScenario('kas-model', 'mystery');

    expect(() => loadScenarios(dir)).toThrow(/Unknown scenario directory "kas-model"/);
  });

  it('rejects the same id in two directories', () => {
    const dir = root();
    writeScenario('shared', 'twice');
    writeScenario('krs-mock', 'twice');

    expect(() => loadScenarios(dir)).toThrow(/Duplicate scenario id "twice"/);
  });

  it('skips a portable scenario with no turns on the krs-mock lane', () => {
    const dir = root();
    writeScenario('shared', 'unscripted');
    writeScenario('shared', 'scripted', {
      turns: [{ respond: { events: [{ type: 'text', content: 'hi' }] } }],
    });

    const kept = filterScenarios(loadScenarios(dir), {
      backend: backend('krs-mock', 'kas'),
    } as RunOptions);
    expect(kept.map((scenario) => scenario.id)).toEqual(['scripted']);
  });

  it('keeps an unscripted scenario on every other lane', () => {
    const dir = root();
    writeScenario('shared', 'unscripted');

    for (const lane of ['live', 'acp-mock'] as const) {
      const kept = filterScenarios(loadScenarios(dir), {
        backend: backend(lane, 'kas'),
      } as RunOptions);
      expect(kept.map((scenario) => scenario.id)).toEqual(['unscripted']);
    }
  });
});
