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
  snapshotReportsSources,
  CONFIG_SUBCOMMANDS,
  type ConfigSnapshot,
} from '../config-panel-model.js';

const emptySnapshot: ConfigSnapshot = {
  cloudSession: false,
  sourcesReported: true,
  agents: [],
  mcpServers: [],
  steering: [],
  steeringDocs: [],
  skills: [],
  hooks: [],
  powers: [],
  diagnostics: [],
};

const populatedSnapshot: ConfigSnapshot = {
  cloudSession: false,
  sourcesReported: true,
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
  diagnostics: [],
};

describe('buildCategoryRows', () => {
  it('lists the six live categories in mock order (env + secrets deferred)', () => {
    const rows = buildCategoryRows(emptySnapshot);
    expect(rows.map((r) => r.id)).toEqual([
      'agents',
      'mcp',
      'powers',
      'steering',
      'skills',
      'hooks',
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
  });

  it('does not surface a deferred env row', () => {
    const rows = buildCategoryRows(populatedSnapshot);
    expect(rows.find((r) => r.id === 'env')).toBeUndefined();
  });
});

describe('resolveCategorySelect', () => {
  it('routes MCP to the shared /mcp panel, not a config page', () => {
    expect(resolveCategorySelect('mcp')).toEqual({ kind: 'open-mcp' });
  });

  it('routes hooks to the existing /hooks panel', () => {
    expect(resolveCategorySelect('hooks')).toEqual({ kind: 'open-hooks' });
  });

  it('routes agents to the selectable /agent picker (fnf board), not a page', () => {
    expect(resolveCategorySelect('agents')).toEqual({ kind: 'open-agent' });
  });

  it('env and secrets are deferred: no alias (typed form alerts), select inert', () => {
    for (const token of ['env', 'environment', 'secrets']) {
      expect(resolveConfigSubcommand(token)).toBeUndefined();
    }
    // Both deferred ids are inert on select — a reintroduced row must not
    // open a blank page.
    expect(resolveCategorySelect('env')).toEqual({ kind: 'none' });
    expect(resolveCategorySelect('secrets')).toEqual({ kind: 'none' });
  });

  it('other categories open in-panel pages', () => {
    for (const id of ['powers', 'steering', 'skills'] as const) {
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
    expect(resolveConfigSubcommand('POWER')).toBe('powers');
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

  it('cloud sessions read cloud for everything (per UX); descriptors decide locally', () => {
    // A cloud session's whole config surface lives in the sandbox — even a
    // user/workspace-origin item reads "cloud" there.
    expect(effectiveSource(true, 'local')).toBe('cloud');
    // Locally the descriptor origin decides: a cloud-synced item is cloud.
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

describe('source column gating (V2 reports no origins)', () => {
  const v2Snapshot: ConfigSnapshot = {
    ...populatedSnapshot,
    sourcesReported: false,
  };

  it('pages drop the Source column when sources are not reported', () => {
    const steering = buildCategoryPage('steering', v2Snapshot);
    expect(steering.columns).toEqual(['Name', 'Inclusion']);
    // No 'local' placement guess anywhere in the rows.
    expect(steering.rows.flat()).not.toContain('local');
    const skills = buildCategoryPage('skills', v2Snapshot);
    expect(skills.columns).toEqual(['Name', 'Description']);
  });

  it('pages keep the Source column when sources are reported', () => {
    const page = buildCategoryPage('steering', populatedSnapshot);
    expect(page.columns).toContain('Source');
  });

  it('footers drop the conflict/cloud-edit lines with the column', () => {
    // A page that reports no origins must not explain how origins conflict.
    const page = buildCategoryPage('skills', v2Snapshot);
    const joined = page.footerLines.join(' ');
    expect(joined).not.toContain('conflict');
    expect(joined).not.toContain('app.kiro.dev');
    // The local-edit path is engine-independent and stays.
    expect(joined).toContain('To edit local configs');
    // Reported sources keep the full footer set.
    const kasPage = buildCategoryPage('skills', populatedSnapshot);
    expect(kasPage.footerLines.join(' ')).toContain('conflict');
  });

  it('snapshotReportsSources: fact-based, not engine-based', () => {
    const bare = { ...populatedSnapshot };
    // populatedSnapshot's MCP servers carry source fields → reported.
    expect(snapshotReportsSources(bare)).toBe(true);
    // Strip every origin: a descriptor-free local session reports nothing,
    // regardless of engine.
    const stripped = {
      ...populatedSnapshot,
      mcpServers: populatedSnapshot.mcpServers.map((s) => ({
        ...s,
        source: undefined,
      })),
    };
    expect(snapshotReportsSources(stripped)).toBe(false);
    // A cloud session always reports (placement 'cloud' is a fact there).
    expect(snapshotReportsSources({ ...stripped, cloudSession: true })).toBe(
      true
    );
    // A single descriptor-tagged hook flips it back on.
    expect(
      snapshotReportsSources({
        ...stripped,
        hooks: [
          { trigger: 'preToolUse', command: 'x.sh', configSource: 'cloud' },
        ],
      })
    ).toBe(true);
  });
});

describe('buildCategoryPage', () => {
  it('agents never renders an in-panel page (routes to the /agent picker)', () => {
    const page = buildCategoryPage('agents', populatedSnapshot);
    expect(page.columns).toEqual([]);
    expect(page.rows).toEqual([]);
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
});
