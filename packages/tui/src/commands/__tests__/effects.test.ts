import {
  describe,
  it,
  expect,
  mock,
  beforeEach,
  afterEach,
  afterAll,
  spyOn,
} from 'bun:test';

// --- Module mocks MUST be declared before importing the module under test ---
const mockSpawnSync = mock(() => ({ status: 1 }));

mock.module('child_process', () => ({ spawnSync: mockSpawnSync }));

import * as fs from 'fs';
const mockWriteFileSync = spyOn(fs, 'writeFileSync').mockImplementation(
  () => {}
);

afterAll(() => {
  mockWriteFileSync.mockRestore();
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

function createMockCtx(
  messages: Array<{ id: string; role: string; content: string }> = []
) {
  return createMockCommandContext({ messages, slashCommands: [copyCmd] });
}

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
    const ctx = createMockCtx([modelMessage(text)]);

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
    const ctx = createMockCtx([modelMessage(text)]);

    runEffect(copyCmd, null, ctx, '');

    const expected = `\x1b]52;c;${Buffer.from(text, 'utf-8').toString('base64')}\x07`;
    const wfCalls = mockWriteFileSync.mock.calls as unknown as unknown[][];
    expect(wfCalls[0]![1]).toBe(expected);
  });

  it('skips OSC 52 for payloads > 100KB', () => {
    const bigText = 'x'.repeat(100_001);
    const ctx = createMockCtx([modelMessage(bigText)]);

    runEffect(copyCmd, null, ctx, '');

    expect(mockWriteFileSync).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Failed');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
  });

  it('handles OSC 52 write failure gracefully', () => {
    const ctx = createMockCtx([modelMessage('test')]);
    mockWriteFileSync.mockImplementation(() => {
      throw new Error('write failed');
    });

    runEffect(copyCmd, null, ctx, '');

    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Failed');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
  });

  it('skips OSC 52 when platform tool succeeds', () => {
    mockSpawnSync.mockImplementation(() => ({ status: 0 }));
    const ctx = createMockCtx([modelMessage('test')]);

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

  it('/clear with new sessionId in result resets UI and adopts session (KAS path)', () => {
    const cmd: SlashCommand = {
      name: '/clear',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: 'Conversation cleared',
      data: {
        sessionId: 'new-session-id',
        currentModel: { id: 'm1', name: 'Test Model' },
        currentAgent: { name: 'default' },
      },
    };

    runEffect(cmd, result, ctx, '');

    // KAS path — full wipe and adopt the new session
    expect(ctx._spies.clearUIState!).toHaveBeenCalled();
    expect(ctx._spies.resetMessages!).toHaveBeenCalled();
    expect(ctx._spies.setSessionId!).toHaveBeenCalledWith('new-session-id');
    expect(ctx._spies.setCurrentModel!).toHaveBeenCalledWith({
      id: 'm1',
      name: 'Test Model',
    });
    expect(ctx._spies.setCurrentAgent!).toHaveBeenCalledWith({
      name: 'default',
    });
    // Legacy Rust-mode behavior (keep-last-turn) must NOT fire
    expect(ctx._spies.clearMessages!).not.toHaveBeenCalled();
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

  it('/changelog calls setShowChangelogPanel', () => {
    const cmd: SlashCommand = {
      name: '/changelog',
      description: '',
      source: 'local',
      meta: { local: true, inputType: 'panel' },
    };
    const ctx = createMockCommandContext();

    runEffect(cmd, null, ctx, '');

    expect(ctx._spies.setShowChangelogPanel!).toHaveBeenCalledWith(true);
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

describe('/reply effect', () => {
  const replyCmd: SlashCommand = {
    name: '/reply',
    description: '',
    source: 'backend',
  };

  it('shows error alert when no assistant message exists (KAS mode)', () => {
    const ctx = createMockCommandContext({ messages: [] });
    const result = { success: true, message: '', data: {} };

    runEffect(replyCmd, result, ctx, '');

    expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
      'No assistant message found',
      'error',
      3000
    );
  });

  it('shows error alert when result is not success', () => {
    const ctx = createMockCommandContext({ messages: [] });
    const result = { success: false, message: 'Something went wrong' };

    runEffect(replyCmd, result, ctx, '');

    expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
      'Something went wrong',
      'error',
      3000
    );
  });

  it('shows error when only user messages exist', () => {
    const ctx = createMockCommandContext({
      messages: [{ id: '1', role: MessageRole.User, content: 'hello' }],
    });
    const result = { success: true, message: '', data: {} };

    runEffect(replyCmd, result, ctx, '');

    expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
      'No assistant message found',
      'error',
      3000
    );
  });

  it('opens editor with quoted assistant message (KAS mode fallback)', () => {
    mockWriteFileSync.mockReset();
    const ctx = createMockCommandContext({
      messages: [
        { id: '1', role: MessageRole.User, content: 'hello' },
        {
          id: '2',
          role: MessageRole.Model,
          content: 'Hi there!\nHow can I help?',
        },
      ],
    });
    const result = { success: true, message: '', data: {} };

    runEffect(replyCmd, result, ctx, '');

    // openEditorSync writes the quoted content to a temp file before opening $EDITOR
    expect(mockWriteFileSync).toHaveBeenCalled();
    const writeCalls = mockWriteFileSync.mock.calls as unknown as [
      string,
      string,
    ][];
    const contentWritten = writeCalls.find(([path]) =>
      path.includes('kiro-reply')
    );
    expect(contentWritten).toBeDefined();
    expect(contentWritten![1]).toBe('> Hi there!\n> How can I help?\n\n');
  });
});

// --- Additional coverage for uncovered effects ---

const chatCmd: SlashCommand = {
  name: '/chat',
  description: '',
  source: 'backend' as const,
  meta: {},
};

const statsCmd: SlashCommand = {
  name: '/stats',
  description: '',
  source: 'backend' as const,
  meta: {},
};

const hooksCmd: SlashCommand = {
  name: '/hooks',
  description: '',
  source: 'backend' as const,
  meta: {},
};

const knowledgeCmd: SlashCommand = {
  name: '/knowledge',
  description: '',
  source: 'backend' as const,
  meta: {},
};

const clearCmd: SlashCommand = {
  name: '/clear',
  description: '',
  source: 'backend' as const,
  meta: {},
};

const helpCmd: SlashCommand = {
  name: '/help',
  description: '',
  source: 'local' as const,
  meta: { local: true },
};

const sessionIdCmd: SlashCommand = {
  name: '/session-id',
  description: '',
  source: 'local' as const,
  meta: { local: true },
};

const toolsCmd: SlashCommand = {
  name: '/tools',
  description: '',
  source: 'backend' as const,
  meta: {},
};

describe('showStatsPanel effect', () => {
  it('shows stats panel with data', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: { stats: [{ name: 'turns', value: 5 }], summary: { total: 5 } },
    };
    runEffect(statsCmd, result, ctx, '');
    expect(ctx._spies.setShowStatsPanel).toHaveBeenCalledWith(
      true,
      [{ name: 'turns', value: 5 }],
      { total: 5 }
    );
  });

  it('shows stats panel with empty data', () => {
    const ctx = createMockCommandContext();
    const result = { success: true, message: '', data: {} };
    runEffect(statsCmd, result, ctx, '');
    expect(ctx._spies.setShowStatsPanel).toHaveBeenCalledWith(true, [], null);
  });

  it('handles /stats save subcommand', () => {
    const ctx = createMockCommandContext();
    const result = { success: true, message: 'Saved!', data: {} };
    runEffect(statsCmd, result, ctx, 'save /tmp/stats.json');
    expect(ctx._spies.setActiveCommand).toHaveBeenCalledWith(null);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Saved!',
      'success',
      3000
    );
  });
});

