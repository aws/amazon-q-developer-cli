import {
  describe,
  it,
  expect,
  mock,
  beforeEach,
  afterEach,
  afterAll,
} from 'bun:test';

// --- Module mocks MUST be declared before importing the module under test ---
const mockSpawnSync = mock(() => ({ status: 1 }));
const mockWriteFileSync = mock((_path: string, _data: string) => {});

mock.module('child_process', () => ({ spawnSync: mockSpawnSync }));
mock.module('fs', () => ({
  writeFileSync: mockWriteFileSync,
  readFileSync: () => '',
}));

afterAll(() => {
  mock.restore();
});

import { runEffect } from '../effects.js';
import { MessageRole } from '../../stores/app-store.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';

const copyCmd: SlashCommand = {
  name: '/copy',
  description: '',
  source: 'local' as const,
  meta: { local: true },
};

function modelMessage(content: string) {
  return { id: '1', role: MessageRole.Model, content };
}

describe('/copy OSC 52 clipboard fallback', () => {
  let originalPlatform: string;

  beforeEach(() => {
    originalPlatform = process.platform;
    Object.defineProperty(process, 'platform', {
      value: 'linux',
      configurable: true,
    });
    mockSpawnSync.mockReset();
    mockWriteFileSync.mockReset();
    // Default: platform tools fail
    mockSpawnSync.mockImplementation(() => ({ status: 1 }));
  });

  afterEach(() => {
    Object.defineProperty(process, 'platform', {
      value: originalPlatform,
      configurable: true,
    });
  });

  it('falls back to OSC 52 when platform tools fail', () => {
    const text = 'hello clipboard';
    const ctx = createMockCommandContext({
      messages: [modelMessage(text)],
      slashCommands: [copyCmd],
    });

    runEffect(copyCmd, null, ctx, '');

    expect(mockWriteFileSync).toHaveBeenCalledTimes(1);
    const calls = mockWriteFileSync.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toBe('/dev/tty');
    const b64 = Buffer.from(text, 'utf-8').toString('base64');
    expect(calls[0]![1]).toBe(`\x1b]52;c;${b64}\x07`);
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Copied');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('success');
  });

  it('writes correct base64 encoding in OSC 52 sequence', () => {
    const text = 'Unicode: 日本語 🎉';
    const ctx = createMockCommandContext({
      messages: [modelMessage(text)],
      slashCommands: [copyCmd],
    });

    runEffect(copyCmd, null, ctx, '');

    const expected = `\x1b]52;c;${Buffer.from(text, 'utf-8').toString('base64')}\x07`;
    const wfCalls = mockWriteFileSync.mock.calls as unknown as unknown[][];
    expect(wfCalls[0]![1]).toBe(expected);
  });

  it('skips OSC 52 for payloads > 100KB', () => {
    const bigText = 'x'.repeat(100_001);
    const ctx = createMockCommandContext({
      messages: [modelMessage(bigText)],
      slashCommands: [copyCmd],
    });

    runEffect(copyCmd, null, ctx, '');

    expect(mockWriteFileSync).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Failed');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
  });

  it('handles OSC 52 write failure gracefully', () => {
    const ctx = createMockCommandContext({
      messages: [modelMessage('test')],
      slashCommands: [copyCmd],
    });
    mockWriteFileSync.mockImplementation(() => {
      throw new Error('write failed');
    });

    runEffect(copyCmd, null, ctx, '');

    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Failed');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
  });

  it('skips OSC 52 when platform tool succeeds', () => {
    mockSpawnSync.mockImplementation(() => ({ status: 0 }));
    const ctx = createMockCommandContext({
      messages: [modelMessage('test')],
      slashCommands: [copyCmd],
    });

    runEffect(copyCmd, null, ctx, '');

    expect(mockWriteFileSync).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Copied');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('success');
  });
});

describe('copyToSystemClipboard platform behavior', () => {
  let originalPlatform: string;

  beforeEach(() => {
    originalPlatform = process.platform;
    mockSpawnSync.mockReset();
    mockWriteFileSync.mockReset();
  });

  afterEach(() => {
    Object.defineProperty(process, 'platform', {
      value: originalPlatform,
      configurable: true,
    });
  });

  it('on darwin uses pbcopy', () => {
    Object.defineProperty(process, 'platform', {
      value: 'darwin',
      configurable: true,
    });
    mockSpawnSync.mockImplementation(() => ({ status: 0 }));

    const ctx = createMockCommandContext({
      messages: [modelMessage('test')],
      slashCommands: [copyCmd],
    });
    runEffect(copyCmd, null, ctx, '');

    // spawnSync should have been called with pbcopy
    const calls = mockSpawnSync.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toBe('pbcopy');
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Copied');
  });

  it('returns false when all tools fail and no /dev/tty (win32)', () => {
    Object.defineProperty(process, 'platform', {
      value: 'win32',
      configurable: true,
    });
    mockSpawnSync.mockImplementation(() => ({ status: 1 }));

    const ctx = createMockCommandContext({
      messages: [modelMessage('test')],
      slashCommands: [copyCmd],
    });
    runEffect(copyCmd, null, ctx, '');

    // On win32, OSC 52 is skipped entirely, so writeFileSync should NOT be called
    expect(mockWriteFileSync).not.toHaveBeenCalled();
    // Should show failure
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Failed');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
  });
});

