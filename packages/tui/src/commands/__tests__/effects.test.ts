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

import { runEffect, sendSpecRevision } from '../effects.js';
import {
  noteCloudScrollbackRepaint,
  cancelCloudScrollbackReconcile,
  isCloudScrollbackReconcileArmed,
} from '../cloud-scrollback-reconcile.js';
import { MessageRole } from '../../stores/app-store.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';
import { KAS_DEFAULT_AGENT_ID } from '../../constants/agents.js';
import { ModeChangeSource } from '../../types/generated/chat-cli.js';

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
    (ctx as any).getUiMode = () => 'lite';

    runEffect(copyCmd, null, ctx, '');

    expect(mockWriteFileSync).toHaveBeenCalledTimes(1);
    const calls = mockWriteFileSync.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toBe('/dev/tty');
    const b64 = Buffer.from(text, 'utf-8').toString('base64');
    expect(calls[0]![1]).toBe(`\x1b]52;c;${b64}\x07`);
    // Confirmation goes via announceSystem (not showAlert) so the row
    // lands in lite scrollback. showAlert(..., 'success') is silently
    // dropped in lite — see app-store.ts ~3479. The clipboard is
    // invisible, so this is the only signal the user gets that /copy
    // worked. This test pins that contract.
    expect(ctx._spies.announceSystem).toHaveBeenCalled();
    expect(ctx._spies.announceSystem!.mock.calls[0]![0]).toContain('Copied');
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
    (ctx as any).getUiMode = () => 'lite';

    runEffect(copyCmd, null, ctx, '');

    expect(mockWriteFileSync).not.toHaveBeenCalled();
    // Confirmation goes via announceSystem (not showAlert) — same
    // contract as the OSC 52 path above.
    expect(ctx._spies.announceSystem).toHaveBeenCalled();
    expect(ctx._spies.announceSystem!.mock.calls[0]![0]).toContain('Copied');
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
    (ctx as any).getUiMode = () => 'lite';
    runEffect(copyCmd, null, ctx, '');

    // spawnSync should have been called with pbcopy
    const calls = mockSpawnSync.mock.calls as unknown as unknown[][];
    expect(calls[0]![0]).toBe('pbcopy');
    // Confirmation goes via announceSystem (not showAlert) — same
    // contract as the OSC 52 path above.
    expect(ctx._spies.announceSystem!.mock.calls[0]![0]).toContain('Copied');
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

  it('/help hides backend voice when voice input is unavailable', () => {
    const original = process.env.KIRO_VOICE_SUPPORTED;
    const originalServerUrl = process.env.KIRO_VOICE_SERVER_URL;
    process.env.KIRO_VOICE_SUPPORTED = '0';
    delete process.env.KIRO_VOICE_SERVER_URL;

    try {
      const helpCmd: SlashCommand = {
        name: '/help',
        description: 'Show help',
        source: 'backend',
      };
      const ctx = createMockCommandContext({ slashCommands: [helpCmd] });
      const result = {
        success: true,
        message: 'Help',
        data: {
          commands: [
            { name: '/help', description: 'Show help', usage: '/help' },
            { name: '/voice', description: 'Record voice', usage: '/voice' },
          ],
        },
      };

      runEffect(helpCmd, result, ctx, '');

      const commands = ctx._spies.setShowHelpPanel!.mock.calls[0]![1];
      expect(
        commands.some((command: { name: string }) => command.name === '/voice')
      ).toBe(false);
      expect(
        commands.some((command: { name: string }) => command.name === '/help')
      ).toBe(true);
    } finally {
      if (original === undefined) delete process.env.KIRO_VOICE_SUPPORTED;
      else process.env.KIRO_VOICE_SUPPORTED = original;
      if (originalServerUrl === undefined)
        delete process.env.KIRO_VOICE_SERVER_URL;
      else process.env.KIRO_VOICE_SERVER_URL = originalServerUrl;
    }
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
        currentAgent: { name: KAS_DEFAULT_AGENT_ID },
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
      name: KAS_DEFAULT_AGENT_ID,
    });
    // Legacy Rust-mode behavior (keep-last-turn) must NOT fire
    expect(ctx._spies.clearMessages!).not.toHaveBeenCalled();
  });

  it('/clear in lite mode does NOT wipe the terminal (Rust keep-last-turn path)', () => {
    // Regression: /clear used to write CSI 2J/3J in lite mode, destroying the
    // terminal scrollback buffer (pre-kiro shell history + prior sessions)
    // instead of just clearing conversation context. Lite never emits 2J/3J.
    // Direct stdout replace (not spyOn) — spyOn(process.stdout,'write') does
    // not intercept in this runtime; the codebase uses direct assignment
    // (see utils/__tests__/notification.test.ts, shell-escape.test.ts).
    const cmd: SlashCommand = {
      name: '/clear',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    (ctx as any).getUiMode = () => 'lite';
    const originalWrite = process.stdout.write;
    const wrote: string[] = [];
    process.stdout.write = ((chunk: any) => {
      wrote.push(String(chunk));
      return true;
    }) as any;
    try {
      runEffect(cmd, null, ctx, '');
    } finally {
      process.stdout.write = originalWrite;
    }
    const joined = wrote.join('');
    expect(joined).not.toContain('\x1b[2J');
    expect(joined).not.toContain('\x1b[3J');
    // Keep-last-turn path still fires (conversation context cleared by backend).
    expect(ctx._spies.clearMessages!).toHaveBeenCalled();
  });

  it('/clear in lite mode (KAS new-session path) does NOT wipe the terminal', () => {
    const cmd: SlashCommand = {
      name: '/clear',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    (ctx as any).getUiMode = () => 'lite';
    const result = {
      success: true,
      message: 'Conversation cleared',
      data: { sessionId: 'new-session-id' },
    };
    const originalWrite = process.stdout.write;
    const wrote: string[] = [];
    process.stdout.write = ((chunk: any) => {
      wrote.push(String(chunk));
      return true;
    }) as any;
    try {
      runEffect(cmd, result, ctx, '');
    } finally {
      process.stdout.write = originalWrite;
    }
    const joined = wrote.join('');
    expect(joined).not.toContain('\x1b[2J');
    expect(joined).not.toContain('\x1b[3J');
    // Clean, scrollback-preserving reset path is still used.
    expect(ctx._spies.resetMessages!).toHaveBeenCalled();
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

  it('/tui from TUI mode is a no-op with a notice (disabled in TUI)', () => {
    const cmd: SlashCommand = {
      name: '/tui',
      description: '',
      source: 'local',
      meta: { local: true, liteOnly: true },
    };
    const ctx = createMockCommandContext();
    // createMockCommandContext defaults getUiMode() to 'tui'.

    runEffect(cmd, null, ctx, '');

    expect(ctx._spies.setUiMode!).not.toHaveBeenCalled();
    expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
      'Already in the TUI'
    );
  });

  it('/tui from lite mode switches to TUI and persists the default', () => {
    const cmd: SlashCommand = {
      name: '/tui',
      description: '',
      source: 'local',
      meta: { local: true },
    };
    const ctx = createMockCommandContext();
    (ctx as any).getUiMode = () => 'lite';

    runEffect(cmd, null, ctx, '');

    expect(ctx._spies.setUiMode!).toHaveBeenCalledWith(
      'tui',
      'Switched to TUI mode'
    );
    // Switching also makes TUI the persisted default (cli.json + ACP).
    expect(ctx.kiro.setSetting).toHaveBeenCalledWith('chat.ui.mode', 'tui');
  });

  it('/lite is a no-op when KIRO_LITE_ROLLOUT_ENABLED is unset', () => {
    const prev = process.env.KIRO_LITE_ROLLOUT_ENABLED;
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
    try {
      const cmd: SlashCommand = {
        name: '/lite',
        description: '',
        source: 'local',
        meta: { local: true },
      };
      const ctx = createMockCommandContext();

      runEffect(cmd, null, ctx, '');

      expect(ctx._spies.setUiMode!).not.toHaveBeenCalled();
      expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
        'Lite mode is not available in this build'
      );
    } finally {
      if (prev !== undefined) process.env.KIRO_LITE_ROLLOUT_ENABLED = prev;
    }
  });

  it('/lite switches to lite mode when KIRO_LITE_ROLLOUT_ENABLED=1', () => {
    const prev = process.env.KIRO_LITE_ROLLOUT_ENABLED;
    process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
    try {
      const cmd: SlashCommand = {
        name: '/lite',
        description: '',
        source: 'local',
        meta: { local: true },
      };
      const ctx = createMockCommandContext();

      runEffect(cmd, null, ctx, '');

      expect(ctx._spies.setUiMode!).toHaveBeenCalledWith(
        'lite',
        '[EXPERIMENTAL] Switched to Lite UI'
      );
      // Switching also makes Lite the persisted default (cli.json + ACP).
      expect(ctx.kiro.setSetting).toHaveBeenCalledWith('chat.ui.mode', 'lite');
      expect(ctx._spies.addSystemMessage!).not.toHaveBeenCalled();
      expect(ctx._spies.announceSystem!).not.toHaveBeenCalled();
      expect(ctx._spies.showAlert!).not.toHaveBeenCalled();
    } finally {
      if (prev === undefined) delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
      else process.env.KIRO_LITE_ROLLOUT_ENABLED = prev;
    }
  });

  it('/lite from lite mode is a no-op with a notice (disabled in lite)', () => {
    const prev = process.env.KIRO_LITE_ROLLOUT_ENABLED;
    process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
    try {
      const cmd: SlashCommand = {
        name: '/lite',
        description: '',
        source: 'local',
        meta: { local: true, tuiOnly: true },
      };
      const ctx = createMockCommandContext();
      (ctx as any).getUiMode = () => 'lite';

      runEffect(cmd, null, ctx, '');

      expect(ctx._spies.setUiMode!).not.toHaveBeenCalled();
      expect(ctx.kiro.setSetting).not.toHaveBeenCalled();
      expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
        'Already in the Lite UI'
      );
    } finally {
      if (prev === undefined) delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
      else process.env.KIRO_LITE_ROLLOUT_ENABLED = prev;
    }
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

  it('updates context usage returned by a model switch', () => {
    const cmd: SlashCommand = {
      name: '/model',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: {
        model: { id: 'gpt-5.6-sol', name: 'GPT 5.6' },
        contextUsagePercentage: 7.5,
      },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setContextUsage!).toHaveBeenCalledWith(7.5);
  });

  it('clears stale context usage when model-switch recomputation fails', () => {
    const cmd: SlashCommand = {
      name: '/model',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: {
        model: { id: 'gpt-5.6-sol', name: 'GPT 5.6' },
        contextUsagePercentage: null,
      },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setContextUsage!).toHaveBeenCalledWith(null);
  });

  it('preserves context usage when the selected model is already active', () => {
    const cmd: SlashCommand = {
      name: '/model',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: {
        model: { id: 'gpt-5.6-sol', name: 'GPT 5.6' },
      },
    };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setContextUsage!).not.toHaveBeenCalled();
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

describe('/effort effect', () => {
  it('calls setCurrentEffort when effort data is present', () => {
    const cmd: SlashCommand = {
      name: '/effort',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = { success: true, message: '', data: { effort: 'xhigh' } };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setCurrentEffort!).toHaveBeenCalledWith('xhigh');
  });

  it('does not call setCurrentEffort when effort data is absent', () => {
    const cmd: SlashCommand = {
      name: '/effort',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const result = { success: false, message: 'not available', data: {} };

    runEffect(cmd, result, ctx, '');

    expect(ctx._spies.setCurrentEffort!).not.toHaveBeenCalled();
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

  it('calls sendModeChanged when the new agent name differs from the current one', () => {
    const cmd: SlashCommand = {
      name: '/agent',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext({
      currentAgent: { name: KAS_DEFAULT_AGENT_ID },
    });
    const agent = { name: 'kiro_planner' };
    const result = { success: true, message: '', data: { agent } };

    runEffect(cmd, result, ctx, '');

    expect(ctx.kiro.sendModeChanged).toHaveBeenCalledTimes(1);
    expect(ctx.kiro.sendModeChanged).toHaveBeenCalledWith({
      fromMode: KAS_DEFAULT_AGENT_ID,
      toMode: 'kiro_planner',
      source: ModeChangeSource.SlashCommand,
      sessionId: undefined,
    });
  });

  it('does not call sendModeChanged when the new agent name matches the current one', () => {
    const cmd: SlashCommand = {
      name: '/agent',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext({
      currentAgent: { name: KAS_DEFAULT_AGENT_ID },
    });
    const agent = { name: KAS_DEFAULT_AGENT_ID };
    const result = { success: true, message: '', data: { agent } };

    runEffect(cmd, result, ctx, '');

    expect(ctx.kiro.sendModeChanged).not.toHaveBeenCalled();
  });

  it('does not call sendModeChanged when there is no current agent (initial bootstrap)', () => {
    const cmd: SlashCommand = {
      name: '/agent',
      description: '',
      source: 'backend',
    };
    const ctx = createMockCommandContext();
    const agent = { name: 'kiro_planner' };
    const result = { success: true, message: '', data: { agent } };

    runEffect(cmd, result, ctx, '');

    expect(ctx.kiro.sendModeChanged).not.toHaveBeenCalled();
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

  it('shows alert on paste failure', () => {
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

    expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
      'No image found in clipboard',
      'error'
    );
    expect(ctx._spies.sendMessage!).not.toHaveBeenCalled();
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
        currentAgent: { name: KAS_DEFAULT_AGENT_ID },
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
      name: KAS_DEFAULT_AGENT_ID,
    });
  });

  it('calls clearMessages when no sessionId (Rust mode)', () => {
    const ctx = createMockCommandContext();
    const result = { success: true, message: '', data: {} };
    runEffect(clearCmd, result, ctx, '');
    expect(ctx._spies.clearMessages).toHaveBeenCalled();
  });

  // The repo footer describes the replaced session; a cloud /clear must reset
  // it so it can't be stashed under the new session's id and leak back later.
  it('resets the cloud repo scope on a cloud /clear (no binding reported)', () => {
    const ctx = createMockCommandContext({
      cloudSessionActive: true,
      kiro: {
        isCloudSessionActive: () => true,
        getSessionRepositories: () => null,
        setConfigOption: () => Promise.resolve(),
      } as any,
    });
    const result = {
      success: true,
      message: '',
      data: { sessionId: 'new-sess-2' },
    };
    runEffect(clearCmd, result, ctx, '');
    expect(ctx._spies.resetCloudSessionScope).toHaveBeenCalled();
    expect(ctx._spies.applyRepoFooter).not.toHaveBeenCalled();
  });

  it('re-lights the footer from the create-reported binding on cloud /clear', () => {
    const ctx = createMockCommandContext({
      cloudSessionActive: true,
      kiro: {
        isCloudSessionActive: () => true,
        getSessionRepositories: () => [
          { name: 'KiroCLIReviewerCDK', branch: 'main' },
        ],
        setConfigOption: () => Promise.resolve(),
      } as any,
    });
    const result = {
      success: true,
      message: '',
      data: { sessionId: 'new-sess-3' },
    };
    runEffect(clearCmd, result, ctx, '');
    expect(ctx._spies.resetCloudSessionScope).toHaveBeenCalled();
    expect(ctx._spies.applyRepoFooter).toHaveBeenCalledWith(
      ['KiroCLIReviewerCDK'],
      'main'
    );
  });

  it('does not touch the cloud repo scope on a local KAS /clear', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '',
      data: { sessionId: 'new-sess-4' },
    };
    runEffect(clearCmd, result, ctx, '');
    expect(ctx._spies.resetCloudSessionScope).not.toHaveBeenCalled();
  });

  // Cloud: the sandbox-provisioning checklist repaints for an unbounded time
  // after the initial wipe, so the effect ARMS an event-driven reconcile that
  // wipes once now and re-wipes on each subsequent repaint until the stream
  // quiets — no fixed-timer guess. Local sessions must arm nothing (dark-ship).
  it('arms the event-driven reconcile and wipes once for a cloud /clear', () => {
    try {
      const ctx = createMockCommandContext({
        kiro: { isCloudSessionActive: () => true } as any,
      });
      ctx.cloudSessionActive = true;
      const result = {
        success: true,
        message: '',
        data: { sessionId: 'new-cloud-sess' },
      };
      runEffect(clearCmd, result, ctx, '');
      // Armed → an immediate wipe, and the window is live.
      expect(ctx._spies.bumpLiteScrollbackClear).toHaveBeenCalledTimes(1);
      expect(isCloudScrollbackReconcileArmed()).toBe(true);
      // A later cloud repaint re-wipes so the wipe lands after it — no matter
      // how long the sandbox took to emit it.
      noteCloudScrollbackRepaint();
      expect(ctx._spies.bumpLiteScrollbackClear).toHaveBeenCalledTimes(2);
      noteCloudScrollbackRepaint();
      expect(ctx._spies.bumpLiteScrollbackClear).toHaveBeenCalledTimes(3);
    } finally {
      cancelCloudScrollbackReconcile();
    }
  });

  it('a second /clear supersedes the previous reconcile window (only one live)', () => {
    try {
      const ctx = createMockCommandContext({
        kiro: { isCloudSessionActive: () => true } as any,
      });
      ctx.cloudSessionActive = true;
      const result = {
        success: true,
        message: '',
        data: { sessionId: 'cloud-sess-1' },
      };
      runEffect(clearCmd, result, ctx, '');
      runEffect(
        clearCmd,
        { ...result, data: { sessionId: 'cloud-sess-2' } },
        ctx,
        ''
      );
      // Two arms = two immediate wipes; still exactly one window armed.
      expect(ctx._spies.bumpLiteScrollbackClear).toHaveBeenCalledTimes(2);
      expect(isCloudScrollbackReconcileArmed()).toBe(true);
    } finally {
      cancelCloudScrollbackReconcile();
    }
  });

  it('arms no reconcile for a local /clear', () => {
    try {
      const ctx = createMockCommandContext();
      const result = {
        success: true,
        message: '',
        data: { sessionId: 'new-local-sess' },
      };
      runEffect(clearCmd, result, ctx, '');
      expect(ctx._spies.bumpLiteScrollbackClear).not.toHaveBeenCalled();
      expect(isCloudScrollbackReconcileArmed()).toBe(false);
      // A repaint event with nothing armed is a no-op.
      noteCloudScrollbackRepaint();
      expect(ctx._spies.bumpLiteScrollbackClear).not.toHaveBeenCalled();
    } finally {
      cancelCloudScrollbackReconcile();
    }
  });
});

describe('showSessionId effect', () => {
  // One announceSystem call renders per-surface; both surfaces get the resume
  // hint and the 10s read time (announceSystem's TUI toast honors autoHideMs,
  // lite scrollback ignores it). No uiMode branch in the effect.
  it('announces ID + resume hint with a 10s TUI read time', () => {
    const ctx = createMockCommandContext({
      kiro: { sessionId: 'abc-123' } as any,
    });
    runEffect(sessionIdCmd, { success: true, message: '', data: {} }, ctx, '');
    expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
      'Session ID: abc-123\nResume with: kiro-cli --resume-id abc-123',
      true,
      10000
    );
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it('announces bare "none" when there is no session', () => {
    const ctx = createMockCommandContext({
      kiro: { sessionId: undefined } as any,
    });
    runEffect(sessionIdCmd, { success: true, message: '', data: {} }, ctx, '');
    expect(ctx._spies.announceSystem!).toHaveBeenCalledWith(
      'Session ID: none',
      true,
      10000
    );
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
  // The dedicated `newSession` effect was removed when /chat was
  // intercepted before the dispatcher's effect pipeline. /chat new
  // is now handled by v2-handlers/chat.ts and kas-handlers/chat.ts
  // calling `ctx.kiro.newSession()` directly; coverage lives in
  // those handlers' test files.
  it.skip('removed - see v2-handlers/chat tests', () => {});
});

describe('loadSession effect (rewind switch path)', () => {
  // /chat is owned by v2-handlers/chat.ts and kas-handlers/chat.ts;
  // they call ensure-session + ctx.kiro.loadSession directly. The
  // loadSession effect is only reached via `rewindAction` which
  // hands it a synthetic `{switchSession, sessionId}` payload.
  const rewindCmd: SlashCommand = {
    name: '/rewind',
    description: 'Rewind',
    source: 'local' as const,
    meta: { local: true },
  };

  it('handles switchSession flag from /rewind', async () => {
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
    runEffect(rewindCmd, result, ctx, '');
    await new Promise((resolve) => setTimeout(resolve, 0));
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
    runEffect(rewindCmd, result, ctx, '');
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
    (ctx as any).getUiMode = () => 'lite';
    await runEffect(
      spawnCmd,
      { success: true, message: '' },
      ctx,
      'do something'
    );
    expect(ctx._spies.addSession).toHaveBeenCalled();
    // Confirmation goes via announceSystem (not showAlert) so the row lands
    // in lite scrollback. showAlert(..., 'success') is silently dropped in
    // lite — see app-store.ts ~3479. The contract for state-changing
    // success messages is announceSystem; this test pins that.
    expect(ctx._spies.announceSystem).toHaveBeenCalled();
    const announceArg = ctx._spies.announceSystem!.mock.calls[0]![0];
    expect(announceArg).toContain('Spawned my-task');
    expect(announceArg).toContain('do something');
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

  // Ralph hunt-1 — KAS's spawnSession is a stub returning an empty sessionId
  // (no manual ephemeral-spawn RPC). The effect must NOT build a bogus
  // `session-` name, register a dead id:'' session (which pollutes /switch
  // with a phantom row opening a broken session-view), or announce a
  // misleading "Spawned session-: …". It must show a clean "not supported"
  // error and touch no state.
  it('shows a clean "not supported" error and adds no session when sessionId is empty (KAS stub)', async () => {
    const ctx = createMockCommandContext({
      kiro: {
        // Mirror the KAS stub: empty sessionId.
        spawnSession: mock(() => Promise.resolve({ sessionId: '', name: '' })),
      } as any,
    });
    (ctx as any).getUiMode = () => 'lite';
    await runEffect(
      spawnCmd,
      { success: true, message: '' },
      ctx,
      'do a research task'
    );
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      '/spawn is not supported in KAS mode',
      'error',
      3000
    );
    // No phantom session registered, no misleading confirmation.
    expect(ctx._spies.addSession).not.toHaveBeenCalled();
    expect(ctx._spies.announceSystem).not.toHaveBeenCalled();
  });
});

describe('/switch effect', () => {
  const switchCmd: SlashCommand = {
    name: '/switch',
    description: 'Switch',
    source: 'local' as const,
    meta: { local: true },
  };

  it('returns to main chat via announceSystem (lite-visible)', () => {
    const ctx = createMockCommandContext();
    (ctx as any).getUiMode = () => 'lite';
    // /switch reads from ctx.sessions to validate at least one session
    // exists; seed one so the "No active sessions" error path doesn't
    // fire. Status filter excludes 'pending'.
    ctx.sessions.set('s1', {
      id: 's1',
      name: 'sub-1',
      status: 'running',
    } as any);
    runEffect(switchCmd, { success: true, message: '' }, ctx, 'main');
    expect(ctx._spies.setActiveSession).toHaveBeenCalledWith('');
    // Confirmation goes via announceSystem (not showAlert) so the row
    // lands in lite scrollback. showAlert(..., 'success') is silently
    // dropped in lite — see app-store.ts ~3479. The named-target branch
    // has its own visual feedback (alt-screen swap + setMode); the
    // main-chat branch has no other cue, so without announceSystem the
    // user sees nothing happen. This test pins that contract.
    expect(ctx._spies.announceSystem).toHaveBeenCalled();
    const announceArg = ctx._spies.announceSystem!.mock.calls[0]![0];
    expect(announceArg).toContain('main chat');
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

describe('loadSession effect (legacy /chat dispatch - removed)', () => {
  // /chat is now intercepted in the dispatcher and never reaches
  // `runEffect`. Save / load / load-error coverage moved to
  // v2-handlers/chat.ts and kas-handlers/chat.ts test files.
  it.skip('removed - see v2-handlers/chat tests', () => {});
});

describe('/spec new effect', () => {
  const specCmd: SlashCommand = {
    name: '/spec',
    description: 'Spec commands',
    source: 'local' as const,
    meta: {
      local: true,
      subcommands: ['new', 'run', 'view', 'analyze_requirements'],
    },
  };

  it('shows usage error when no feature name is given', async () => {
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });
    await runEffect(specCmd, null, ctx, 'new');
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain(
      'Usage: /spec new'
    );
    expect(ctx._spies.setPendingSpecDescription).not.toHaveBeenCalled();
  });

  it('switches to spec mode and arms the description-collection step instead of sending a prompt', async () => {
    const setConfigOption = mock(() => Promise.resolve());
    const ctx = createMockCommandContext({
      slashCommands: [specCmd],
      kiro: { setConfigOption } as any,
      currentAgent: { name: 'default' },
    });
    await runEffect(specCmd, null, ctx, 'new slack bot');

    expect(setConfigOption).toHaveBeenCalledWith('mode', 'spec');
    expect(ctx._spies.setCurrentAgent).toHaveBeenCalledWith({ name: 'spec' });
    // No transcript write: the intro block renders from live state so a
    // cancelled setup leaves no trace.
    expect(ctx._spies.addSystemMessage).not.toHaveBeenCalled();
    // The kickoff prompt is NOT sent yet — it waits for the description.
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
    expect(ctx._spies.setPendingSpecDescription).toHaveBeenCalledWith({
      featureName: 'slack bot',
    });
  });

  it('does not arm the step when the mode switch fails', async () => {
    const setConfigOption = mock(() => Promise.reject(new Error('nope')));
    const ctx = createMockCommandContext({
      slashCommands: [specCmd],
      kiro: { setConfigOption } as any,
    });
    await runEffect(specCmd, null, ctx, 'new my-feature');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('error');
    expect(ctx._spies.setPendingSpecDescription).not.toHaveBeenCalled();
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
  });
});

describe('/spec cloud-session guard', () => {
  const specCmd: SlashCommand = {
    name: '/spec',
    description: 'Spec commands',
    source: 'local' as const,
    meta: {
      local: true,
      subcommands: ['new', 'run', 'view', 'analyze_requirements'],
    },
  };

  // One `it` per form so a regression names the failing form directly.
  const forms: Array<[label: string, args: string]> = [
    ['bare /spec (picker)', ''],
    ['/spec new', 'new slack bot'],
    ['/spec run', 'run alpha'],
    ['/spec view', 'view alpha'],
    ['/spec analyze_requirements', 'analyze_requirements alpha'],
  ];
  for (const [label, args] of forms) {
    it(`refuses ${label} in a cloud session without touching local specs`, async () => {
      const setConfigOption = mock(() => Promise.resolve());
      const ctx = createMockCommandContext({
        slashCommands: [specCmd],
        kiro: { setConfigOption } as any,
        cloudSessionActive: true,
      });
      // The async effect's return value is not observable here, so pin the
      // observable behavior: the alert fired and nothing downstream ran.
      await runEffect(specCmd, null, ctx, args);

      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        '/spec is not available for a cloud session yet.',
        'error',
        5000
      );
      // Nothing downstream ran: no mode switch, no picker, no prompt.
      expect(setConfigOption).not.toHaveBeenCalled();
      expect(ctx._spies.setCurrentAgent).not.toHaveBeenCalled();
      expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
    });
  }

  it('the KIRO_TEST_SPEC_CLOUD_PARITY seam is inert unless the value is exactly "1"', async () => {
    // The cloud-parity E2E lifts the gate with this env var; any other
    // value (including truthy-looking ones) must leave the gate armed so
    // a stray ambient variable can't change user-visible behavior.
    for (const value of ['true', 'yes', '0', '']) {
      process.env.KIRO_TEST_SPEC_CLOUD_PARITY = value;
      try {
        const ctx = createMockCommandContext({
          slashCommands: [specCmd],
          cloudSessionActive: true,
        });
        await runEffect(specCmd, null, ctx, '');
        expect(ctx._spies.showAlert).toHaveBeenCalledWith(
          '/spec is not available for a cloud session yet.',
          'error',
          5000
        );
      } finally {
        delete process.env.KIRO_TEST_SPEC_CLOUD_PARITY;
      }
    }
  });

  it('KIRO_TEST_SPEC_CLOUD_PARITY="1" lifts the gate (the seam is alive)', async () => {
    // The positive half: without it, a rename of the env var would leave
    // the seam permanently inert with this suite green, and only the
    // (skippable) cloud E2E would notice. With the gate lifted and no
    // specs in cwd, the bare form falls through to the local empty-state
    // path instead of the cloud refusal.
    process.env.KIRO_TEST_SPEC_CLOUD_PARITY = '1';
    try {
      const ctx = createMockCommandContext({
        slashCommands: [specCmd],
        cloudSessionActive: true,
      });
      await runEffect(specCmd, null, ctx, '');
      expect(ctx._spies.showAlert).not.toHaveBeenCalledWith(
        '/spec is not available for a cloud session yet.',
        'error',
        5000
      );
    } finally {
      delete process.env.KIRO_TEST_SPEC_CLOUD_PARITY;
    }
  });
});

describe('/spec analyze_requirements effect', () => {
  const specCmd: SlashCommand = {
    name: '/spec',
    description: 'Spec commands',
    source: 'local' as const,
    meta: {
      local: true,
      subcommands: ['new', 'run', 'view', 'analyze_requirements'],
    },
  };

  let workspaceRoot: string;
  let originalCwd: typeof process.cwd;

  beforeEach(() => {
    const { mkdtempSync, mkdirSync, writeFileSync } = require('fs');
    const { tmpdir } = require('os');
    const { join } = require('path');
    workspaceRoot = mkdtempSync(join(tmpdir(), 'spec-analyze-test-'));
    originalCwd = process.cwd;
    process.cwd = () => workspaceRoot;
  });

  afterEach(() => {
    process.cwd = originalCwd;
    const { rmSync } = require('fs');
    rmSync(workspaceRoot, { recursive: true, force: true });
  });

  function makeSpec(featureName: string, files: string[]) {
    const { mkdirSync, writeFileSync } = require('fs');
    const { join } = require('path');
    const dir = join(workspaceRoot, '.kiro', 'specs', featureName);
    mkdirSync(dir, { recursive: true });
    for (const file of files) {
      writeFileSync(join(dir, file), 'content');
    }
  }

  it('shows warning when no specs with requirements.md exist', () => {
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });
    runEffect(specCmd, null, ctx, 'analyze_requirements');
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain(
      'No specs with requirements.md found'
    );
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('warning');
  });

  it('shows picker when no feature name provided and specs exist', () => {
    makeSpec('my-feature', ['requirements.md', 'design.md']);
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });
    runEffect(specCmd, null, ctx, 'analyze_requirements');
    expect(ctx._spies.setActiveCommand).toHaveBeenCalledTimes(1);
    const call = (ctx._spies.setActiveCommand!.mock.calls as any)[0][0];
    expect(call.options[0].label).toBe('my-feature');
    expect(call.options[0].value).toBe('analyze_requirements my-feature');
  });

  it('shows picker when feature name is not an exact match', () => {
    makeSpec('agent-skills', ['requirements.md']);
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });
    runEffect(specCmd, null, ctx, 'analyze_requirements agent');
    expect(ctx._spies.setActiveCommand).toHaveBeenCalledTimes(1);
  });

  it('shows warning when typed name has no requirements.md and no other specs qualify', () => {
    makeSpec('no-reqs', ['design.md']);
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });
    runEffect(specCmd, null, ctx, 'analyze_requirements no-reqs');
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain(
      'No specs with requirements.md found'
    );
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('warning');
  });

  it('switches to spec mode and sends analysis prompt on exact match', async () => {
    makeSpec('my-feature', ['requirements.md']);
    const setConfigOptionMock = mock(() => Promise.resolve());
    const ctx = createMockCommandContext({
      slashCommands: [specCmd],
      kiro: { setConfigOption: setConfigOptionMock },
    });
    await runEffect(specCmd, null, ctx, 'analyze_requirements my-feature');
    expect(setConfigOptionMock).toHaveBeenCalledWith('mode', 'spec');
    expect(ctx._spies.setCurrentAgent).toHaveBeenCalledWith({ name: 'spec' });
    const sendCall = (
      ctx._spies.sendMessage!.mock.calls as any
    )[0][0] as string;
    expect(sendCall).toContain('requirements.md');
    expect(sendCall).toContain('analyze_requirements');
  });

  it('filters picker to only specs that have requirements.md', () => {
    makeSpec('has-reqs', ['requirements.md', 'tasks.md']);
    makeSpec('no-reqs', ['design.md', 'tasks.md']);
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });
    runEffect(specCmd, null, ctx, 'analyze_requirements');
    const call = (ctx._spies.setActiveCommand!.mock.calls as any)[0][0];
    expect(call.options).toHaveLength(1);
    expect(call.options[0].label).toBe('has-reqs');
  });
});

describe('quit effect (/quit cloud prompt)', () => {
  const quitCmd: SlashCommand = {
    name: '/quit',
    description: '',
    source: 'local' as const,
    meta: { local: true },
  };

  it('opens the cloud-quit prompt (no teardown) for an active cloud session', () => {
    const closeSpy = mock(() => {});
    const ctx = createMockCommandContext({
      kiro: { isCloudSessionActive: () => true, close: closeSpy },
    });

    const handled = runEffect(quitCmd, null, ctx, '');

    expect(handled).toBe(true);
    expect(ctx._spies.setShowCloudQuitPrompt!).toHaveBeenCalledWith(true);
    // Must NOT tear down / exit — the prompt's handlers own that.
    expect(closeSpy).not.toHaveBeenCalled();
  });

  it('exits immediately for a local session (unchanged behavior)', () => {
    const exitSpy = spyOn(process, 'exit').mockImplementation(
      (() => undefined) as never
    );
    const closeSpy = mock(() => {});
    const ctx = createMockCommandContext({
      kiro: { isCloudSessionActive: () => false, close: closeSpy },
    });

    runEffect(quitCmd, null, ctx, '');

    expect(closeSpy).toHaveBeenCalledTimes(1);
    expect(exitSpy).toHaveBeenCalledWith(0);
    expect(ctx._spies.setShowCloudQuitPrompt!).not.toHaveBeenCalled();
    exitSpy.mockRestore();
  });
});

describe('/spec view — which document it opens', () => {
  const specCmd: SlashCommand = {
    name: '/spec',
    description: 'Spec commands',
    source: 'local' as const,
    meta: { local: true, subcommands: ['new', 'run', 'view'] },
  };

  let workspaceRoot: string;
  let originalCwd: typeof process.cwd;

  beforeEach(() => {
    const { mkdtempSync } = require('fs');
    const { tmpdir } = require('os');
    const { join } = require('path');
    workspaceRoot = mkdtempSync(join(tmpdir(), 'spec-view-test-'));
    originalCwd = process.cwd;
    process.cwd = () => workspaceRoot;
  });

  afterEach(() => {
    process.cwd = originalCwd;
    const { rmSync } = require('fs');
    rmSync(workspaceRoot, { recursive: true, force: true });
  });

  function makeSpec(featureName: string, files: string[]) {
    const { mkdirSync, writeFileSync } = require('fs');
    const { join } = require('path');
    const dir = join(workspaceRoot, '.kiro', 'specs', featureName);
    mkdirSync(dir, { recursive: true });
    for (const file of files) {
      writeFileSync(join(dir, file), '# content');
    }
  }

  it('opens the panel for a document the parser can summarize', async () => {
    makeSpec('alpha', ['requirements.md']);
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });

    await runEffect(specCmd, null, ctx, 'view alpha requirements');

    expect(ctx._spies.openArtifactView).toHaveBeenCalledWith(
      'alpha',
      'requirements'
    );
  });

  it('opens a bugfix document like any other', async () => {
    makeSpec('a-bug', ['bugfix.md']);
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });

    await runEffect(specCmd, null, ctx, 'view a-bug bugfix');

    expect(ctx._spies.openArtifactView).toHaveBeenCalledWith('a-bug', 'bugfix');
  });

  it('picks bugfix.md when it is the only document, rather than refusing', async () => {
    // Before, the candidate list held the three feature documents, so a bugfix
    // spec was told to "generate requirements first" — naming documents its
    // workflow never writes.
    makeSpec('a-bug', ['bugfix.md']);
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });

    await runEffect(specCmd, null, ctx, 'view a-bug');

    expect(ctx._spies.openArtifactView).toHaveBeenCalledWith('a-bug', 'bugfix');
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it('names bugfix among the documents it accepts', async () => {
    makeSpec('alpha', ['requirements.md']);
    const ctx = createMockCommandContext({ slashCommands: [specCmd] });

    await runEffect(specCmd, null, ctx, 'view alpha nonsense');

    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('bugfix');
  });
});

