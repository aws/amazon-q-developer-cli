/**
 * Unit tests for the `/config` panel model (cloud config UX, Figma frames
 * 23-41). Covers category rows, source labelling per session placement,
 * subcommand routing (`/config mcp` must reach the /mcp view, never a
 * duplicate page), footers, and page building.
 */

import { describe, it, expect } from 'bun:test';
import {
  buildCategoryRows,
  buildCategoryPage,
  buildTopFooterLines,
  resolveCategorySelect,
  resolveConfigSubcommand,
  effectiveSource,
  collectKiroEnv,
  CONFIG_SUBCOMMANDS,
  type ConfigSnapshot,
} from '../config-panel-model.js';

const emptySnapshot: ConfigSnapshot = {
  cloudSession: false,
  agents: [],
  mcpServers: [],
  steering: [],
  steeringDocs: [],
  skills: [],
  hooks: [],
  powers: [],
  kiroEnv: [],
  diagnostics: [],
};

const populatedSnapshot: ConfigSnapshot = {
  cloudSession: false,
  agents: [
    { id: 'default', name: 'Default', description: 'Bundled default agent' },
    { id: 'spec', name: 'Spec' },
  ],
  mcpServers: [
    { name: 'github', status: 'running', toolCount: 26, source: 'local' },
    { name: 'aws-docs', status: 'disabled', toolCount: 7, source: 'cloud' },
    { name: 'fetch', status: 'running', toolCount: 0, source: 'local' },
  ],
  steering: [
    { name: 'house-rules', source: { kind: 'workspace' } },
    { name: 'org-style', source: { kind: 'global' } },
  ],
  steeringDocs: [],
  skills: [{ name: 'figma-to-code', source: { kind: 'workspace' } }],
  hooks: [{ trigger: 'preToolUse', command: 'lint.sh' }],
  powers: [],
  kiroEnv: [
    ['KIRO_INTERNAL', '1'],
    ['KIRO_VERSION', '2.13.1'],
  ],
  diagnostics: [],
};

describe('buildCategoryRows', () => {
  it('lists the seven live categories in mock order (secrets deferred)', () => {
    const rows = buildCategoryRows(emptySnapshot);
    expect(rows.map((r) => r.id)).toEqual([
      'agents',
      'mcp',
      'powers',
      'steering',
      'skills',
      'hooks',
      'env',
    ]);
  });

  it('renders em-dash placeholders when a category has no data', () => {
    const rows = buildCategoryRows(emptySnapshot);
    const mcp = rows.find((r) => r.id === 'mcp')!;
    expect(mcp.source).toBe('—');
    expect(mcp.status).toBe('—');
  });

  it('summarizes mixed MCP sources as "local + cloud"', () => {
    const rows = buildCategoryRows(populatedSnapshot);
    const mcp = rows.find((r) => r.id === 'mcp')!;
    expect(mcp.source).toBe('local + cloud');
    expect(mcp.status).toBe('2 active'); // only running servers count
  });

  it('summarizes descriptor-tagged items as local + cloud in a local session', () => {
    // Per-item configSource (from the KAS ConfigResource descriptor) drives
    // the summary even in a local session — a cloud-synced steering file
    // makes the category read "local + cloud" (frame 23).
    const rows = buildCategoryRows({
      ...populatedSnapshot,
      steering: [
        { name: 'local-doc', source: { kind: 'workspace' } },
        {
          name: 'cloud-doc',
          source: { kind: 'global' },
          configSource: 'cloud',
        },
      ],
      skills: [
        {
          name: 'cloud-skill',
          source: { kind: 'global' },
          configSource: 'cloud',
        },
      ],
    });
    expect(rows.find((r) => r.id === 'steering')!.source).toBe('local + cloud');
    expect(rows.find((r) => r.id === 'skills')!.source).toBe('cloud');
  });

  it('labels every populated category cloud in a cloud session', () => {
    const rows = buildCategoryRows({
      ...populatedSnapshot,
      cloudSession: true,
      mcpServers: populatedSnapshot.mcpServers.map((s) => ({
        ...s,
        source: undefined,
      })),
    });
    for (const id of ['agents', 'mcp', 'steering', 'skills', 'hooks']) {
      expect(rows.find((r) => r.id === id)!.source).toBe('cloud');
    }
  });

  it('counts statuses per category', () => {
    const rows = buildCategoryRows(populatedSnapshot);
    expect(rows.find((r) => r.id === 'agents')!.status).toBe('2 available');
    expect(rows.find((r) => r.id === 'steering')!.status).toBe('2 files');
    expect(rows.find((r) => r.id === 'skills')!.status).toBe('1 skill');
    expect(rows.find((r) => r.id === 'hooks')!.status).toBe('1 configured');
    expect(rows.find((r) => r.id === 'env')!.status).toBe('2 configured');
  });
});

