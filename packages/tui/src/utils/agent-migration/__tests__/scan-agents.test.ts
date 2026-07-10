/**
 * Tests for `scanAgents` (the `/upgrade-agent` classifier) — classification,
 * directory handling, and the per-scope/classification `counts` tally. Each
 * test writes agent files into isolated temp dirs, runs the scan, and asserts
 * the result.
 */

import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { scanAgents, type AgentScope } from '../scan.js';

interface Row {
  name: string;
  scope: string;
  classification: string;
}

let root: string;
let localDir: string;
let globalDir: string;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'scan-case-'));
  localDir = join(root, 'local');
  globalDir = join(root, 'global');
  mkdirSync(localDir, { recursive: true });
  mkdirSync(globalDir, { recursive: true });
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

function writeFiles(dir: string, files: Record<string, unknown> = {}): void {
  for (const [name, content] of Object.entries(files)) {
    writeFileSync(join(dir, name), JSON.stringify(content, null, 2));
  }
}

function sortRows(rows: Row[]): Row[] {
  return [...rows].sort((a, b) =>
    `${a.name}/${a.scope}`.localeCompare(`${b.name}/${b.scope}`)
  );
}

/** Write the given files, scan both dirs, and return the sorted (name, scope, classification) rows. */
function scan(
  local: Record<string, unknown> = {},
  global: Record<string, unknown> = {}
): Row[] {
  writeFiles(localDir, local);
  writeFiles(globalDir, global);
  const result = scanAgents([
    { dir: localDir, scope: 'local' as AgentScope },
    { dir: globalDir, scope: 'global' as AgentScope },
  ]);
  return sortRows(
    result.agents.map((a) => ({
      name: a.name,
      scope: a.scope,
      classification: a.classification,
    }))
  );
}

describe('scanAgents — classification', () => {
  test('v2-only: V2 marker tools, no V3 trust field', () => {
    expect(scan({ 'a.json': { tools: ['fs_read'] } })).toEqual([
      { name: 'a', scope: 'local', classification: 'v2-only' },
    ]);
  });

  test('v2-only: allowedTools triggers V2 trust even with no V2 marker tools', () => {
    expect(
      scan({ 'b.json': { tools: ['read'], allowedTools: ['fs_read'] } })
    ).toEqual([{ name: 'b', scope: 'local', classification: 'v2-only' }]);
  });

  test('v2-only: toolsSettings triggers V2 trust', () => {
    expect(
      scan({
        'c.json': {
          tools: ['execute_bash'],
          toolsSettings: { execute_bash: { allowedCommands: ['git status'] } },
        },
      })
    ).toEqual([{ name: 'c', scope: 'local', classification: 'v2-only' }]);
  });

  test('universal-out-of-sync: V3 trust does not match a fresh derivation from V2', () => {
    expect(
      scan({
        'o.json': {
          tools: ['fs_read', 'fs_write'],
          permissions: { rules: [] },
        },
      })
    ).toEqual([
      { name: 'o', scope: 'local', classification: 'universal-out-of-sync' },
    ]);
  });

  test('universal-in-sync: V3 trust matches a fresh derivation from V2', () => {
    expect(
      scan({
        'u.json': {
          tools: ['shell'],
          toolsSettings: {
            execute_bash: {
              allowedCommands: ['git status', 'ls'],
              deniedCommands: ['rm -rf'],
            },
          },
          permissions: {
            rules: [
              {
                capability: 'shell',
                match: ['git status', 'ls'],
                effect: 'allow',
              },
              { capability: 'shell', match: ['rm -rf'], effect: 'deny' },
            ],
          },
        },
      })
    ).toEqual([
      { name: 'u', scope: 'local', classification: 'universal-in-sync' },
    ]);
  });

  test('v3-only: V3 trust field with no V2 trust', () => {
    expect(
      scan({ 'k.json': { tools: ['read'], permissions: { rules: [] } } })
    ).toEqual([{ name: 'k', scope: 'local', classification: 'v3-only' }]);
  });

  test('v3-only: only includePowers (V3-only field) present', () => {
    expect(
      scan({ 'p.json': { prompt: 'you help with X', includePowers: ['foo'] } })
    ).toEqual([{ name: 'p', scope: 'local', classification: 'v3-only' }]);
  });
});

