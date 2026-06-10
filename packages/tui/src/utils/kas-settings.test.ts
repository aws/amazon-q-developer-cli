import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync, mkdirSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

/** Default settings that buildKasSettings always includes (tools that were always-on for CLI). */
const CLI_DEFAULTS = {
  codeIntelligence: { enabled: true },
  knowledge: { enabled: true },
  thinking: { enabled: true },
};

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
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, 'cli.json'), JSON.stringify(settings));
  }

  async function getBuildKasSettings() {
    delete require.cache[require.resolve('./kas-settings.js')];
    delete require.cache[require.resolve('./cli-settings.js')];
    const mod = await import('./kas-settings.js');
    return mod.buildKasSettings;
  }

  test('returns defaults when no settings file exists', async () => {
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual(CLI_DEFAULTS);
  });

  test('returns defaults when settings file is empty object', async () => {
    writeSettings({});
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual(CLI_DEFAULTS);
  });

  test('maps boolean flag to { enabled: true }', async () => {
    writeSettings({ 'chat.enableThinking': true });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({
      ...CLI_DEFAULTS,
      thinking: { enabled: true },
    });
  });

  test('maps boolean flag to { enabled: false }', async () => {
    writeSettings({ 'chat.enableThinking': false });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual({
      ...CLI_DEFAULTS,
      thinking: { enabled: false },
    });
  });

  test('ignores non-boolean values for boolean flags', async () => {
    writeSettings({ 'chat.enableThinking': 'yes', 'chat.enableKnowledge': 42 });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()).toEqual(CLI_DEFAULTS);
  });

  test('explicit false overrides CLI defaults', async () => {
    writeSettings({ 'chat.enableCodeIntelligence': false });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()?.codeIntelligence).toEqual({ enabled: false });
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
    expect(result?.thinking).toEqual({ enabled: true });
    expect(result?.codeIntelligence).toEqual({ enabled: true });
    expect(result?.todoList).toEqual({ enabled: false });
    expect(result?.checkpoint).toEqual({ enabled: true });
    expect(result?.tangentMode).toEqual({ enabled: false });
    expect(result?.disableAutoCompaction).toEqual({ enabled: true });
    expect(result?._subagent).toEqual({ enabled: true });
    expect(result?._delegate).toEqual({ enabled: false });
  });

  test('maps toolSearch with all fields', async () => {
    writeSettings({
      'toolSearch.enabled': true,
      'toolSearch.minPct': 5,
      'toolSearch.minTokens': 50000,
    });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()?.toolSearch).toEqual({
      enabled: true,
      minPct: 5,
      minTokens: 50000,
    });
  });

  test('maps toolSearch with only enabled=false', async () => {
    writeSettings({ 'toolSearch.enabled': false });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()?.toolSearch).toEqual({ enabled: false });
  });

  test('maps compaction settings', async () => {
    writeSettings({
      'compaction.excludeContextWindowPercent': 20,
      'compaction.excludeMessages': 4,
    });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()?.compaction).toEqual({
      enabled: true,
      excludePercent: 20,
      excludeMessages: 4,
    });
  });

  test('maps knowledge with structured config', async () => {
    writeSettings({
      'chat.enableKnowledge': true,
      'knowledge.maxFiles': 1000,
      'knowledge.indexType': 'fast',
    });
    const buildKasSettings = await getBuildKasSettings();
    expect(buildKasSettings()?.knowledge).toEqual({
      enabled: true,
      maxFiles: 1000,
      indexType: 'fast',
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
    expect(buildKasSettings()).toEqual({
      ...CLI_DEFAULTS,
      thinking: { enabled: true },
    });
  });
});