describe('resolveCategorySelect', () => {
  it('routes MCP to the shared /mcp panel, not a config page', () => {
    expect(resolveCategorySelect('mcp')).toEqual({ kind: 'open-mcp' });
  });

  it('routes hooks to the existing /hooks panel', () => {
    expect(resolveCategorySelect('hooks')).toEqual({ kind: 'open-hooks' });
  });

  it('secrets is deferred: no alias (typed form alerts), select is inert', () => {
    expect(resolveConfigSubcommand('secrets')).toBeUndefined();
    expect(resolveCategorySelect('secrets')).toEqual({ kind: 'none' });
  });

  it('other categories open in-panel pages', () => {
    for (const id of [
      'agents',
      'powers',
      'steering',
      'skills',
      'env',
    ] as const) {
      expect(resolveCategorySelect(id)).toEqual({ kind: 'page', category: id });
    }
  });
});

describe('resolveConfigSubcommand', () => {
  it('maps every advertised subcommand to a category', () => {
    for (const sub of CONFIG_SUBCOMMANDS) {
      expect(resolveConfigSubcommand(sub)).toBeDefined();
    }
  });

  it('accepts aliases and mixed case', () => {
    expect(resolveConfigSubcommand('MCP')).toBe('mcp');
    expect(resolveConfigSubcommand('agent')).toBe('agents');
    expect(resolveConfigSubcommand('environment')).toBe('env');
  });

  it('rejects unknown tokens', () => {
    expect(resolveConfigSubcommand('nonsense')).toBeUndefined();
  });
});

describe('effectiveSource', () => {
  it('placement decides when no explicit source', () => {
    expect(effectiveSource(true)).toBe('cloud');
    expect(effectiveSource(false)).toBe('local');
  });

  it('an explicit source wins over placement', () => {
    expect(effectiveSource(true, 'local')).toBe('local');
    expect(effectiveSource(false, 'cloud')).toBe('cloud');
  });
});

describe('buildTopFooterLines', () => {
  it('cloud session gets the cloud-edit footer', () => {
    const lines = buildTopFooterLines(true);
    expect(lines.join(' ')).toContain('kiro.dev/settings');
    expect(lines).toHaveLength(1);
  });

  it('local session explains cloud vs local scope', () => {
    const lines = buildTopFooterLines(false);
    expect(lines.length).toBeGreaterThan(1);
    expect(lines.join(' ')).toContain('kiro.dev/settings');
  });

  it('leads with cloud-config diagnostics when present', () => {
    const lines = buildTopFooterLines(true, [
      { severity: 'warning', message: 'Using your last synced settings' },
      { severity: 'error', message: 'A steering file failed to sync' },
    ]);
    // Diagnostics come first, severity-marked, before the edit line.
    expect(lines[0]).toContain('Using your last synced settings');
    expect(lines[0]).toContain('⚠');
    expect(lines[1]).toContain('A steering file failed to sync');
    expect(lines[1]).toContain('✗');
    expect(lines[lines.length - 1]).toContain('kiro.dev/settings');
  });

  it('adds nothing for an empty diagnostics set (not a health claim)', () => {
    expect(buildTopFooterLines(true, [])).toEqual(buildTopFooterLines(true));
  });

  it('unknown severities render unmarked, after marked lines', () => {
    const lines = buildTopFooterLines(true, [
      { severity: 'info', message: 'Sync completed from cache' },
      { severity: 'error', message: 'A file failed to sync' },
    ]);
    // error first (marked), info after it (no glyph masquerade).
    expect(lines[0]).toContain('✗');
    expect(lines[0]).toContain('A file failed to sync');
    expect(lines[1]).toBe('Sync completed from cache');
  });
});

