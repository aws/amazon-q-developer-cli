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
import {
  __setListAllSessionsOverrideForTests,
  type ListAllSessionsResult,
} from '../../utils/list-all-sessions-cli';

const listAllSessionsMock = mock<() => Promise<ListAllSessionsResult>>(() =>
  Promise.resolve({ ok: false, error: 'not stubbed' })
);

beforeEach(() => {
  __setListAllSessionsOverrideForTests(() => listAllSessionsMock());
});

afterEach(() => {
  __setListAllSessionsOverrideForTests(undefined);
});

const resolveAgentEngineMock = mock<() => 'kas' | 'v2'>(() => 'v2');
mock.module('../../agent-engine', () => ({
  resolveAgentEngine: () => resolveAgentEngineMock(),
}));

// Prevent /editor effect from spawning a real $EDITOR subprocess during tests.
const mockSpawnSync = mock(() => ({ status: 1 }));
import * as realChildProcess from 'child_process';
// A module mock is process-wide, so the real exports are carried over rather
// than dropped: a suite loaded later that imports a different export would
// otherwise resolve against a module that no longer provides it.
mock.module('child_process', () => ({
  ...realChildProcess,
  spawnSync: mockSpawnSync,
}));

import * as fs from 'fs';
const mockWriteFileSync = spyOn(fs, 'writeFileSync').mockImplementation(
  () => {}
);

afterAll(() => {
  mockWriteFileSync.mockRestore();
  mock.restore();
});

import { dispatch } from '../dispatcher';
import type { SlashCommand } from '../../stores/app-store';
import { createMockCommandContext } from './test-helpers.js';

function makeCmd(overrides: Partial<SlashCommand> = {}): SlashCommand {
  return {
    name: '/test',
    description: 'test',
    source: 'backend',
    ...overrides,
  };
}

