import { describe, test, expect, beforeEach, afterEach, mock } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

describe('buildKasSettings', () => {
  let tmpDir: string;
  let originalKiroHome: string | undefined;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'kas-settings-test-'));
    originalKiroHome = process.env.KIRO_HOME;
    process.env.KIRO_HOME = tmpDir;
  });

  afterEach(() => {
    if (originalKiroHome === undefined) {
      delete process.env.KIRO_HOME;
    } else {
      process.env.KIRO_HOME = originalKiroHome;
    }
    rmSync(tmpDir, { recursive: true, force: true });
  });

  function writeSettings(settings: Record<string, unknown>) {
    const dir = join(tmpDir, 'settings');
    const { mkdirSync } = require('fs');
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'cli.json'), JSON.stringify(settings));
  }

  // Re-import each test to pick up fresh env
  async function getBuildKasSettings() {
    // Clear module cache to pick up new KIRO_HOME
    delete require.cache[require.resolve('./kas-settings.js')];
    delete require.cache[require.resolve('./cli-settings.js')];
    const mod = await import('./kas-settings.js');
    return mod.buildKasSettings;
  }

  test('returns undefined when no settings file exists', async () => {
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toBeUndefined();
  });

  test('returns undefined when settings file is empty object', async () => {
    writeSettings({});
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toBeUndefined();
  });

  test('maps boolean flag to { enabled: true }', async () => {
    writeSettings({ 'chat.enableThinking': true });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({ thinking: { enabled: true } });
  });

  test('maps boolean flag to { enabled: false }', async () => {
    writeSettings({ 'chat.enableThinking': false });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({ thinking: { enabled: false } });
  });

  test('ignores non-boolean values for boolean flags', async () => {
    writeSettings({ 'chat.enableThinking': 'yes', 'chat.enableKnowledge': 42 });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toBeUndefined();
  });

  test('maps all boolean flags correctly', async () => {
    writeSettings({
      'chat.enableThinking': true,
      'chat.enableCodeIntelligence': true,
      'chat.enableTodoList': false,
      'chat.enableCheckpoint': true,
      'chat.enableTangentMode': false,
      'chat.disableAutoCompaction': true,
      'chat.enableSubagent': true,
      'chat.enableDelegate': false,
    });
    const buildKasSettings = await getBuildKasSettings();
    const result = buildKasSettings();
    expect(result).toEqual({
      thinking: { enabled: true },
      codeIntelligence: { enabled: true },
      todoList: { enabled: false },
      checkpoint: { enabled: true },
      tangentMode: { enabled: false },
      disableAutoCompaction: { enabled: true },
      _subagent: { enabled: true },
      _delegate: { enabled: false },
    });
  });

  test('maps toolSearch with all fields', async () => {
    writeSettings({
      'toolSearch.enabled': true,
      'toolSearch.minPct': 5,
      'toolSearch.minTokens': 50000,
    });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({
      toolSearch: { enabled: true, minPct: 5, minTokens: 50000 },
    });
  });

  test('maps toolSearch with only enabled', async () => {
    writeSettings({ 'toolSearch.enabled': false });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({
      toolSearch: { enabled: false },
    });
  });

  test('maps compaction settings', async () => {
    writeSettings({
      'compaction.excludeContextWindowPercent': 20,
      'compaction.excludeMessages': 4,
    });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({
      compaction: { enabled: true, excludePercent: 20, excludeMessages: 4 },
    });
  });

  test('maps knowledge with structured config', async () => {
    writeSettings({
      'chat.enableKnowledge': true,
      'knowledge.maxFiles': 1000,
      'knowledge.indexType': 'fast',
    });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({
      knowledge: { enabled: true, maxFiles: 1000, indexType: 'fast' },
    });
  });

  test('knowledge structured overrides simple boolean mapping', async () => {
    writeSettings({ 'chat.enableKnowledge': false });
    const buildKasSettings = await getBuildKasSettings();
    // knowledge structured config overrides the simple boolean entry
    expect(buildKasSettings()).toEqual({
      knowledge: { enabled: false },
    });
  });

  test('ignores unrelated settings', async () => {
    writeSettings({
      'chat.greeting.enabled': false,
      'chat.disableWrap': true,
      'telemetry.enabled': true,
      'chat.enableThinking': true,
    });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({ thinking: { enabled: true } });
  });
});