describe('runEffect routing', () => {
  it('returns false for unknown commands', () => {
    const cmd: SlashCommand = {
      name: '/totally-unknown',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = runEffect(cmd, null, ctx, '');
    expect(result).toBe(false);
  });

  it('/help calls setShowHelpPanel with merged local+backend commands', () => {
    const helpCmd: SlashCommand = {
      name: '/help',
      description: 'Show help',
      source: 'backend',
    };
    const localCmd: SlashCommand = {
      name: '/editor',
      description: 'Open editor',
      source: 'local',
      meta: { local: true },
    };
    const ctx = createMockCommandContext({
      slashCommands: [helpCmd, localCmd],
    });
    const result = {
      success: true,
      message: 'Help',
      data: {
        commands: [{ name: '/help', description: 'Show help', usage: '/help' }],
      },
    };

    runEffect(helpCmd, result, ctx, '');

    expect(ctx._spies.setShowHelpPanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowHelpPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    // Should include both backend and local commands
    expect(call[1].length).toBeGreaterThanOrEqual(2);
  });

  it('/usage calls setShowUsagePanel', () => {
    const cmd: SlashCommand = {
      name: '/usage',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = { success: true, message: '', data: { planName: 'Pro' } };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowUsagePanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowUsagePanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
  });

  it('/mcp calls setShowMcpPanel with servers data', () => {
    const cmd: SlashCommand = {
      name: '/mcp',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const servers = [{ name: 'test-server', status: 'running', toolCount: 3 }];
    const result = {
      success: true,
      message: '',
      data: { servers, mode: 'list' },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowMcpPanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowMcpPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toEqual(servers);
    expect(call[2]).toBe('list');
  });

  it('/tools calls setShowToolsPanel', () => {
    const cmd: SlashCommand = {
      name: '/tools',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const tools = [
      {
        name: 'fs_write',
        source: 'builtin',
        description: 'Write files',
        status: 'allowed',
      },
    ];
    const result = { success: true, message: '', data: { tools } };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowToolsPanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowToolsPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toEqual(tools);
  });

  it('/knowledge with entries shows panel', () => {
    const cmd: SlashCommand = {
      name: '/knowledge',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const entries = [
      {
        name: 'docs',
        id: '1',
        description: 'Documentation',
        item_count: 5,
        path: '/docs',
      },
    ];
    const result = {
      success: true,
      message: '',
      data: { entries, status: 'ready' },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowKnowledgePanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowKnowledgePanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toEqual(entries);
  });

  it('/knowledge without entries hides panel and shows alert', () => {
    const cmd: SlashCommand = {
      name: '/knowledge',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = { success: true, message: 'No knowledge found', data: {} };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowKnowledgePanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowKnowledgePanel!.mock.calls[0]!;
    expect(call[0]).toBe(false);
    expect(ctx._spies.showAlert!).toHaveBeenCalled();
  });

  it('/clear calls clearMessages', () => {
    const cmd: SlashCommand = {
      name: '/clear',
      description: '',
      source: 'local',
      meta: { local: true },
    };
    const ctx = createMockCommandContext();

    runEffect(cmd, null, ctx, '');

    expect(ctx._spies.clearMessages!).toHaveBeenCalled();
  });

  it('/context with breakdown shows panel', () => {
    const cmd: SlashCommand = {
      name: '/context',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: {
        breakdown: { contextFiles: { percent: 50, tokens: 1000 } },
        contextUsagePercentage: 50,
      },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowContextBreakdown!).toHaveBeenCalled();
    expect(ctx._spies.setContextUsage!).toHaveBeenCalledWith(50);
  });

  it('/prompts with executePrompt sends message', () => {
    const cmd: SlashCommand = {
      name: '/prompts',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: { executePrompt: 'run this prompt' },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.sendMessage!).toHaveBeenCalledWith('run this prompt');
  });

  it('/tui calls setShowTuiPanel', () => {
    const cmd: SlashCommand = {
      name: '/tui',
      description: '',
      source: 'local',
      meta: { local: true },
    };
    const ctx = createMockCommandContext();

    runEffect(cmd, null, ctx, '');

    expect(ctx._spies.setShowTuiPanel!).toHaveBeenCalledWith(true);
  });

  it('/code with executePrompt sends message', () => {
    const cmd: SlashCommand = {
      name: '/code',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: { executePrompt: 'code prompt', label: 'code label' },
    };

    const handled = runEffect(cmd, result, ctx, '');

    expect(handled).toBe(true);
    expect(ctx._spies.sendMessage!).toHaveBeenCalled();
  });

  it('/code with data shows panel', () => {
    const cmd: SlashCommand = {
      name: '/code',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: {
        status: 'initialized',
        rootPath: '/project',
        detectedLanguages: ['typescript'],
        projectMarkers: [],
        lsps: [],
        configPath: '/config',
      },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowCodePanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowCodePanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
  });

  it('/feedback with url shows alert', () => {
    const cmd: SlashCommand = {
      name: '/feedback',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: 'Open this URL: https://example.com',
      data: { url: 'https://example.com' },
    };

    const handled = runEffect(cmd, result, ctx, '');

    expect(handled).toBe(true);
    expect(ctx._spies.showAlert!).toHaveBeenCalled();
    const call = ctx._spies.showAlert!.mock.calls[0]!;
    expect(call[0]).toContain('https://example.com');
  });
});

describe('/model effect', () => {
  it('calls setCurrentModel when model data is present', () => {
    const cmd: SlashCommand = {
      name: '/model',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const model = { id: 'claude-4', name: 'Claude 4' };
    const result = { success: true, message: '', data: { model } };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setCurrentModel!).toHaveBeenCalledWith(model);
  });

  it('does not call setCurrentModel when model data is absent', () => {
    const cmd: SlashCommand = {
      name: '/model',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = { success: true, message: 'No model', data: {} };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setCurrentModel!).not.toHaveBeenCalled();
  });
});

describe('/agent effect', () => {
  it('calls setCurrentAgent when agent data (not path) is present', () => {
    const cmd: SlashCommand = {
      name: '/agent',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const agent = { name: 'software-engineer' };
    const result = { success: true, message: '', data: { agent } };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setCurrentAgent!).toHaveBeenCalledWith(agent);
  });
});

describe('/hooks effect', () => {
  it('calls setShowHooksPanel with hooks data', () => {
    const cmd: SlashCommand = {
      name: '/hooks',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const hooks = [{ name: 'pre-commit', event: 'commit', status: 'active' }];
    const result = { success: true, message: '', data: { hooks } };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowHooksPanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowHooksPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toEqual(hooks);
  });

  it('calls setShowHooksPanel with empty array when hooks data is absent', () => {
    const cmd: SlashCommand = {
      name: '/hooks',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = { success: true, message: '', data: {} };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowHooksPanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowHooksPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toEqual([]);
  });
});

describe('/paste effect', () => {
  it('calls sendMessage with formatted label and image when data is present', () => {
    const cmd: SlashCommand = {
      name: '/paste',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: {
        data: 'base64encodeddata',
        mimeType: 'image/png',
        width: 100,
        height: 200,
        sizeBytes: 5000,
      },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.sendMessage!).toHaveBeenCalled();
    const call = ctx._spies.sendMessage!.mock.calls[0]!;
    // First arg is the formatted label string
    expect(typeof call[0]).toBe('string');
    // Second arg is the images array
    expect(call[1]).toEqual([
      { base64: 'base64encodeddata', mimeType: 'image/png' },
    ]);
  });

  it('calls showAlert when paste fails with error message', () => {
    const cmd: SlashCommand = {
      name: '/paste',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: false,
      message: 'No image found in clipboard',
      data: {},
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.showAlert!).toHaveBeenCalled();
    const call = ctx._spies.showAlert!.mock.calls[0]!;
    expect(call[0]).toBe('No image found in clipboard');
    expect(call[1]).toBe('error');
  });
});

describe('/mcp with registryServers', () => {
  it('passes registryServers to setShowMcpPanel', () => {
    const cmd: SlashCommand = {
      name: '/mcp',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const servers = [{ name: 'local-server', status: 'running', toolCount: 2 }];
    const registryServers = [
      { name: 'registry-server', status: 'available', toolCount: 5 },
    ];
    const result = {
      success: true,
      message: '',
      data: { servers, mode: 'list', registryServers },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowMcpPanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowMcpPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toEqual(servers);
    expect(call[2]).toBe('list');
    expect(call[3]).toEqual(registryServers);
  });
});

describe('/code without data (close panel)', () => {
  it('calls setShowCodePanel(false) and showAlert when result has message but no data', () => {
    const cmd: SlashCommand = {
      name: '/code',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: 'Code panel closed',
      data: undefined as unknown,
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowCodePanel!).toHaveBeenCalled();
    const call = ctx._spies.setShowCodePanel!.mock.calls[0]!;
    expect(call[0]).toBe(false);
    expect(ctx._spies.showAlert!).toHaveBeenCalled();
    const alertCall = ctx._spies.showAlert!.mock.calls[0]!;
    expect(alertCall[0]).toBe('Code panel closed');
    expect(alertCall[1]).toBe('success');
  });
});

describe('/tools subcommand (no tools data)', () => {
  it('does not call setShowToolsPanel when tools data is absent', () => {
    const cmd: SlashCommand = {
      name: '/tools',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: 'All tools trusted',
      data: {},
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setShowToolsPanel!).not.toHaveBeenCalled();
  });
});