describe('dispatch - additional coverage', () => {
  describe('prompt type commands', () => {
    it('sends /<cmd> as message when no args', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/fix',
        meta: { type: 'prompt' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledWith('/fix');
    });

    it('sends /<cmd> <args> as message when args provided', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/fix',
        meta: { type: 'prompt' },
      });

      await dispatch(cmd, 'the bug in auth', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledWith(
        '/fix the bug in auth'
      );
    });
  });

  describe('skill type commands', () => {
    it('sends message for skill commands', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/review',
        meta: { type: 'skill' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledWith('/review');
    });
  });

  describe('steering type commands', () => {
    it('sends message for steering commands without args', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/project-context',
        meta: { type: 'steering' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledWith('/project-context');
    });

    it('sends message for steering commands with args', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/project-context',
        meta: { type: 'steering' },
      });

      await dispatch(cmd, 'extra info', ctx);

      expect(ctx._spies.sendMessage!).toHaveBeenCalledWith(
        '/project-context extra info'
      );
    });
  });

  describe('panel inputType with no args', () => {
    it('sets activeCommand with empty options', async () => {
      const ctx = createMockCommandContext();
      const cmd = makeCmd({
        name: '/tools',
        meta: { inputType: 'panel' },
      });

      await dispatch(cmd, '', ctx);

      expect(ctx._spies.setActiveCommand!).toHaveBeenCalled();
      const call = ctx._spies.setActiveCommand!.mock.calls[0]!;
      expect(call[0].command.name).toBe('/tools');
      expect(call[0].options).toEqual([]);
    });
  });

  describe('backend command execution', () => {
    it('calls kiro.executeCommand with command and args', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'done',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/compact',
        source: 'backend',
      });
      await dispatch(cmd, 'aggressive', ctx);

      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
      const call = (ctx.kiro.executeCommand as any).mock.calls[0]!;
      expect(call[0].command).toBe('compact');
      expect(call[0].args).toEqual({ value: 'aggressive' });
    });

    it('shows error alert when kiro.executeCommand fails', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockRejectedValue(
        new Error('Connection failed')
      );

      const cmd = makeCmd({
        name: '/compact',
        source: 'backend',
      });
      await dispatch(cmd, '', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const call = ctx._spies.showAlert!.mock.calls[0]!;
      expect(call[0]).toBe('Connection failed');
      expect(call[1]).toBe('error');
    });
  });

  describe('result.message display', () => {
    it('shows result.message when no effect handled messaging', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Operation completed',
        data: undefined,
      });

      // Use a command name that has no effect handler
      const cmd = makeCmd({
        name: '/unknown-backend',
        source: 'backend',
      });
      await dispatch(cmd, 'args', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const call = ctx._spies.showAlert!.mock.calls[0]!;
      expect(call[0]).toBe('Operation completed');
      expect(call[1]).toBe('success');
    });

    it('shows error status for unsuccessful result', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: false,
        message: 'Something went wrong',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/unknown-backend',
        source: 'backend',
      });
      await dispatch(cmd, 'args', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const call = ctx._spies.showAlert!.mock.calls[0]!;
      expect(call[0]).toBe('Something went wrong');
      expect(call[1]).toBe('error');
    });

    it('shows /knowledge update errors with args via tail alert', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: false,
        message: 'No contexts found under /tmp/missing',
        data: undefined,
      });

      const cmd = makeCmd({
        name: '/knowledge',
        source: 'backend',
        meta: { inputType: 'panel' },
      });
      await dispatch(cmd, 'update /tmp/missing', ctx);

      expect(ctx._spies.setShowKnowledgePanel!).toHaveBeenCalledWith(false);
      expect(ctx._spies.showAlert!).toHaveBeenCalledTimes(1);
      expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
        'No contexts found under /tmp/missing',
        'error',
        5000
      );
    });
  });

  describe('local commands skip backend', () => {
    it('does not call executeCommand for local commands', async () => {
      const ctx = createMockCommandContext();

      const cmd = makeCmd({
        name: '/editor',
        source: 'local' as const,
        meta: { local: true },
      });
      // runEffect for 'editor' calls openEditorSync which we can't test here,
      // but we verify executeCommand was NOT called
      await dispatch(cmd, '', ctx);

      expect(ctx.kiro.executeCommand).not.toHaveBeenCalled();
    });
  });

  describe('/agent swap loading message', () => {
    it('shows loading message when swapping agent', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Agent switched',
        data: { agent: { name: 'coder' } },
      });

      const cmd = makeCmd({
        name: '/agent',
        source: 'backend',
      });
      await dispatch(cmd, 'swap coder', ctx);

      // setLoadingMessage should have been called with the agent name
      const loadingCalls = ctx._spies.setLoadingMessage!.mock.calls;
      const hasAgentMessage = loadingCalls.some(
        (c: any[]) => typeof c[0] === 'string' && c[0].includes('coder')
      );
      expect(hasAgentMessage).toBe(true);
    });
  });

  describe('/chat options fetched via merged --list-sessions', () => {
    // /chat option formatting moved into v2-handlers/chat.ts and
    // kas-handlers/chat.ts. Coverage lives in those handlers' test
    // files; the dispatcher's routing of /chat is exercised by
    // kas-intercept.test.ts.
    it.skip('see v2-handlers/chat tests + kas-intercept.test.ts', () => {});
  });

  describe('selection with no options', () => {
    it('shows alert for non-chat commands with no options', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.getCommandOptions as any).mockResolvedValue({
        options: [],
      });

      const cmd = makeCmd({
        name: '/model',
        meta: { inputType: 'selection' },
      });
      await dispatch(cmd, '', ctx);

      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const call = ctx._spies.showAlert!.mock.calls[0]!;
      expect(call[0]).toContain('No options available');
      expect(call[1]).toBe('error');
    });

    it('falls through to backend for /effort with no options', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.getCommandOptions as any).mockResolvedValue({
        options: [],
      });

      const cmd = makeCmd({
        name: '/effort',
        meta: { inputType: 'selection' },
      });
      await dispatch(cmd, '', ctx);

      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
    });
  });

  // Regression: the loader for `/compact` is owned by the backend's
  // `compaction_status` event stream (it sets loadingMessage to "Compacting
  // conversation..."). The dispatcher must NOT clear loadingMessage for
  // commands it didn't set one for — its post-RPC setLoadingMessage(null) used
  // to fire the instant the (async-spawned) compact RPC returned, nulling the
  // event-driven loader a few ms after it appeared so the spinner never showed.
  describe('loadingMessage ownership', () => {
    it('does not clear loadingMessage for /compact (event-owned loader)', async () => {
      const ctx = createMockCommandContext();
      (ctx.kiro.executeCommand as any).mockResolvedValue({
        success: true,
        message: 'Compacting conversation...',
      });

      await dispatch(makeCmd({ name: '/compact' }), '', ctx);

      expect(ctx.kiro.executeCommand).toHaveBeenCalled();
      expect(ctx._spies.setLoadingMessage!).not.toHaveBeenCalled();
    });

    it('still clears the loadingMessage it set itself for /agent swap', async () => {
      const ctx = createMockCommandContext();

      await dispatch(makeCmd({ name: '/agent' }), 'swap planner', ctx);

      const calls = ctx._spies.setLoadingMessage!.mock.calls.map((c) => c[0]);
      // Sets the swap label, then clears it once the RPC resolves.
      expect(calls).toContain('Agent changing to planner');
      expect(calls).toContain(null);
    });
  });

  describe('cloudOnly guard', () => {
    it('refuses a cloudOnly command outside a cloud session', async () => {
      // Mock ctx defaults to cloudSessionActive: false.
      const ctx = createMockCommandContext();
      await dispatch(
        makeCmd({ name: '/disconnect', meta: { cloudOnly: true } }),
        '',
        ctx
      );
      // No handler ran — the guard returned before dispatch.
      expect(ctx._spies.sendMessage!).not.toHaveBeenCalled();
      expect(ctx._spies.setActiveCommand!).not.toHaveBeenCalled();
    });
  });

  describe('localOnly guard (cloud workflow stopgap, kiro-agent #178)', () => {
    it('refuses a localOnly command (prefix-typed) inside a cloud session', async () => {
      const ctx = createMockCommandContext({ cloudSessionActive: true });
      await dispatch(
        makeCmd({ name: '/workflow', meta: { localOnly: true, local: true } }),
        'run demo',
        ctx
      );
      // Guard returned before any input gathering or execution.
      expect(ctx._spies.sendMessage!).not.toHaveBeenCalled();
      expect(ctx._spies.setActiveCommand!).not.toHaveBeenCalled();
    });

    it('allows a localOnly command in a local (non-cloud) session', async () => {
      // Mock ctx defaults to cloudSessionActive: false. `/workflow` is a
      // KAS-owned panel, so dispatch through the KAS engine — under v2 the
      // shared panel guard would refuse it for an unrelated reason.
      const listWorkflows = mock(() => Promise.resolve([]));
      const ctx = createMockCommandContext({
        kiro: { listWorkflows } as any,
      });
      ctx.agentEngine = 'kas';
      await dispatch(
        makeCmd({
          name: '/workflow',
          meta: { localOnly: true, inputType: 'panel', local: true },
        }),
        '',
        ctx
      );
      // The KAS handler ran (bare /workflow opens history) rather than being
      // refused by either guard.
      expect(listWorkflows).toHaveBeenCalled();
    });
  });
});
