import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';

// --- Module mocks MUST be declared before importing the module under test ---
const mockSpawnSync = mock(() => ({ status: 1 }));
const mockWriteFileSync = mock(() => {});

mock.module('child_process', () => ({ spawnSync: mockSpawnSync }));
mock.module('fs', () => ({
  writeFileSync: mockWriteFileSync,
  readFileSync: () => '',
}));

import { runEffect } from '../effects.js';
import { MessageRole } from '../../stores/app-store.js';
import type { SlashCommand } from '../../stores/app-store.js';
import type { CommandContext } from '../types.js';

const copyCmd: SlashCommand = {
  name: '/copy',
  description: '',
  source: 'local' as const,
  meta: { local: true },
};

function createMockCtx(
  messages: Array<{ id: string; role: string; content: string }> = []
): CommandContext & { _spies: Record<string, ReturnType<typeof mock>> } {
  const spies: Record<string, ReturnType<typeof mock>> = {};
  const spy = (name: string) => {
    const fn = mock(() => {});
    spies[name] = fn;
    return fn;
  };
  return {
    kiro: {} as any,
    slashCommands: [copyCmd],
    showAlert: spy('showAlert') as any,
    setLoadingMessage: spy('setLoadingMessage') as any,
    setActiveCommand: spy('setActiveCommand') as any,
    setCurrentModel: spy('setCurrentModel') as any,
    setCurrentAgent: spy('setCurrentAgent') as any,
    setContextUsage: spy('setContextUsage') as any,
    setShowContextBreakdown: spy('setShowContextBreakdown') as any,
    setShowHelpPanel: spy('setShowHelpPanel') as any,
    setShowUsagePanel: spy('setShowUsagePanel') as any,
    setShowMcpPanel: spy('setShowMcpPanel') as any,
    setShowToolsPanel: spy('setShowToolsPanel') as any,
    setShowKnowledgePanel: spy('setShowKnowledgePanel') as any,
    setShowCodePanel: spy('setShowCodePanel') as any,
    clearMessages: spy('clearMessages') as any,
    resetMessages: spy('resetMessages') as any,
    sendMessage: spy('sendMessage') as any,
    clearUIState: spy('clearUIState') as any,
    createStreamEventHandler: spy('createStreamEventHandler') as any,
    setSessionId: spy('setSessionId') as any,
    addSystemMessage: spy('addSystemMessage') as any,
    addSession: spy('addSession') as any,
    setActiveSession: spy('setActiveSession') as any,
    sessions: new Map(),
    setMode: spy('setMode') as any,
    getMessages: mock(() => messages) as any,
    setUserColors: spy('setUserColors') as any,
    setThemePreview: spy('setThemePreview') as any,
    _spies: spies,
  };
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
    expect(mockWriteFileSync.mock.calls[0]![0]).toBe('/dev/tty');
    const b64 = Buffer.from(text, 'utf-8').toString('base64');
    expect(mockWriteFileSync.mock.calls[0]![1]).toBe(`\x1b]52;c;${b64}\x07`);
    expect(ctx._spies.showAlert!.mock.calls[0]![0]).toContain('Copied');
    expect(ctx._spies.showAlert!.mock.calls[0]![1]).toBe('success');
  });

  it('writes correct base64 encoding in OSC 52 sequence', () => {
    const text = 'Unicode: 日本語 🎉';
    const ctx = createMockCtx([modelMessage(text)]);

    runEffect(copyCmd, null, ctx, '');

    const expected = `\x1b]52;c;${Buffer.from(text, 'utf-8').toString('base64')}\x07`;
    expect(mockWriteFileSync.mock.calls[0]![1]).toBe(expected);
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