describe('sendSpecRevision', () => {
  function makeDeps(setConfigOption: () => Promise<void>, busy = false) {
    const sendMessage = mock(
      async (
        _content: string,
        _images?: Array<{ base64: string; mimeType: string }>,
        _displayContent?: string
      ) => {}
    );
    const setCurrentAgent = mock((_agent: { name: string } | null) => {});
    const showAlert = mock(
      (
        _message: string,
        _status: 'error' | 'success' | 'warning',
        _autoHideMs?: number
      ) => {}
    );
    return {
      sendMessage,
      setCurrentAgent,
      showAlert,
      deps: {
        kiro: { setConfigOption } as never,
        setCurrentAgent,
        sendMessage,
        showAlert,
        isBusy: () => busy,
      },
    };
  }

  it('sends the request to the agent and the summary to the transcript', async () => {
    const { deps, sendMessage, setCurrentAgent } = makeDeps(async () => {});

    const sent = await sendSpecRevision(
      deps,
      '<comment on="Glossary" quote="- **Duration**">drop it</comment>',
      'Reviewed requirements.md and left 1 comment:\n- drop it (Glossary)'
    );

    expect(sent).toBe(true);
    expect(setCurrentAgent).toHaveBeenCalledWith({ name: 'spec' });
    // The tagged request is what the agent acts on; the summary is what the
    // transcript shows in its place.
    expect(sendMessage.mock.calls[0]![0]).toContain('<comment on="Glossary"');
    expect(sendMessage.mock.calls[0]![2]).toBe(
      'Reviewed requirements.md and left 1 comment:\n- drop it (Glossary)'
    );
  });

  it('refuses while the agent is busy, so the comments stay staged', async () => {
    // A message sent while a turn is in flight is queued as the text the
    // transcript shows, dropping the tagged request — so this must not report
    // success, or the caller would clear comments the agent never received.
    const { deps, sendMessage, showAlert } = makeDeps(async () => {}, true);

    const sent = await sendSpecRevision(deps, 'request', 'summary');

    expect(sent).toBe(false);
    expect(sendMessage).not.toHaveBeenCalled();
    expect(showAlert.mock.calls[0]![0]).toContain('busy');
  });

  it('refuses when a turn starts during the mode switch', async () => {
    // The mode switch awaits, so a turn can start between the first check and
    // the re-check. Both halves of "we are in spec mode" must have landed
    // before the re-check, so a refusal does not strand the mode without the
    // agent name.
    let busy = false;
    const setConfigOption = mock(async () => {
      busy = true;
    });
    const { deps, sendMessage, showAlert, setCurrentAgent } = makeDeps(
      setConfigOption as never,
      false
    );
    deps.isBusy = () => busy;

    const sent = await sendSpecRevision(deps, 'request', 'summary');

    expect(sent).toBe(false);
    expect(sendMessage).not.toHaveBeenCalled();
    expect(showAlert.mock.calls[0]![0]).toContain('busy');
    // The agent name was set before the re-check, so mode and agent agree.
    expect(setCurrentAgent).toHaveBeenCalledWith({ name: 'spec' });
    expect(setConfigOption).toHaveBeenCalled();
  });

  it('reports a failed mode switch instead of sending', async () => {
    // The caller keeps hand-typed comments staged on a false return, so this is
    // the signal that stops them being discarded for a message that never went.
    const { deps, sendMessage, showAlert } = makeDeps(async () => {
      throw new Error('rpc down');
    });

    const sent = await sendSpecRevision(deps, 'request', 'summary');

    expect(sent).toBe(false);
    expect(sendMessage).not.toHaveBeenCalled();
    expect(showAlert.mock.calls[0]![0]).toContain('rpc down');
  });

  it('calls onSent before dispatching, so the clear is synchronous with the send', async () => {
    const order: string[] = [];
    const onSent = mock(() => {
      order.push('onSent');
    });
    const sendMessage = mock(
      async (
        _content: string,
        _images?: Array<{ base64: string; mimeType: string }>,
        _displayContent?: string
      ) => {
        order.push('sendMessage');
      }
    );
    const setCurrentAgent = mock((_agent: { name: string } | null) => {});
    const showAlert = mock(
      (
        _message: string,
        _status: 'error' | 'success' | 'warning',
        _autoHideMs?: number
      ) => {}
    );

    const sent = await sendSpecRevision(
      {
        kiro: { setConfigOption: async () => {} } as never,
        setCurrentAgent,
        sendMessage,
        showAlert,
        isBusy: () => false,
      },
      'request',
      'summary',
      onSent
    );

    expect(sent).toBe(true);
    expect(onSent).toHaveBeenCalledTimes(1);
    // The ordering is what prevents the mid-turn checkpoint from seeing stale
    // comments: onSent (= clear) must run before sendMessage starts the turn.
    expect(order).toEqual(['onSent', 'sendMessage']);
  });

  it('does not call onSent when the mode switch fails', async () => {
    const onSent = mock(() => {});
    const { deps } = makeDeps(async () => {
      throw new Error('rpc down');
    });

    await sendSpecRevision(deps, 'request', 'summary', onSent);

    expect(onSent).not.toHaveBeenCalled();
  });

  it('does not call onSent when the busy re-check fires', async () => {
    let busy = false;
    const onSent = mock(() => {});
    const { deps } = makeDeps(async () => {
      busy = true;
    }, false);
    deps.isBusy = () => busy;

    await sendSpecRevision(deps, 'request', 'summary', onSent);

    expect(onSent).not.toHaveBeenCalled();
  });
});

