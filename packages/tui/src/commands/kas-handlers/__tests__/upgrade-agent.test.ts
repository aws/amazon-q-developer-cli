/**
 * Unit tests for the `/upgrade-agent` command handler.
 *
 * The interesting logic — picker option construction, diagnostics row building,
 * bucket-value parsing, and the upgrade summary — is extracted into pure
 * functions and tested directly with hand-built `ScanResult`/outcome inputs (no
 * filesystem, no `mock.module`). A few thin routing tests exercise the handler
 * itself against an empty real scan dir.
 */

import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import {
  handleUpgradeAgent,
  buildUpgradeBuckets,
  buildDiagnosticsRows,
  parseBucketValue,
  summarizeBucketUpgrade,
} from '../upgrade-agent';
import type { DispatchOptions } from '../../dispatcher';
import {
  createMockCommandContext,
  type MockCommandContext,
} from '../../__tests__/test-helpers';
import { KasCommandName, type KasCommand } from '../../../kas-commands';
import type {
  ScanResult,
  ScannedAgent,
  AgentClassification,
  AgentScope,
  AgentUpgradeOutcome,
  MigrationWarning,
} from '../../../utils/agent-migration/index.js';

const cmd = { name: KasCommandName.UpgradeAgent } as unknown as KasCommand;
const synthetic: DispatchOptions = { argIsSynthetic: true };

function makeScan(agents: ScannedAgent[]): ScanResult {
  const zero = () => ({ local: 0, global: 0, total: 0 });
  const counts = {
    'v2-only': zero(),
    'universal-out-of-sync': zero(),
    'universal-in-sync': zero(),
    'v3-only': zero(),
  } as ScanResult['counts'];
  for (const a of agents) {
    counts[a.classification][a.scope] += 1;
    counts[a.classification].total += 1;
  }
  return { agents, counts, total: agents.length };
}

function agent(
  name: string,
  scope: AgentScope,
  classification: AgentClassification,
  warnings: MigrationWarning[] = []
): ScannedAgent {
  return {
    name,
    scope,
    classification,
    sourcePath: `/ws/${name}.json`,
    warnings,
  };
}

function outcome(
  sourcePath: string,
  status: AgentUpgradeOutcome['status'],
  warnings: MigrationWarning[] = []
): AgentUpgradeOutcome {
  return {
    name: sourcePath,
    sourcePath,
    classification: 'v2-only',
    status,
    warnings,
  };
}

describe('buildUpgradeBuckets', () => {
  test('builds 3-column options per (bucket × scope), skipping empty buckets', () => {
    const { options } = buildUpgradeBuckets(
      makeScan([
        agent('a', 'local', 'v2-only'),
        agent('b', 'local', 'v2-only'),
        agent('c', 'global', 'universal-out-of-sync'),
        agent('d', 'global', 'universal-out-of-sync'),
        agent('e', 'global', 'universal-out-of-sync'),
        agent('f', 'local', 'v3-only'), // not actionable → no row
      ])
    );
    expect(options).toEqual([
      {
        value: 'v2-only:local',
        label: 'V2 [2 agents]',
        group: 'Workspace',
        description: 'Upgrade to universal (V2 + V3) config',
      },
      {
        value: 'universal-out-of-sync:global',
        label: 'Universal · Out of Sync [3 agents]',
        group: 'Global',
        description: 'Rebuild V3 from V2 (overwrites manual V3 edits)',
      },
    ]);
  });

  test('stashes each bucket value → its sorted agent names for the preview panel', () => {
    const { preview } = buildUpgradeBuckets(
      makeScan([agent('b', 'local', 'v2-only'), agent('a', 'local', 'v2-only')])
    );
    expect(preview).toEqual({ 'v2-only:local': ['a', 'b'] });
  });

  test('singular label for a single agent', () => {
    const { options } = buildUpgradeBuckets(
      makeScan([agent('a', 'local', 'v2-only')])
    );
    expect(options[0]!.label).toBe('V2 [1 agent]');
  });

  test('nothing actionable → no options', () => {
    const { options } = buildUpgradeBuckets(
      makeScan([
        agent('k', 'local', 'v3-only'),
        agent('s', 'local', 'universal-in-sync'),
      ])
    );
    expect(options).toEqual([]);
  });
});

