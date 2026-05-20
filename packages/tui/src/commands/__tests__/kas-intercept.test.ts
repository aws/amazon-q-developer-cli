import { describe, it, expect, mock } from 'bun:test';
import { dispatch } from '../dispatcher';
import { createMockCommandContext } from '../__tests__/test-helpers';
import type { SlashCommand } from '../../stores/app-store';
import type { KasCommand } from '../../kas-commands';
import { KasCommandName } from '../../kas-commands';

const CHAT_CMD: SlashCommand = {
  name: '/chat',
  description: 'x',
  source: 'backend',
  meta: { inputType: 'selection', local: true },
};
const HELP_CMD: KasCommand = {
  name: KasCommandName.Help,
  description: 'x',
  meta: { inputType: 'panel' },
};

describe('dispatcher KAS intercept', () => {
  it("agentEngine='kas' + /chat: skips backend and runs handler", async () => {
    const exec = mock(() =>
      Promise.resolve({ success: true, message: '', data: undefined })
    );
    const listSessions = mock(() =>
      Promise.resolve({ sessions: [], nextCursor: undefined })
    );
    const ctx = createMockCommandContext({
      slashCommands: [CHAT_CMD],
      kiro: {
        sessionId: 'cur',
        executeCommand: exec,
        listSessions,
      } as any,
    });
    ctx.agentEngine = 'kas';
    await dispatch(CHAT_CMD, '', ctx);
    expect((listSessions as any).mock.calls.length).toBe(1);
    expect((exec as any).mock.calls.length).toBe(0);
  });

  it("agentEngine='rust' + /chat save: falls through to V2 backend executeCommand", async () => {
    const exec = mock(() =>
      Promise.resolve({
        success: true,
        message: 'saved',
        data: undefined,
      })
    );
    const ctx = createMockCommandContext({
      slashCommands: [CHAT_CMD],
      kiro: {
        sessionId: 'cur',
        executeCommand: exec,
      } as any,
    });
    ctx.agentEngine = 'rust';
    await dispatch(CHAT_CMD, 'save', ctx);
    expect((exec as any).mock.calls.length).toBe(1);
    expect((exec as any).mock.calls[0][0]).toEqual({
      command: 'chat',
      args: { value: 'save' },
    });
  });

  it("agentEngine='kas' + non-handler command: falls through to existing dispatch", async () => {
    const ctx = createMockCommandContext({
      kasCommands: [HELP_CMD],
    });
    ctx.agentEngine = 'kas';
    await dispatch(HELP_CMD, '', ctx);
    expect(ctx._spies.setActiveCommand).toHaveBeenCalled();
  });
});
