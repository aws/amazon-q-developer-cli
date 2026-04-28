import { describe, it, expect } from 'bun:test';
import { executeCommand } from '../index';
import { createMockCommandContext } from './test-helpers.js';
import type { SlashCommand } from '../../stores/app-store';

const saveCmd: SlashCommand = {
  name: '/save',
  description: 'Save session',
  source: 'backend',
};

describe('executeCommand', () => {
  it('returns false for unknown commands (no error shown)', async () => {
    const ctx = createMockCommandContext({ slashCommands: [saveCmd] });
    const result = await executeCommand('/notacommand', ctx);
    expect(result).toBe(false);
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it('returns false for pasted file paths like /Users/me/file.txt', async () => {
    const ctx = createMockCommandContext({ slashCommands: [saveCmd] });
    const result = await executeCommand('/Users/me/file.txt', ctx);
    expect(result).toBe(false);
  });

  it('returns false for extensionless filenames like /Makefile', async () => {
    const ctx = createMockCommandContext({ slashCommands: [saveCmd] });
    expect(await executeCommand('/Makefile', ctx)).toBe(false);
    expect(await executeCommand('/Dockerfile explain this', ctx)).toBe(false);
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it('returns true and dispatches for known commands', async () => {
    const ctx = createMockCommandContext({ slashCommands: [saveCmd] });
    const result = await executeCommand('/save', ctx);
    expect(result).toBe(true);
  });

  it('returns true for prefix matches', async () => {
    const ctx = createMockCommandContext({ slashCommands: [saveCmd] });
    const result = await executeCommand('/sav', ctx);
    expect(result).toBe(true);
  });

  it('returns false for non-slash input', async () => {
    const ctx = createMockCommandContext({ slashCommands: [saveCmd] });
    const result = await executeCommand('hello world', ctx);
    expect(result).toBe(false);
  });
});