describe('buildDiagnosticsRows', () => {
  test('lists only in-sync agents with their warnings', () => {
    const warn: MigrationWarning = {
      kind: 'unmapped-allowed-tool',
      detail: 'my_tool',
    };
    const result = buildDiagnosticsRows(
      makeScan([
        agent('synced', 'local', 'universal-in-sync', [warn]),
        agent('pending', 'global', 'v2-only'), // excluded from diagnostics
      ])
    );
    expect(result).toEqual({
      desc: '1 Universal agent',
      rows: [
        {
          name: 'synced',
          scope: 'local',
          warnings: [{ kind: 'unmapped-allowed-tool', detail: 'my_tool' }],
        },
      ],
    });
  });

  test('no in-sync agents → null', () => {
    const result = buildDiagnosticsRows(
      makeScan([
        agent('a', 'local', 'v2-only'),
        agent('k', 'global', 'v3-only'),
      ])
    );
    expect(result).toBeNull();
  });
});

describe('parseBucketValue', () => {
  test('splits classification:scope', () => {
    expect(parseBucketValue('v2-only:local')).toEqual({
      classification: 'v2-only',
      scope: 'local',
    });
  });

  test('malformed value (no colon) → null', () => {
    expect(parseBucketValue('garbage')).toBeNull();
  });
});

describe('summarizeBucketUpgrade', () => {
  test('counts upgraded agents (success)', () => {
    const { message, level } = summarizeBucketUpgrade([
      outcome('/ws/a.json', 'upgraded'),
      outcome('/ws/b.json', 'upgraded'),
    ]);
    expect(message).toBe('Upgraded 2 agents (backed up to .json.bak)');
    expect(level).toBe('success');
  });

  test('surfaces warnings and errors in the summary', () => {
    const { message, level } = summarizeBucketUpgrade([
      outcome('/ws/a.json', 'upgraded', [{ kind: 'unmapped-allowed-tool' }]),
      outcome('/ws/b.json', 'error'),
    ]);
    expect(message).toBe(
      'Upgraded 1 agent · 1 with warnings — run /upgrade-agent diagnostics to review · 1 failed (backed up to .json.bak)'
    );
    expect(level).toBe('error');
  });

  test('no changes written when nothing upgraded', () => {
    const { message } = summarizeBucketUpgrade([]);
    expect(message).toBe('No changes written (backed up to .json.bak)');
  });
});

// Thin routing tests: exercise the handler against an empty real scan dir
// (KIRO_HOME + cwd redirected so the scan finds nothing), so no module mocking.
describe('handleUpgradeAgent — routing', () => {
  let tmp: string;
  let prevCwd: string;
  let prevKiroHome: string | undefined;

  beforeEach(() => {
    tmp = mkdtempSync(join(tmpdir(), 'upgrade-agent-routing-'));
    prevCwd = process.cwd();
    prevKiroHome = process.env.KIRO_HOME;
    process.chdir(tmp);
    process.env.KIRO_HOME = join(tmp, 'home');
  });

  afterEach(() => {
    process.chdir(prevCwd);
    if (prevKiroHome === undefined) delete process.env.KIRO_HOME;
    else process.env.KIRO_HOME = prevKiroHome;
    rmSync(tmp, { recursive: true, force: true });
  });

  async function run(
    args: string,
    opts?: DispatchOptions
  ): Promise<MockCommandContext> {
    const ctx = createMockCommandContext();
    await handleUpgradeAgent(cmd, args, ctx, opts);
    return ctx;
  }

  test('unknown non-synthetic argument shows an error alert', async () => {
    const ctx = await run('wat');
    expect(ctx._spies.showAlert!.mock.calls[0]!).toEqual([
      'Unknown /upgrade-agent argument: wat',
      'error',
      3000,
    ]);
    expect(ctx._spies.setActiveCommand).not.toHaveBeenCalled();
  });

  test('empty arg with no agents → success alert, no picker', async () => {
    const ctx = await run('');
    expect(ctx._spies.setActiveCommand).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe(
      'No agents to upgrade — every agent is already in sync or V3-only'
    );
  });

  test('diagnostics with no agents → warning alert', async () => {
    const ctx = await run('diagnostics');
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe(
      'No agent configs found in .kiro/agents/'
    );
    expect(ctx._spies.setUpgradeDiagnostics).not.toHaveBeenCalled();
  });

  test('synthetic bucket selection with no matching agents → warning', async () => {
    const ctx = await run('v2-only:local', synthetic);
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe(
      'Nothing to upgrade in that bucket (state changed since picker)'
    );
  });

  test('malformed synthetic value (no colon) → invalid-option error', async () => {
    const ctx = await run('garbage', synthetic);
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toBe(
      'Invalid /upgrade-agent option: garbage'
    );
  });
});