describe('showHooksPanel effect', () => {
  it('shows hooks panel with data', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: { hooks: [{ name: 'pre-tool' }] },
    };
    runEffect(hooksCmd, result, ctx, '');
    expect(ctx._spies.setShowHooksPanel).toHaveBeenCalledWith(true, [
      { name: 'pre-tool' },
    ]);
  });

  it('shows hooks panel with empty hooks', () => {
    const ctx = createMockCommandContext();
    const result = { success: true, message: '', data: {} };
    runEffect(hooksCmd, result, ctx, '');
    expect(ctx._spies.setShowHooksPanel).toHaveBeenCalledWith(true, []);
  });
});

describe('showKnowledgePanel effect', () => {
  it('shows knowledge panel when entries exist', () => {
    const ctx = createMockCommandContext();
    const entries = [{ name: 'doc.md', tokens: 100 }];
    const result = {
      success: true,
      message: '',
      data: { entries, status: 'indexed' },
    };
    runEffect(knowledgeCmd, result, ctx, '');
    expect(ctx._spies.setShowKnowledgePanel).toHaveBeenCalledWith(
      true,
      entries,
      'indexed'
    );
  });

  it('hides panel and shows alert when no entries', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: 'No knowledge base configured',
      data: {},
    };
    runEffect(knowledgeCmd, result, ctx, '');
    expect(ctx._spies.setShowKnowledgePanel).toHaveBeenCalledWith(false);
    expect(ctx._spies.showAlert).toHaveBeenCalled();
  });
});

