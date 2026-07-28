import { describe, expect, it } from 'bun:test';
import { KAS_COMMANDS, KasCommandName } from '../../kas-commands.js';
import type { AvailableCommand } from '../../types/commands.js';
import { executeCommand } from '../index.js';
import { createMockCommandContext } from './test-helpers.js';

const goalCommand =
  KAS_COMMANDS.find((command) => command.name === KasCommandName.Goal) ??
  (() => {
    throw new Error('KAS /goal command is not registered');
  })();

function createKasContext() {
  const ctx = createMockCommandContext({ kasCommands: [goalCommand] });
  ctx.agentEngine = 'kas';
  return ctx;
}

describe('executeCommand - /goal', () => {
  it('opens the existing goal panel for a bare KAS invocation', async () => {
    const ctx = createKasContext();

    expect(await executeCommand('/goal', ctx)).toBe(true);

    expect(ctx._spies.setShowGoalPanel).toHaveBeenCalledWith(true);
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
    expect(ctx.kiro.executeCommand).not.toHaveBeenCalled();
  });

  it('passes every non-bare KAS invocation through unchanged', async () => {
    for (const input of [
      '/goal build a static site --max 3',
      '/goal clear',
      '/goal status',
      '/goal explore --max 0',
    ]) {
      const ctx = createKasContext();

      expect(await executeCommand(input, ctx)).toBe(true);
      expect(ctx._spies.sendMessage).toHaveBeenCalledWith(input);
      expect(ctx._spies.setShowGoalPanel).not.toHaveBeenCalled();
      expect(ctx.kiro.executeCommand).not.toHaveBeenCalled();
    }
  });

  it('preserves the V2 clear command boundary', async () => {
    const v2Goal: AvailableCommand = {
      name: '/goal',
      description: 'Manage a goal',
    };
    const ctx = createMockCommandContext({ slashCommands: [v2Goal] });

    expect(await executeCommand('/goal clear', ctx)).toBe(true);

    expect(ctx.kiro.executeCommand).toHaveBeenCalled();
    expect(ctx._spies.sendMessage).not.toHaveBeenCalled();
  });
});
