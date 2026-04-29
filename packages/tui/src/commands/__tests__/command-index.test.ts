import { describe, it, expect } from 'bun:test';
import { executeCommand, executeCommandWithArg } from '../index.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';

function makeCmd(overrides: Partial<SlashCommand> = {}): SlashCommand {
  return {
    name: '/test',
    description: 'test',
    source: 'backend',
    ...overrides,
  };
}

describe('executeCommand', () => {
  it('returns false for non-slash input', async () => {
    const ctx = createMockCommandContext();
    const result = await executeCommand('hello world', ctx);
    expect(result).toBe(false);
  });

  it('returns true for recognized commands', async () => {
    const cmd = makeCmd({ name: '/clear', meta: { local: true } });
    const ctx = createMockCommandContext({ slashCommands: [cmd] });
    const result = await executeCommand('/clear', ctx);
    expect(result).toBe(true);
  });

  it('returns false for unknown slash commands', async () => {
    const ctx = createMockCommandContext({ slashCommands: [] });
    const result = await executeCommand('/nonexistent', ctx);
    expect(result).toBe(false);
  });

  it('matches by prefix (e.g. /cl matches /clear)', async () => {
    const cmd = makeCmd({ name: '/clear', meta: { local: true } });
    const ctx = createMockCommandContext({ slashCommands: [cmd] });
    const result = await executeCommand('/cl', ctx);
    expect(result).toBe(true);
    // Should have dispatched the clear command (calls clearMessages effect)
    expect(ctx._spies.clearMessages!).toHaveBeenCalled();
  });

  it('parses args correctly', async () => {
    const cmd = makeCmd({ name: '/help', source: 'backend' });
    const ctx = createMockCommandContext({ slashCommands: [cmd] });
    (ctx.kiro.executeCommand as any).mockResolvedValue({
      success: true,
      message: 'Help output',
      data: { commands: [] },
    });

    const result = await executeCommand('/help topic', ctx);
    expect(result).toBe(true);
    // The /help command should have been executed with args 'topic'
    const executeCall = (ctx.kiro.executeCommand as any).mock.calls[0]!;
    expect(executeCall[0].command).toBe('help');
    expect(executeCall[0].args).toEqual({ value: 'topic' });
  });

  it('returns false for file-path-like input starting with /', async () => {
    const ctx = createMockCommandContext();
    const result = await executeCommand('/Users/foo/bar.txt', ctx);
    expect(result).toBe(false);
  });

  it('matches case-insensitively (e.g. /CLEAR matches /clear)', async () => {
    const cmd = makeCmd({ name: '/clear', meta: { local: true } });
    const ctx = createMockCommandContext({ slashCommands: [cmd] });
    const result = await executeCommand('/CLEAR', ctx);
    expect(result).toBe(true);
    expect(ctx._spies.clearMessages!).toHaveBeenCalled();
  });

  it('prefix matches alphabetically when multiple commands share prefix', async () => {
    const clearCmd = makeCmd({ name: '/clear', meta: { local: true } });
    const compactCmd = makeCmd({
      name: '/compact',
      source: 'backend',
    });
    const ctx = createMockCommandContext({
      slashCommands: [clearCmd, compactCmd],
    });

    // /cl should match /clear (alphabetical first)
    const result1 = await executeCommand('/cl', ctx);
    expect(result1).toBe(true);
    expect(ctx._spies.clearMessages!).toHaveBeenCalled();

    // /co should match /compact
    const ctx2 = createMockCommandContext({
      slashCommands: [clearCmd, compactCmd],
    });
    (ctx2.kiro.executeCommand as any).mockResolvedValue({
      success: true,
      message: '',
      data: undefined,
    });
    const result2 = await executeCommand('/co', ctx2);
    expect(result2).toBe(true);
    expect(ctx2.kiro.executeCommand).toHaveBeenCalled();
    const executeCall = (ctx2.kiro.executeCommand as any).mock.calls[0]!;
    expect(executeCall[0].command).toBe('compact');
  });

  it('exact match takes priority over prefix match', async () => {
    const cCmd = makeCmd({ name: '/c', source: 'backend' });
    const clearCmd = makeCmd({ name: '/clear', meta: { local: true } });
    const ctx = createMockCommandContext({
      slashCommands: [cCmd, clearCmd],
    });
    (ctx.kiro.executeCommand as any).mockResolvedValue({
      success: true,
      message: '',
      data: undefined,
    });

    const result = await executeCommand('/c', ctx);
    expect(result).toBe(true);
    // Should match /c exactly, not prefix-match to /clear
    expect(ctx.kiro.executeCommand).toHaveBeenCalled();
    const executeCall = (ctx.kiro.executeCommand as any).mock.calls[0]!;
    expect(executeCall[0].command).toBe('c');
    // clearMessages should NOT have been called (that's the /clear effect)
    expect(ctx._spies.clearMessages!.mock.calls.length).toBe(0);
  });
});

describe('executeCommandWithArg', () => {
  it('finds command by name and dispatches', async () => {
    const cmd = makeCmd({ name: '/help', source: 'backend' });
    const ctx = createMockCommandContext({ slashCommands: [cmd] });
    (ctx.kiro.executeCommand as any).mockResolvedValue({
      success: true,
      message: '',
      data: { commands: [] },
    });

    await executeCommandWithArg('help', 'topic', ctx);

    expect(ctx.kiro.executeCommand).toHaveBeenCalled();
  });

  it('shows alert for unknown command', async () => {
    const ctx = createMockCommandContext({ slashCommands: [] });

    await executeCommandWithArg('nonexistent', 'arg', ctx);

    expect(ctx._spies.showAlert!).toHaveBeenCalled();
    const call = ctx._spies.showAlert!.mock.calls[0]!;
    expect(call[0]).toContain('Unknown command');
    expect(call[1]).toBe('error');
  });

  it('prepends "swap " for /agent command', async () => {
    const cmd = makeCmd({ name: '/agent', source: 'backend' });
    const ctx = createMockCommandContext({ slashCommands: [cmd] });
    (ctx.kiro.executeCommand as any).mockResolvedValue({
      success: true,
      message: 'Agent switched',
      data: { agent: { name: 'coder' } },
    });

    await executeCommandWithArg('agent', 'coder', ctx);

    const executeCall = (ctx.kiro.executeCommand as any).mock.calls[0]!;
    expect(executeCall[0].args.value).toBe('swap coder');
  });

  it('dispatches with empty string arg when argValue is empty', async () => {
    const cmd = makeCmd({ name: '/help', source: 'backend' });
    const ctx = createMockCommandContext({ slashCommands: [cmd] });
    (ctx.kiro.executeCommand as any).mockResolvedValue({
      success: true,
      message: '',
      data: undefined,
    });

    await executeCommandWithArg('help', '', ctx);

    // Should still dispatch - executeCommand should be called with empty args
    expect(ctx.kiro.executeCommand).toHaveBeenCalled();
    const executeCall = (ctx.kiro.executeCommand as any).mock.calls[0]!;
    expect(executeCall[0].command).toBe('help');
    expect(executeCall[0].args).toEqual({});
  });
});
