/**
 * Focused unit tests for the module's error paths and directory helpers —
 * the branches the data-driven suites don't reach (unreadable/invalid files,
 * default dir resolution).
 */

import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { upgradeAgentFile, defaultAgentDirs } from '../io.js';
import { defaultScanDirs } from '../scan.js';

describe('upgradeAgentFile — error / skip paths', () => {
  let dir: string;
  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'io-err-'));
  });
  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  test('missing file → error outcome (no throw)', () => {
    const outcome = upgradeAgentFile(join(dir, 'does-not-exist.json'));
    expect(outcome.status).toBe('error');
    expect(outcome.error).toContain('Failed to read');
  });

  test('invalid JSON → error outcome (no throw)', () => {
    const path = join(dir, 'bad.json');
    writeFileSync(path, '{ not valid json ');
    const outcome = upgradeAgentFile(path);
    expect(outcome.status).toBe('error');
    expect(outcome.error).toContain('Invalid JSON');
  });

  test('valid JSON that is not an agent → skipped-not-agent', () => {
    const path = join(dir, 'mcp.json');
    writeFileSync(path, JSON.stringify({ mcpServers: {} }));
    const outcome = upgradeAgentFile(path);
    expect(outcome.status).toBe('skipped-not-agent');
  });
});

describe('default directory helpers', () => {
  test('defaultAgentDirs: workspace-local first, then user-global', () => {
    const dirs = defaultAgentDirs('/tmp/ws');
    expect(dirs).toHaveLength(2);
    expect(dirs[0]).toBe(join('/tmp/ws', '.kiro', 'agents'));
    expect(dirs[1]!.endsWith(join('.kiro', 'agents'))).toBe(true);
  });

  test('defaultScanDirs: local + global scopes derived from the agent dirs', () => {
    const dirs = defaultScanDirs('/tmp/ws');
    expect(dirs.map((d) => d.scope)).toEqual(['local', 'global']);
    expect(dirs[0]!.dir).toBe(join('/tmp/ws', '.kiro', 'agents'));
  });
});
