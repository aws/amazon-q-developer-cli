import { describe, it, expect } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';

const ROOT = path.join(__dirname, '..');

describe('Subagent Visibility', () => {
  it('acp-client has SUBAGENT_LIST_UPDATE handler', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/acp-client.ts'), 'utf8');
    expect(src).toContain('SUBAGENT_LIST_UPDATE');
    expect(src).toContain('subagentListHandlers');
  });

  it('acp-client has spawnSession method', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/acp-client.ts'), 'utf8');
    expect(src).toContain('spawnSession');
    expect(src).toContain('SESSION_SPAWN');
  });

  it('kiro.ts has spawnSession and onSubagentListUpdate', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/kiro.ts'), 'utf8');
    expect(src).toContain('spawnSession');
    expect(src).toContain('onSubagentListUpdate');
  });

  it('index.tsx wires onSubagentListUpdate to store', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/index.tsx'), 'utf8');
    expect(src).toContain('onSubagentListUpdate');
  });

  it('store has sessionEventBuffer', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/stores/app-store.ts'), 'utf8');
    expect(src).toContain('sessionEventBuffer');
    expect(src).toContain('pushSessionEvent');
  });

  it('/spawn command is registered', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/commands/effects.ts'), 'utf8');
    expect(src).toContain('spawn');
    expect(src).toContain('spawnSession');
  });

  it('CommandContext has sessions field', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/commands/types.ts'), 'utf8');
    expect(src).toContain('sessions: Map');
  });
});