describe('/sessions rename discovery source', () => {
  const sessionsCmd: SlashCommand = {
    name: '/sessions',
    description: '',
    source: 'local' as const,
    meta: { local: true },
  };

  let sandbox: string;
  let previousSessionsDir: string | undefined;
  let activeWriteSpy = mockWriteFileSync;

  beforeEach(() => {
    // The sidecar title write must really happen or the effect returns
    // before the rename RPC, so the file-wide writeFileSync no-op is
    // suspended per test and a fresh no-op spy is reinstated afterward.
    activeWriteSpy.mockRestore();
    sandbox = require('fs').mkdtempSync(
      require('path').join(require('os').tmpdir(), 'sessions-rename-')
    );
    previousSessionsDir = process.env.KIRO_TEST_SESSIONS_DIR;
    process.env.KIRO_TEST_SESSIONS_DIR = sandbox;
    require('../../utils/session-bookmarks.js').resetSessionBookmarkStore();
  });

  afterEach(() => {
    activeWriteSpy = spyOn(fs, 'writeFileSync').mockImplementation(() => {});
    if (previousSessionsDir === undefined) {
      delete process.env.KIRO_TEST_SESSIONS_DIR;
    } else {
      process.env.KIRO_TEST_SESSIONS_DIR = previousSessionsDir;
    }
    require('../../utils/session-bookmarks.js').resetSessionBookmarkStore();
    require('fs').rmSync(sandbox, { recursive: true, force: true });
  });

  function renameCtx(cloudActive: boolean) {
    const renameSessionById = mock(() => Promise.resolve(true));
    const ctx = createMockCommandContext({
      kiro: {
        sessionId: 'active-session',
        isCloudSessionActive: () => cloudActive,
        renameSessionById,
      },
    });
    (ctx as { agentEngine: string }).agentEngine = 'kas';
    return { ctx, renameSessionById };
  }

  it('qualifies a local active session rename with source local', () => {
    const { ctx, renameSessionById } = renameCtx(false);

    runEffect(sessionsCmd, null, ctx, 'rename kept conversation');

    expect(renameSessionById).toHaveBeenCalledWith(
      'active-session',
      'kept conversation',
      { source: 'local' }
    );
  });

  it('qualifies a cloud-active session rename with source remote', () => {
    const { ctx, renameSessionById } = renameCtx(true);

    runEffect(sessionsCmd, null, ctx, 'rename remote work');

    expect(renameSessionById).toHaveBeenCalledWith(
      'active-session',
      'remote work',
      { source: 'remote' }
    );
  });
});