describe('scanAgents — directory handling', () => {
  test('agents are classified per scope across local and global dirs', () => {
    expect(
      scan(
        { 'a.json': { tools: ['fs_read'] } },
        { 'b.json': { tools: ['fs_write'] } }
      )
    ).toEqual([
      { name: 'a', scope: 'local', classification: 'v2-only' },
      { name: 'b', scope: 'global', classification: 'v2-only' },
    ]);
  });

  test('non-agent JSON (e.g. mcp.json) is ignored', () => {
    expect(
      scan({ 'mcp.json': { mcpServers: {} }, 'a.json': { tools: ['fs_read'] } })
    ).toEqual([{ name: 'a', scope: 'local', classification: 'v2-only' }]);
  });

  test('backup files (.bak) are excluded from the scan', () => {
    expect(
      scan({
        'a.json': { tools: ['fs_read'] },
        'a.json.bak': { tools: ['fs_read'] },
        'a.json.bak.1': { tools: ['fs_read'] },
      })
    ).toEqual([{ name: 'a', scope: 'local', classification: 'v2-only' }]);
  });

  test('AIM-managed agents are excluded so the upgrade never touches them', () => {
    expect(
      scan({
        'managed.json': {
          description: 'A workspace agent managed by AIM',
          tools: ['fs_read'],
        },
        'mine.json': { tools: ['fs_read'] },
      })
    ).toEqual([{ name: 'mine', scope: 'local', classification: 'v2-only' }]);
  });
});

describe('scanAgents — counts', () => {
  test('tallies by scope and classification, exposes per-agent warnings', () => {
    // V2-only with a lossy regex (content char class → emits regex-shell-pattern warning).
    writeFiles(localDir, {
      'lossy.json': {
        name: 'lossy',
        tools: ['execute_bash'],
        toolsSettings: {
          execute_bash: { allowedCommands: ['sleep [0-9]+'] },
        },
      },
    });
    // V2-only with autoAllowReadonly (maps cleanly to read-only-shell; no warning).
    writeFiles(globalDir, {
      'readonly.json': {
        name: 'readonly',
        tools: ['execute_bash'],
        toolsSettings: { execute_bash: { autoAllowReadonly: true } },
      },
      // V3-only — V2 skips, no warnings.
      'kas-only.json': {
        name: 'kas-only',
        tools: ['read'],
        permissions: { rules: [] },
      },
    });

    const scan = scanAgents([
      { dir: localDir, scope: 'local' as AgentScope },
      { dir: globalDir, scope: 'global' as AgentScope },
    ]);

    expect(scan.total).toBe(3);
    expect(scan.counts['v2-only'].total).toBe(2);
    expect(scan.counts['v2-only'].local).toBe(1);
    expect(scan.counts['v2-only'].global).toBe(1);
    expect(scan.counts['v3-only'].total).toBe(1);

    const withWarnings = scan.agents
      .filter((a) => a.warnings.length > 0)
      .map((a) => a.name)
      .sort();
    expect(withWarnings).toEqual(['lossy']);
    const lossy = scan.agents.find((a) => a.name === 'lossy');
    expect(lossy?.warnings.map((w) => w.kind)).toEqual(['regex-shell-pattern']);
  });

  test('empty workspace produces zero counts and no warnings', () => {
    const scan = scanAgents([
      { dir: localDir, scope: 'local' as AgentScope },
      { dir: globalDir, scope: 'global' as AgentScope },
    ]);
    expect(scan.total).toBe(0);
    expect(scan.counts['v2-only'].total).toBe(0);
    expect(scan.counts['universal-in-sync'].total).toBe(0);
    expect(scan.agents).toEqual([]);
  });
});