describe('clearMessages effect', () => {
  it('does full reset when result has sessionId (KAS mode)', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: {
        sessionId: 'new-sess-1',
        currentModel: { id: 'claude', name: 'Claude' },
        currentAgent: { name: 'default' },
      },
    };
    runEffect(clearCmd, result, ctx, '');
    expect(ctx._spies.clearUIState).toHaveBeenCalled();
    expect(ctx._spies.resetMessages).toHaveBeenCalled();
    expect(ctx._spies.setSessionId).toHaveBeenCalledWith('new-sess-1');
    expect(ctx._spies.setCurrentModel).toHaveBeenCalledWith({
      id: 'claude',
      name: 'Claude',
    });
    expect(ctx._spies.setCurrentAgent).toHaveBeenCalledWith({
      name: 'default',
    });
  });

  it('calls clearMessages when no sessionId (Rust mode)', () => {
    const ctx = createMockCommandContext();
    const result = { success: true, message: '', data: {} };
    runEffect(clearCmd, result, ctx, '');
    expect(ctx._spies.clearMessages).toHaveBeenCalled();
  });
});

describe('showSessionId effect', () => {
  it('shows session ID when available', () => {
    const ctx = createMockCommandContext({
      kiro: { sessionId: 'abc-123' } as any,
    });
    runEffect(sessionIdCmd, { success: true, message: '', data: {} }, ctx, '');
    expect(ctx._spies.showAlert).toHaveBeenCalled();
    const alertMsg = (ctx._spies.showAlert!.mock.calls[0] as any[])[0];
    expect(alertMsg).toContain('abc-123');
  });
});

describe('showToolsPanel effect', () => {
  it('shows tools panel when tools data present', () => {
    const ctx = createMockCommandContext();
    const tools = [{ name: 'shell', description: 'run commands' }];
    runEffect(
      toolsCmd,
      { success: true, message: '', data: { tools } },
      ctx,
      ''
    );
    expect(ctx._spies.setShowToolsPanel).toHaveBeenCalledWith(true, tools);
  });

  it('does nothing when no tools data (subcommand result)', () => {
    const ctx = createMockCommandContext();
    runEffect(
      toolsCmd,
      { success: true, message: 'Trust all enabled', data: {} },
      ctx,
      ''
    );
    expect(ctx._spies.setShowToolsPanel).not.toHaveBeenCalled();
  });
});