describe('/sessions dashboard opens regardless of cloud state', () => {
  const sessionsCmd: SlashCommand = {
    name: '/sessions',
    description: '',
    source: 'local' as const,
    meta: { local: true },
  };

  function dashboardCtx(cloudActive: boolean) {
    const ctx = createMockCommandContext({ cloudSessionActive: cloudActive });
    (ctx as { agentEngine: string }).agentEngine = 'kas';
    return ctx;
  }

  function openDashboard(ctx: ReturnType<typeof dashboardCtx>) {
    // The effect enters the alt screen via a raw stdout escape; stub it so the
    // test doesn't scribble control codes into the runner output.
    const stdoutSpy = spyOn(process.stdout, 'write').mockImplementation(
      () => true
    );
    try {
      runEffect(sessionsCmd, null, ctx, '');
    } finally {
      stdoutSpy.mockRestore();
    }
  }

  it('bare /sessions opens the full-screen dashboard inside a cloud session', () => {
    const ctx = dashboardCtx(true);
    openDashboard(ctx);
    expect(ctx._spies.setShowSessionDashboard).toHaveBeenCalledTimes(1);
    expect((ctx._spies.setShowSessionDashboard as any).mock.calls[0][0]).toBe(
      true
    );
    expect(ctx._spies.setMode).toHaveBeenCalledWith('session-dashboard');
  });

  it('bare /sessions opens the same dashboard outside a cloud session', () => {
    const ctx = dashboardCtx(false);
    openDashboard(ctx);
    expect(ctx._spies.setShowSessionDashboard).toHaveBeenCalledTimes(1);
    expect(ctx._spies.setMode).toHaveBeenCalledWith('session-dashboard');
  });
});