describe('buildCategoryPage', () => {
  it('agents page has Agent|Source|Install|Details columns and one row per agent', () => {
    const page = buildCategoryPage('agents', {
      ...populatedSnapshot,
      agents: [
        {
          id: 'default',
          name: 'Default',
          description: 'Bundled default agent',
          source: 'bundled',
        },
        { id: 'spec', name: 'Spec', source: 'workspace' },
        { id: 'plan', name: 'Plan' },
      ],
    });
    expect(page.columns).toEqual(['Agent', 'Source', 'Install', 'Details']);
    expect(page.rows).toHaveLength(3);
    expect(page.rows[0]).toEqual([
      'Default',
      'local',
      'Bundled',
      'Bundled default agent',
    ]);
    expect(page.rows[1]).toEqual(['Spec', 'local', 'Workspace', '']);
    // No source metadata → blank Install, never a guess.
    expect(page.rows[2]).toEqual(['Plan', 'local', '', '']);
  });

  it('unknown Install source values render blank, not raw passthrough', () => {
    const page = buildCategoryPage('agents', {
      ...populatedSnapshot,
      agents: [{ id: 'x', name: 'X', source: 'some-future-source' }],
    });
    expect(page.rows[0]).toEqual(['X', 'local', '', '']);
  });

  it('steering page shows Inclusion column with scope kind', () => {
    const page = buildCategoryPage('steering', populatedSnapshot);
    expect(page.columns).toEqual(['Name', 'Source', 'Inclusion']);
    expect(page.rows[0]).toEqual(['house-rules', 'local', 'workspace']);
  });

  it('page rows honor per-item descriptor sources over placement', () => {
    const page = buildCategoryPage('steering', {
      ...populatedSnapshot,
      steering: [
        { name: 'local-doc', source: { kind: 'workspace' } },
        {
          name: 'cloud-doc',
          source: { kind: 'global' },
          configSource: 'cloud',
        },
      ],
    });
    expect(page.rows[0]?.[1]).toBe('local');
    expect(page.rows[1]?.[1]).toBe('cloud');
  });

  it('cloud session pages label sources cloud and drop local-edit hints', () => {
    const page = buildCategoryPage('skills', {
      ...populatedSnapshot,
      cloudSession: true,
    });
    expect(page.rows[0]?.[1]).toBe('cloud');
    expect(page.footerLines.join(' ')).not.toContain('conflict');
  });

  it('local session pages include the conflict-precedence line', () => {
    const page = buildCategoryPage('skills', populatedSnapshot);
    expect(page.footerLines[0]).toContain('local will override cloud');
  });

  it('powers page is an empty state when nothing is installed', () => {
    const page = buildCategoryPage('powers', populatedSnapshot);
    expect(page.rows).toHaveLength(0);
    expect(page.emptyMessage).toBeTruthy();
  });

  it('powers page lists installed powers with displayName and descriptor source', () => {
    const page = buildCategoryPage('powers', {
      ...populatedSnapshot,
      powers: [
        {
          name: 'aws-tools',
          displayName: 'AWS Tools',
          description: 'AWS helpers',
          configSource: 'cloud',
        },
        { name: 'figma', description: 'Figma to code' },
      ],
    });
    expect(page.columns).toEqual(['Name', 'Source', 'Description']);
    expect(page.rows[0]).toEqual(['AWS Tools', 'cloud', 'AWS helpers']);
    expect(page.rows[1]).toEqual(['figma', 'local', 'Figma to code']);
  });

  it('steering page prefers documents_changed rows with real inclusion mode', () => {
    const page = buildCategoryPage('steering', {
      ...populatedSnapshot,
      steeringDocs: [
        { name: 'team', scope: 'global', inclusion: 'always' },
        {
          name: 'api-rules',
          scope: 'workspace',
          inclusion: 'fileMatch',
          configSource: 'cloud',
        },
        { name: 'no-mode', scope: 'workspace' },
      ],
    });
    // documents_changed listing wins over the slash-command projection
    expect(page.rows).toHaveLength(3);
    expect(page.rows[0]).toEqual(['team', 'local', 'always']);
    expect(page.rows[1]).toEqual(['api-rules', 'cloud', 'fileMatch']);
    // inclusion absent → scope kind as the fallback fact
    expect(page.rows[2]).toEqual(['no-mode', 'local', 'workspace']);
  });

  it('category rows count steeringDocs when present', () => {
    const rows = buildCategoryRows({
      ...populatedSnapshot,
      steeringDocs: [
        { name: 'a', scope: 'global' },
        { name: 'b', scope: 'global' },
        { name: 'c', scope: 'workspace', configSource: 'cloud' },
      ],
    });
    const steering = rows.find((r) => r.id === 'steering')!;
    expect(steering.status).toBe('3 files');
    expect(steering.source).toBe('local + cloud');
    const powers = rows.find((r) => r.id === 'powers')!;
    expect(powers.status).toBe('—');
  });

  it('powers category row counts installed powers', () => {
    const rows = buildCategoryRows({
      ...populatedSnapshot,
      powers: [{ name: 'aws-tools', configSource: 'cloud' }],
    });
    const powers = rows.find((r) => r.id === 'powers')!;
    expect(powers.status).toBe('1 installed');
    expect(powers.source).toBe('cloud');
  });

  it('env page lists KIRO_* pairs', () => {
    const page = buildCategoryPage('env', populatedSnapshot);
    expect(page.columns).toEqual(['Name', 'Value']);
    expect(page.rows).toEqual([
      ['KIRO_INTERNAL', '1'],
      ['KIRO_VERSION', '2.13.1'],
    ]);
  });
});