describe('newSession effect', () => {
  it('calls newSession and sets session on success', async () => {
    const mockNewSession = mock(() =>
      Promise.resolve({
        sessionId: 'new-1',
        currentModel: { id: 'claude', name: 'Claude' },
        currentAgent: { name: 'kiro' },
      })
    );
    const ctx = createMockCommandContext({
      kiro: { newSession: mockNewSession } as any,
    });
    runEffect(chatCmd, { success: true, message: '', data: {} }, ctx, 'new');
    await new Promise((r) => setTimeout(r, 10));
    expect(ctx._spies.clearUIState).toHaveBeenCalled();
    expect(ctx._spies.setSessionId).toHaveBeenCalledWith('new-1');
    expect(ctx._spies.setCurrentModel).toHaveBeenCalledWith({
      id: 'claude',
      name: 'Claude',
    });
    expect(ctx._spies.setCurrentAgent).toHaveBeenCalledWith({ name: 'kiro' });
  });

  it('sends prompt after new session when args contain text', async () => {
    const mockNewSession = mock(() => Promise.resolve({ sessionId: 'new-2' }));
    const ctx = createMockCommandContext({
      kiro: { newSession: mockNewSession } as any,
    });
    runEffect(
      chatCmd,
      { success: true, message: '', data: {} },
      ctx,
      'new hello world'
    );
    await new Promise((r) => setTimeout(r, 10));
    expect(ctx._spies.sendMessage).toHaveBeenCalledWith('hello world');
  });

  it('shows error alert on newSession failure', async () => {
    const mockNewSession = mock(() => Promise.reject(new Error('auth failed')));
    const ctx = createMockCommandContext({
      kiro: { newSession: mockNewSession } as any,
    });
    runEffect(chatCmd, { success: true, message: '', data: {} }, ctx, 'new');
    await new Promise((r) => setTimeout(r, 10));
    expect(ctx._spies.setLoadingMessage).toHaveBeenCalledWith(null);
    expect(ctx._spies.showAlert).toHaveBeenCalled();
  });
});

describe('loadSession effect', () => {
  it('handles /chat save subcommand', () => {
    const ctx = createMockCommandContext();
    const result = { success: true, message: 'Session saved', data: {} };
    runEffect(chatCmd, result, ctx, 'save');
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Session saved',
      'success',
      5000
    );
  });

  it('handles /chat load with sessionId in result', () => {
    const mockLoadSession = mock(() =>
      Promise.resolve({ sessionId: 'imported-1' })
    );
    const ctx = createMockCommandContext({
      kiro: {
        loadSession: mockLoadSession,
        onUpdate: mock(() => () => {}),
      } as any,
    });
    const result = {
      success: true,
      message: '',
      data: { sessionId: 'imported-1' },
    };
    runEffect(chatCmd, result, ctx, 'load /tmp/session.json');
    // loadSession triggers async flow — just verify it started
    expect(ctx._spies.clearUIState).toHaveBeenCalled();
    expect(ctx._spies.setLoadingMessage).toHaveBeenCalled();
  });

  it('shows error for failed /chat load', () => {
    const ctx = createMockCommandContext();
    const result = { success: false, message: 'File not found', data: {} };
    runEffect(chatCmd, result, ctx, 'load /tmp/missing.json');
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'File not found',
      'error',
      5000
    );
  });

  it('handles switchSession flag from /rewind', () => {
    const mockLoadSession = mock(() =>
      Promise.resolve({ sessionId: 'forked-1' })
    );
    const ctx = createMockCommandContext({
      kiro: {
        loadSession: mockLoadSession,
        onUpdate: mock(() => () => {}),
      } as any,
    });
    const result = {
      success: true,
      message: '',
      data: {
        switchSession: true,
        sessionId: 'forked-1',
        resetMessagesBeforeReplay: true,
      },
    };
    runEffect(chatCmd, result, ctx, '');
    expect(ctx._spies.resetMessages).toHaveBeenCalled();
    expect(ctx._spies.setLoadingMessage).toHaveBeenCalled();
  });

  it('shows error when switchSession fails', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: false,
      message: 'Rewind failed',
      data: { switchSession: true, sessionId: 'x' },
    };
    runEffect(chatCmd, result, ctx, '');
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Rewind failed',
      'error',
      5000
    );
  });
});

