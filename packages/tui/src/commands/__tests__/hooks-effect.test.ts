import { describe, it, expect, mock } from 'bun:test';

// --- Module mocks MUST be declared before importing the module under test ---
mock.module('child_process', () => ({
  spawnSync: mock(() => ({ status: 1 })),
}));
mock.module('fs', () => ({
  writeFileSync: mock(() => {}),
  readFileSync: () => '',
}));

import { runEffect } from '../effects.js';
import type { SlashCommand } from '../../stores/app-store.js';
import { createMockCommandContext } from './test-helpers.js';

const hooksCmd: SlashCommand = {
  name: '/hooks',
  description: 'View configured hooks',
  source: 'backend',
  meta: { inputType: 'panel' },
};

describe('/hooks effect', () => {
  it('opens hooks panel with hook data', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: '2 hooks configured',
      data: {
        hooks: [
          { trigger: 'agentSpawn', command: 'git status' },
          {
            trigger: 'preToolUse',
            command: 'validate.sh',
            matcher: 'fs_write',
          },
        ],
        message: '2 hooks configured',
      },
    };

    runEffect(hooksCmd, result, ctx, '');

    expect(ctx._spies.setShowHooksPanel!).toHaveBeenCalledTimes(1);
    const call = ctx._spies.setShowHooksPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toHaveLength(2);
    expect(call[1][0].trigger).toBe('agentSpawn');
    expect(call[1][1].matcher).toBe('fs_write');
  });

  it('opens hooks panel with empty list when no hooks configured', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: 'No hooks configured',
      data: { hooks: [], message: 'No hooks configured' },
    };

    runEffect(hooksCmd, result, ctx, '');

    expect(ctx._spies.setShowHooksPanel!).toHaveBeenCalledTimes(1);
    const call = ctx._spies.setShowHooksPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toHaveLength(0);
  });

  it('opens hooks panel with empty list when result has no data', () => {
    const ctx = createMockCommandContext();
    const result = {
      success: true,
      message: 'No hooks configured',
      data: undefined,
    };

    runEffect(hooksCmd, result, ctx, '');

    expect(ctx._spies.setShowHooksPanel!).toHaveBeenCalledTimes(1);
    const call = ctx._spies.setShowHooksPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toHaveLength(0);
  });

  it('opens hooks panel with empty list when result is null', () => {
    const ctx = createMockCommandContext();

    runEffect(hooksCmd, null, ctx, '');

    expect(ctx._spies.setShowHooksPanel!).toHaveBeenCalledTimes(1);
    const call = ctx._spies.setShowHooksPanel!.mock.calls[0]!;
    expect(call[0]).toBe(true);
    expect(call[1]).toHaveLength(0);
  });

  it('is recognized as a valid effect for the hooks command', () => {
    const ctx = createMockCommandContext();
    runEffect(hooksCmd, null, ctx, '');
    expect(ctx._spies.setShowHooksPanel!).toHaveBeenCalled();
  });
});