describe('collectKiroEnv', () => {
  it('filters to KIRO_* keys, sorts, and truncates long values', () => {
    const pairs = collectKiroEnv({
      PATH: '/usr/bin',
      KIRO_ZETA: 'z',
      KIRO_ALPHA: 'a'.repeat(80),
      KIRO_UNDEF: undefined,
    });
    expect(pairs.map(([k]) => k)).toEqual(['KIRO_ALPHA', 'KIRO_ZETA']);
    expect(pairs[0]![1].endsWith('...')).toBe(true);
    expect(pairs[0]![1].length).toBe(60);
  });

  it('redacts credential-bearing values (never prints live secrets)', () => {
    const pairs = collectKiroEnv({
      KIRO_API_KEY: 'sk-live-abc123',
      KIRO_ACCESS_TOKEN: 'tok',
      KIRO_CLIENT_SECRET: 's3cret',
      KIRO_DB_PASSWORD: 'pw',
      KIRO_AWS_CREDENTIALS: 'creds',
      KIRO_AGENT_ENGINE: 'kas',
    });
    const byName = Object.fromEntries(pairs);
    expect(byName.KIRO_API_KEY).toBe('<redacted>');
    expect(byName.KIRO_ACCESS_TOKEN).toBe('<redacted>');
    expect(byName.KIRO_CLIENT_SECRET).toBe('<redacted>');
    expect(byName.KIRO_DB_PASSWORD).toBe('<redacted>');
    expect(byName.KIRO_AWS_CREDENTIALS).toBe('<redacted>');
    expect(byName.KIRO_AGENT_ENGINE).toBe('kas');
  });
});
