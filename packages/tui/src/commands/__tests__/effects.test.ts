import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';

// --- Module mocks MUST be declared before importing the module under test ---
const mockSpawnSync = mock(() => ({ status: 1 }));
const mockWriteFileSync = mock((_path: string, _data: string) => {});

mock.module('child_process', () => ({ spawnSync: mockSpawnSync }));
mock.module('fs', () => ({
  writeFileSync: mockWriteFileSync,
  readFileSync: () => '',
}));

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
