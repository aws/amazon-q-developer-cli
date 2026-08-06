import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { persistUiModeDefault } from '../ui-mode-default.js';

let testDir: string;
let originalHome: string | undefined;
let originalKiroHome: string | undefined;

function cliJsonPath(): string {
  return join(testDir, '.kiro', 'settings', 'cli.json');
}

function writeCliJson(data: Record<string, unknown>): void {
  mkdirSync(join(testDir, '.kiro', 'settings'), { recursive: true });
  writeFileSync(cliJsonPath(), JSON.stringify(data, null, 2), 'utf-8');
}

function readCliJson(): Record<string, unknown> {
  return JSON.parse(readFileSync(cliJsonPath(), 'utf-8'));
}

function makeKiro() {
  return {
    sessionId: 'sess-1',
    setSetting: mock(() => Promise.resolve()),
    sendUiModeDefaultChanged: mock(() => undefined),
  } as any;
}

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `ui-mode-default-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(testDir, { recursive: true });
  originalHome = process.env.HOME;
  originalKiroHome = process.env.KIRO_HOME;
  process.env.HOME = testDir;
  delete process.env.KIRO_HOME;
});

afterEach(() => {
  process.env.HOME = originalHome;
  if (originalKiroHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalKiroHome;
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    /* ignore */
  }
});

describe('persistUiModeDefault', () => {
  it('writes cli.json, mirrors to ACP, and emits telemetry on a real change', () => {
    writeCliJson({ 'chat.ui.mode': 'tui' });
    const kiro = makeKiro();

    persistUiModeDefault('lite', kiro);

    expect(readCliJson()['chat.ui.mode']).toBe('lite');
    expect(kiro.setSetting).toHaveBeenCalledWith('chat.ui.mode', 'lite');
    expect(kiro.sendUiModeDefaultChanged).toHaveBeenCalledWith({
      from: 'tui',
      to: 'lite',
      sessionId: 'sess-1',
    });
  });

  it('treats a missing prior setting as "unset" for telemetry', () => {
    const kiro = makeKiro();

    persistUiModeDefault('lite', kiro);

    expect(kiro.sendUiModeDefaultChanged).toHaveBeenCalledWith({
      from: 'unset',
      to: 'lite',
      sessionId: 'sess-1',
    });
  });

  it('skips telemetry when the default is unchanged (still persists)', () => {
    writeCliJson({ 'chat.ui.mode': 'lite' });
    const kiro = makeKiro();

    persistUiModeDefault('lite', kiro);

    expect(readCliJson()['chat.ui.mode']).toBe('lite');
    expect(kiro.setSetting).toHaveBeenCalledWith('chat.ui.mode', 'lite');
    expect(kiro.sendUiModeDefaultChanged).not.toHaveBeenCalled();
  });
});