describe('showHelpPanel effect', () => {
  it('shows help panel when commands data present', () => {
    const ctx = createMockCommandContext({
      slashCommands: [helpCmd],
    });
    const result = {
      success: true,
      message: '',
      data: {
        commands: [
          {
            name: 'clear',
            description: 'Clear chat',
            usage: '/clear',
            subcommands: [],
          },
        ],
      },
    };
    runEffect(helpCmd, result, ctx, '');
    expect(ctx._spies.setShowHelpPanel).toHaveBeenCalled();
  });
});

describe('/spawn effect', () => {
  const spawnCmd: SlashCommand = {
    name: '/spawn',
    description: 'Spawn session',
    source: 'local' as const,
    meta: { local: true },
  };

  it('shows error when no args', async () => {
    const ctx = createMockCommandContext();
    await runEffect(spawnCmd, { success: true, message: '' }, ctx, '');
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Task description is required',
      'error',
      3000
    );
  });

  it('spawns session with task', async () => {
    const ctx = createMockCommandContext({
      kiro: {
        spawnSession: mock(() =>
          Promise.resolve({ sessionId: 'new-s', name: 'my-task' })
        ),
      } as any,
    });
    await runEffect(
      spawnCmd,
      { success: true, message: '' },
      ctx,
      'do something'
    );
    expect(ctx._spies.addSession).toHaveBeenCalled();
    expect(ctx._spies.showAlert).toHaveBeenCalled();
  });

  it('parses --name flag', async () => {
    const spawnMock = mock(() =>
      Promise.resolve({ sessionId: 'ns', name: 'custom' })
    );
    const ctx = createMockCommandContext({
      kiro: { spawnSession: spawnMock } as any,
    });
    await runEffect(
      spawnCmd,
      { success: true, message: '' },
      ctx,
      '--name custom do work'
    );
    expect(spawnMock).toHaveBeenCalledWith('do work', 'custom');
  });
});

describe('/rewind effect', () => {
  const rewindCmd: SlashCommand = {
    name: '/rewind',
    description: 'Rewind',
    source: 'local' as const,
    meta: { local: true },
  };

  it('shows explorer when turns are returned', () => {
    const ctx = createMockCommandContext();
    const turns = [
      { logIndex: 0, label: 'Turn 1', group: 'g1', responseSnippet: 'hi' },
    ];
    runEffect(
      rewindCmd,
      { success: true, message: '', data: { turns } },
      ctx,
      ''
    );
    expect(ctx._spies.setShowRewindExplorer).toHaveBeenCalledWith(true, turns);
  });

  it('shows alert when no turns', () => {
    const ctx = createMockCommandContext();
    runEffect(
      rewindCmd,
      { success: true, message: '', data: { turns: [] } },
      ctx,
      ''
    );
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'No previous turns to rewind to',
      'warning',
      3000
    );
  });
});

describe('switchToGuideAgent effect', () => {
  const guideCmd: SlashCommand = {
    name: '/guide',
    description: 'Guide',
    source: 'backend' as const,
  };

  it('sets agent and sends prompt', () => {
    const ctx = createMockCommandContext();
    runEffect(
      guideCmd,
      {
        success: true,
        message: '',
        data: { agent: { name: 'guide' }, prompt: 'help me' },
      },
      ctx,
      ''
    );
    expect(ctx._spies.setCurrentAgent).toHaveBeenCalledWith({
      name: 'guide',
    });
    expect(ctx._spies.sendMessage).toHaveBeenCalledWith('help me');
  });
});

describe('loadSession effect', () => {
  const chatCmd: SlashCommand = {
    name: '/chat',
    description: 'Load session',
    source: 'local' as const,
    meta: { local: true },
  };

  it('shows error when load command fails', () => {
    const ctx = createMockCommandContext();
    runEffect(
      chatCmd,
      { success: false, message: 'Import failed' },
      ctx,
      'load /tmp/file.json'
    );
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Import failed',
      'error',
      5000
    );
  });
});
