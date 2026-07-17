import { describe, expect, it, mock } from 'bun:test';
import type { HookInfo } from '../../../stores/app-store';
import { KasCommandName, type KasCommand } from '../../../kas-commands';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import { handleHooks } from '../hooks';

const hooksCmd: KasCommand = {
  name: KasCommandName.Hooks,
  description: 'View configured hooks',
  meta: { inputType: 'panel' },
};

describe('/hooks KAS handler', () => {
  it('opens the panel from the store snapshot without a transport fetch', async () => {
    const hooksList: HookInfo[] = [
      { trigger: 'preToolUse', matcher: 'write', command: 'validate.sh' },
    ];
    const executeCommand = mock(() =>
      Promise.resolve({ success: true, message: '' })
    );
    const ctx = createMockCommandContext({
      hooksList,
      kiro: { executeCommand },
    });

    await handleHooks(hooksCmd, '', ctx);

    expect(executeCommand).not.toHaveBeenCalled();
    expect(ctx._spies.setShowHooksPanel).toHaveBeenCalledWith(true, hooksList);
  });

  it('fetches hooks when the store snapshot is empty', async () => {
    const hooks: HookInfo[] = [
      { trigger: 'agentSpawn', command: 'git status' },
    ];
    const executeCommand = mock(() =>
      Promise.resolve({
        success: true,
        message: '1 hook configured',
        data: { hooks },
      })
    );
    const ctx = createMockCommandContext({
      hooksList: [],
      kiro: { executeCommand },
    });

    await handleHooks(hooksCmd, '', ctx);

    expect(executeCommand).toHaveBeenCalledTimes(1);
    expect(ctx._spies.setShowHooksPanel).toHaveBeenCalledWith(true, hooks);
  });

  it('surfaces transport failures without opening the panel', async () => {
    const ctx = createMockCommandContext({
      kiro: {
        executeCommand: mock(() =>
          Promise.resolve({ success: false, message: 'agent unavailable' })
        ),
      },
    });

    await handleHooks(hooksCmd, '', ctx);

    expect(ctx._spies.setShowHooksPanel).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'agent unavailable',
      'error',
      5000
    );
  });
});
