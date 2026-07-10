/**
 * Tests for the universal-config upgrade I/O layer (`upgradeAgentFile`). Each
 * test writes agent files into an isolated temp dir, upgrades every candidate
 * file the way the `/upgrade-agent` handler does (list + upgrade per file), and
 * asserts outcome statuses, on-disk content, and `.bak` backups.
 */

import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import {
  mkdtempSync,
  writeFileSync,
  rmSync,
  readFileSync,
  existsSync,
} from 'fs';
import { tmpdir } from 'os';
import { join, basename } from 'path';

import { listAgentFiles, upgradeAgentFile } from '../io.js';

let dir: string;

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'io-case-'));
});

afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
});

/** Write `files` into the temp dir, upgrade each candidate, return status-by-filename. */
function run(files: Record<string, unknown>): Map<string, string> {
  for (const [name, content] of Object.entries(files)) {
    writeFileSync(join(dir, name), JSON.stringify(content, null, 2));
  }
  const outcomes = listAgentFiles(dir).map((p) => upgradeAgentFile(p));
  return new Map(outcomes.map((o) => [basename(o.sourcePath), o.status]));
}

function readJson(name: string): unknown {
  return JSON.parse(readFileSync(join(dir, name), 'utf-8'));
}

function exists(name: string): boolean {
  return existsSync(join(dir, name));
}

describe('upgradeAgentFile', () => {
  test('v2-only: source is upgraded in place; original is backed up to .bak', () => {
    const status = run({ 'a.json': { tools: ['fs_read'] } });
    expect(status.get('a.json')).toBe('upgraded');
    expect(readJson('a.json')).toEqual({ tools: ['read'] });
    expect(readJson('a.json.bak')).toEqual({ tools: ['fs_read'] });
  });

  test('universal-in-sync: V3 trust matches a fresh derivation; nothing is written', () => {
    const status = run({
      'u.json': {
        tools: ['shell'],
        toolsSettings: { execute_bash: { allowedCommands: ['git status'] } },
        permissions: {
          rules: [
            { capability: 'shell', match: ['git status'], effect: 'allow' },
          ],
        },
      },
    });
    expect(status.get('u.json')).toBe('skipped-in-sync');
    expect(exists('u.json.bak')).toBe(false);
  });

  test('universal-out-of-sync: re-derives tools but preserves hand-authored permissions; backs up original', () => {
    const status = run({
      'o.json': { tools: ['fs_read', 'fs_write'], permissions: { rules: [] } },
    });
    expect(status.get('o.json')).toBe('upgraded');
    // Tool spelling is fixed, but the hand-authored `permissions` (no V2 source
    // to re-derive it from) is preserved verbatim rather than deleted.
    expect(readJson('o.json')).toEqual({
      tools: ['read', 'write'],
      permissions: { rules: [] },
    });
    expect(readJson('o.json.bak')).toEqual({
      tools: ['fs_read', 'fs_write'],
      permissions: { rules: [] },
    });
  });

  test('v3-only: V3 trust with no V2 trust is left untouched', () => {
    const status = run({
      'k.json': { tools: ['read'], permissions: { rules: [] } },
    });
    expect(status.get('k.json')).toBe('skipped-v3-only');
    expect(exists('k.json.bak')).toBe(false);
  });

  test('non-agent JSON file (e.g. mcp.json) is skipped', () => {
    const status = run({ 'mcp.json': { mcpServers: {} } });
    expect(status.get('mcp.json')).toBe('skipped-not-agent');
    expect(exists('mcp.json.bak')).toBe(false);
  });

  test('externally-managed (AIM) agent is never rewritten, even given its path directly', () => {
    const status = run({
      'managed.json': {
        description: 'A workspace agent managed by AIM',
        tools: ['fs_read'],
      },
    });
    expect(status.get('managed.json')).toBe('skipped-managed');
    expect(readJson('managed.json')).toEqual({
      description: 'A workspace agent managed by AIM',
      tools: ['fs_read'],
    });
    expect(exists('managed.json.bak')).toBe(false);
  });

  test('second upgrade collides with existing .bak; numbered suffix used', () => {
    const status = run({
      'a.json': { tools: ['fs_read'] },
      'a.json.bak': { tools: ['fs_write'] },
    });
    expect(status.get('a.json')).toBe('upgraded');
    expect(readJson('a.json')).toEqual({ tools: ['read'] });
    expect(readJson('a.json.bak')).toEqual({ tools: ['fs_write'] });
    expect(readJson('a.json.bak.1')).toEqual({ tools: ['fs_read'] });
  });

  test('non-disruptive: V2 fields preserved; distinct read-family tools (grep/glob) kept, redundant alias spellings (fs_read/execute_bash) dropped in favor of tags', () => {
    const status = run({
      'v.json': {
        name: 'v',
        tools: ['fs_read', 'grep', 'glob', 'execute_bash'],
        allowedTools: ['fs_read'],
        toolsSettings: { execute_bash: { allowedCommands: ['git status'] } },
      },
    });
    expect(status.get('v.json')).toBe('upgraded');
    expect(readJson('v.json')).toEqual({
      name: 'v',
      tools: ['glob', 'grep', 'read', 'shell'],
      allowedTools: ['fs_read'],
      toolsSettings: { execute_bash: { allowedCommands: ['git status'] } },
      permissions: {
        rules: [
          { capability: 'shell', match: ['git status'], effect: 'allow' },
          { capability: 'fs_read', effect: 'allow' },
        ],
      },
    });
    expect(readJson('v.json.bak')).toEqual({
      name: 'v',
      tools: ['fs_read', 'grep', 'glob', 'execute_bash'],
      allowedTools: ['fs_read'],
      toolsSettings: { execute_bash: { allowedCommands: ['git status'] } },
    });
  });
});
