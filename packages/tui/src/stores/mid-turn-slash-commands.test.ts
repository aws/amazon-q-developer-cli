/**
 * Mid-turn slash command routing in the modern TUI.
 *
 * Every slash command typed during a turn used to be refused with "Slash
 * commands can't be queued". Now a known command is held and dispatched at
 * turn-end — the behavior lite already had — while bare unknown names are
 * refused and slash-prefixed prose keeps its message routing. These tests pin
 * those outcomes and ensure a held command runs when the turn ends.
 */

import { describe, it, expect, mock, beforeAll } from 'bun:test';
import { tmpdir } from 'os';
import { join } from 'path';
import { createAppStore, MessageRole } from './app-store';
import { Kiro } from '../kiro';
import { CommandHistory } from '../utils/command-history';
import { getKasCommands } from '../kas-commands';

beforeAll(() => {
  CommandHistory.getInstance().switchToFile(
    join(tmpdir(), `mid-turn-slash-commands-${process.pid}.history`)
  );
});

/** A TUI-mode store mid-turn, with the commands these tests dispatch. */
function createBusyStore() {
  const kiro = new Kiro();
  const store = createAppStore({ kiro });
  store.setState({
    isInitialized: true,
    sessionId: 'test-session',
    uiMode: 'tui',
    isProcessing: true,
    activeInterruptMode: 'steer',
    slashCommands: [
      ...store.getState().slashCommands,
      { name: '/model', description: 'Switch model', source: 'local' as const },
      { name: '/compact', description: 'Compact', source: 'local' as const },
      { name: '/plan', description: 'Plan mode', source: 'local' as const },
      {
        name: '/agent',
        description: 'Switch agent',
        source: 'backend' as const,
      },
      {
        name: '/transcript',
        description: 'Open transcript',
        source: 'local' as const,
      },
      { name: '/goal', description: 'Goal', source: 'backend' as const },
      { name: '/context', description: 'Context', source: 'backend' as const },
      { name: '/mcp', description: 'MCP', source: 'backend' as const },
      { name: '/tools', description: 'Tools', source: 'backend' as const },
      {
        name: '/knowledge',
        description: 'Knowledge',
        source: 'backend' as const,
      },
      { name: '/usage', description: 'Usage', source: 'backend' as const },
      { name: '/stats', description: 'Stats', source: 'backend' as const },
      // Backend-sourced, so dispatch issues an RPC instead of running locally.
      {
        name: '/help',
        description: 'Show help',
        source: 'backend' as const,
        meta: { inputType: 'panel' as const },
      },
    ],
  });
  return store;
}

describe('mid-turn slash commands (TUI)', () => {
  describe('queue policy', () => {
    it('queues /model instead of rejecting it', async () => {
      const store = createBusyStore();

      await store.getState().handleUserInput('/model');

      expect(store.getState().queuedMessages).toEqual(['/model']);
      // The queue strip is the feedback surface — no rejection alert.
      expect(store.getState().transientAlert).toBeNull();
    });

    it('queues /model with an argument verbatim', async () => {
      const store = createBusyStore();

      await store.getState().handleUserInput('/model claude-sonnet-5');

      expect(store.getState().queuedMessages).toEqual([
        '/model claude-sonnet-5',
      ]);
    });

    it('keeps a queued command out of steerMessage', async () => {
      // STEER is the default interrupt mode. Routing a slash command there
      // would deliver "/model" to the model as prose instead of running it.
      const store = createBusyStore();
      const mockSteer = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteer;
      store.setState({ activeInterruptMode: 'steer' });

      await store.getState().handleUserInput('/model');

      expect(mockSteer).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual(['/model']);
    });

    it('still steers a plain chat message typed mid-turn', async () => {
      // The policy must not capture ordinary prompts — steering is unchanged.
      const store = createBusyStore();
      const mockSteer = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteer;
      store.setState({ activeInterruptMode: 'steer' });

      await store.getState().handleUserInput('also check the tests');

      expect(mockSteer).toHaveBeenCalledWith(
        'test-session',
        'also check the tests'
      );
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('queues conversation-state commands', async () => {
      const store = createBusyStore();

      await store.getState().handleUserInput('/compact');

      expect(store.getState().queuedMessages).toEqual(['/compact']);
    });

    it('preserves submit order across queued commands', async () => {
      const store = createBusyStore();

      await store.getState().handleUserInput('/model');
      await store.getState().handleUserInput('/compact');

      expect(store.getState().queuedMessages).toEqual(['/model', '/compact']);
    });

    it('clears the input row after queueing', async () => {
      // Without both clears the visible row keeps the queued text and the next
      // keystroke appends to it ("/model/model").
      const store = createBusyStore();
      store.setState({ commandInputValue: '/model' });

      await store.getState().handleUserInput('/model');

      expect(store.getState().commandInputValue).toBe('');
    });

    it('queues a pre-init slash command instead of buffering it as steer', () => {
      // Pre-init input normally buffers on pendingSteerContent, which replays
      // as a fresh sendMessage — that would deliver "/model" to the model as a
      // prompt. Commands take the queue, which dispatches them instead.
      const store = createBusyStore();
      store.setState({
        isInitialized: false,
        sessionId: null,
        isProcessing: false,
      });

      store.getState().queueMessage('/model');

      expect(store.getState().queuedMessages).toEqual(['/model']);
      expect(store.getState().pendingSteerContent).toBeNull();
    });

    it('still buffers pre-init plain text as steer', () => {
      // The pre-init path is unchanged for ordinary prompts.
      const store = createBusyStore();
      store.setState({
        isInitialized: false,
        sessionId: null,
        isProcessing: false,
      });

      store.getState().queueMessage('hello there');

      expect(store.getState().pendingSteerContent).toBe('hello there');
      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('client-side commands', () => {
    it.each([
      '/settings',
      '/settings display',
      '/settings terminal:newlines',
      '/settings badsub',
    ])('queues %s until the turn ends', async (command) => {
      const store = createBusyStore();
      const dispatch = mock(() => Promise.resolve());
      store.setState({ dispatchSlashCommand: dispatch as never });

      await store.getState().handleUserInput(command);

      expect(dispatch).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual([command]);
      expect(store.getState().isProcessing).toBe(true);
    });

    it('runs an inert prefix abbreviation immediately', async () => {
      const store = createBusyStore();
      const dispatch = mock(() => Promise.resolve());
      store.setState({ dispatchSlashCommand: dispatch as never });

      await store.getState().handleUserInput('/hel');

      expect(dispatch).toHaveBeenCalledWith('/hel');
      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().transientAlert).toBeNull();
    });

    it('classifies a colliding prefix with KAS dispatch precedence', async () => {
      const store = createBusyStore();
      const dispatch = mock(() => Promise.resolve());
      store.setState({
        agentEngine: 'kas',
        kasCommands: [...getKasCommands()],
        dispatchSlashCommand: dispatch as never,
      });

      await store.getState().handleUserInput('/ch new');

      expect(dispatch).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual(['/ch new']);
    });

    it('reports an unknown settings subcommand only after drain', async () => {
      const store = createBusyStore();
      store.setState({ uiMode: 'lite' });

      await store.getState().handleUserInput('/settings badsub');

      expect(store.getState().queuedMessages).toEqual(['/settings badsub']);
      expect(store.getState().transientAlert).toBeNull();
      expect(
        store
          .getState()
          .messages.some((message) =>
            message.content.includes('Unknown settings subcommand: badsub')
          )
      ).toBe(false);

      store.setState({ isProcessing: false });
      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
      const errorRow = store
        .getState()
        .messages.filter((message) => message.role === MessageRole.System)
        .at(-1);
      expect(errorRow?.content).toContain(
        'Unknown settings subcommand: badsub'
      );
      expect(errorRow?.success).toBe(false);
    });

    it('keeps the swap prefix when an agent selection queues mid-turn', async () => {
      const store = createBusyStore();
      const executeCommand = mock(() =>
        Promise.resolve({ success: true, message: '' })
      );
      (store.getState().kiro as any).executeCommand = executeCommand;
      store.setState({
        activeCommand: {
          command: {
            name: '/agent',
            description: 'Switch agent',
            meta: { inputType: 'selection' },
          },
          options: [],
        },
      });

      await store.getState().executeCommandWithArg('coder');

      expect(executeCommand).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual(['/agent swap coder']);
    });

    it('queues an inert command while another command interaction is open', async () => {
      const store = createBusyStore();
      const dispatch = mock(() => Promise.resolve());
      store.setState({
        showSettingsPanel: true,
        dispatchSlashCommand: dispatch as never,
      });

      await store.getState().handleUserInput('/help');

      expect(dispatch).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual(['/help']);
    });

    it('runs inert /usage with a turn-affecting command already queued', async () => {
      const store = createBusyStore();
      let completeUsage!: () => void;
      const dispatch = mock(async () => {
        await new Promise<void>((resolve) => {
          completeUsage = resolve;
        });
        store.setState({ showUsagePanel: true });
      });
      store.setState({ dispatchSlashCommand: dispatch as never });

      await store.getState().handleUserInput('/model');
      const usageDispatch = store.getState().handleUserInput('/usage');

      expect(dispatch).toHaveBeenCalledWith('/usage');
      expect(store.getState().queuedMessages).toEqual(['/model']);
      expect(store.getState().isProcessing).toBe(true);

      store.setState({ isProcessing: false });
      await store.getState().processQueue();
      expect(store.getState().queuedMessages).toEqual(['/model']);

      completeUsage();
      await usageDispatch;
      expect(store.getState().showUsagePanel).toBe(true);
      expect(store.getState().queuedMessages).toEqual(['/model']);
    });

    it('resumes queued work after an inert dispatch outlasts the turn', async () => {
      const store = createBusyStore();
      let completeUsage!: () => void;
      const usagePending = new Promise<void>((resolve) => {
        completeUsage = resolve;
      });
      const dispatch = mock((command: string) =>
        command === '/usage' ? usagePending : Promise.resolve()
      );
      store.setState({ dispatchSlashCommand: dispatch as never });

      await store.getState().handleUserInput('/model');
      const usageDispatch = store.getState().handleUserInput('/usage');
      store.setState({ isProcessing: false });
      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual(['/model']);

      completeUsage();
      await usageDispatch;

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('keeps an inert command queued while another command interaction is open', async () => {
      const store = createBusyStore();
      const dispatch = mock(() => Promise.resolve());
      store.setState({ dispatchSlashCommand: dispatch as never });

      await store.getState().handleUserInput('/model');
      store.setState({ showUsagePanel: true });
      await store.getState().handleUserInput('/help');

      expect(dispatch).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual(['/model', '/help']);
    });

    it('queues an inert command while a selected command is dispatching', async () => {
      const store = createBusyStore();
      let completeContext!: () => void;
      const contextPending = new Promise<void>((resolve) => {
        completeContext = resolve;
      });
      const executeCommand = mock(async () => {
        await contextPending;
        return { success: true, message: '' };
      });
      const dispatch = mock(() => Promise.resolve());
      const backendContextCommand = {
        name: '/context',
        description: 'Context',
        source: 'backend' as const,
      };
      (store.getState().kiro as any).executeCommand = executeCommand;
      store.setState({
        activeCommand: {
          command: backendContextCommand,
          options: [],
        },
        dispatchSlashCommand: dispatch as never,
      });

      const contextDispatch = store.getState().executeCommandWithArg('show');
      expect(executeCommand).toHaveBeenCalled();

      await store.getState().handleUserInput('/usage');

      expect(dispatch).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual(['/usage']);

      completeContext();
      await contextDispatch;
    });

    it('leaves the turn running when a command is queued', async () => {
      const store = createBusyStore();

      await store.getState().handleUserInput('/model');

      expect(store.getState().queuedMessages).toEqual(['/model']);
      expect(store.getState().isProcessing).toBe(true);
    });

    it('leaves the turn running when settings queues mid-turn', async () => {
      const store = createBusyStore();

      await store.getState().handleUserInput('/settings');

      expect(store.getState().queuedMessages).toEqual(['/settings']);
      expect(store.getState().isProcessing).toBe(true);
    });

    it('does not hit the backend when a turn-affecting command queues', async () => {
      // The RPC happens on drain, not on submit.
      const store = createBusyStore();
      const mockExecute = mock(() =>
        Promise.resolve({ success: true, message: '' })
      );
      (store.getState().kiro as any).executeCommand = mockExecute;

      await store.getState().handleUserInput('/model');

      expect(mockExecute).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual(['/model']);
    });

    it('matches lite for the same input', async () => {
      const tui = createBusyStore();
      const lite = createBusyStore();
      lite.setState({ uiMode: 'lite' });

      await tui.getState().handleUserInput('/settings');
      await lite.getState().handleUserInput('/settings');

      expect(tui.getState().queuedMessages).toEqual(
        lite.getState().queuedMessages
      );
    });

    it('holds /goal with a description but runs the bare panel', async () => {
      // A dispatched goal is requeued by the busy guard, so queue state alone is ambiguous.
      const withDescription = createBusyStore();
      const heldDispatch = mock(() => Promise.resolve());
      withDescription.setState({ dispatchSlashCommand: heldDispatch as never });

      await withDescription.getState().handleUserInput('/goal fix all tests');

      expect(heldDispatch).not.toHaveBeenCalled();
      expect(withDescription.getState().queuedMessages).toEqual([
        '/goal fix all tests',
      ]);

      const bare = createBusyStore();
      const bareDispatch = mock(() => Promise.resolve());
      bare.setState({ dispatchSlashCommand: bareDispatch as never });

      await bare.getState().handleUserInput('/goal');

      expect(bareDispatch).toHaveBeenCalledWith('/goal');
      expect(bare.getState().queuedMessages).toEqual([]);
    });

    it('holds mutating subcommands of inert panels', async () => {
      for (const input of [
        '/context clear',
        '/mcp auth server',
        '/tools trust-all',
        '/knowledge update docs',
      ]) {
        const store = createBusyStore();
        const dispatch = mock(() => Promise.resolve());
        store.setState({ dispatchSlashCommand: dispatch as never });

        await store.getState().handleUserInput(input);

        expect(dispatch, input).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages, input).toEqual([input]);
      }
    });

    it('runs explicit read-only panel subcommands', async () => {
      for (const input of ['/context show', '/mcp list', '/knowledge show']) {
        const store = createBusyStore();
        const dispatch = mock(() => Promise.resolve());
        store.setState({ dispatchSlashCommand: dispatch as never });

        await store.getState().handleUserInput(input);

        expect(dispatch, input).toHaveBeenCalledWith(input);
        expect(store.getState().queuedMessages, input).toEqual([]);
      }
    });

    it('holds a command that would seize the terminal mid-turn', async () => {
      const store = createBusyStore();

      await store.getState().handleUserInput('/transcript');

      expect(store.getState().queuedMessages).toEqual(['/transcript']);
    });

    it('holds /copy and mutating or invalid /stats forms', async () => {
      for (const input of [
        '/copy',
        '/stats save stats.json',
        '/stats unknown',
      ]) {
        const store = createBusyStore();
        const dispatch = mock(() => Promise.resolve());
        store.setState({ dispatchSlashCommand: dispatch as never });

        await store.getState().handleUserInput(input);

        expect(dispatch, input).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages, input).toEqual([input]);
      }
    });

    it('runs read-only /stats forms immediately', async () => {
      for (const input of ['/stats', '/stats 10']) {
        const store = createBusyStore();
        const dispatch = mock(() => Promise.resolve());
        store.setState({ dispatchSlashCommand: dispatch as never });

        await store.getState().handleUserInput(input);

        expect(dispatch, input).toHaveBeenCalledWith(input);
        expect(store.getState().queuedMessages, input).toEqual([]);
      }
    });
  });

  describe('rejects unknown tokens', () => {
    it('queues a known command that used to be refused', async () => {
      const store = createBusyStore();

      await store.getState().handleUserInput('/plan');

      expect(store.getState().queuedMessages).toEqual(['/plan']);
      expect(store.getState().transientAlert).toBeNull();
    });

    it('forwards a pasted path as a message rather than refusing it', async () => {
      const store = createBusyStore();
      const mockSteer = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteer;

      await store.getState().handleUserInput('/some/file/path');

      expect(mockSteer).toHaveBeenCalledWith('test-session', '/some/file/path');
      expect(store.getState().transientAlert).toBeNull();
    });

    it('refuses a bare unknown command name', async () => {
      const store = createBusyStore();
      const mockSteer = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteer;

      await store.getState().handleUserInput('/foozle');

      expect(mockSteer).not.toHaveBeenCalled();
      expect(store.getState().transientAlert?.message).toContain(
        'Unrecognized command: /foozle'
      );
    });

    it('refuses a bare unknown command name in a cloud session', async () => {
      const store = createBusyStore();
      const mockSteer = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteer;
      store.setState({ cloudSessionActive: true });

      await store.getState().handleUserInput('/foozle');

      expect(mockSteer).not.toHaveBeenCalled();
      expect(store.getState().transientAlert?.message).toContain(
        'Unrecognized command: /foozle'
      );
    });

    it.each(['tui', 'lite'] as const)(
      'steers slash-prefixed prose during a turn in %s mode',
      async (uiMode) => {
        const store = createBusyStore();
        const mockSteer = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteer;
        store.setState({ uiMode });

        await store.getState().handleUserInput('/summarize the diff below');

        expect(mockSteer).toHaveBeenCalledWith(
          'test-session',
          '/summarize the diff below'
        );
        expect(store.getState().transientAlert).toBeNull();
      }
    );

    it.each([
      ['tui', 'summarize the diff below'],
      ['lite', '/summarize the diff below'],
    ] as const)(
      'sends slash-prefixed prose while idle in %s mode',
      async (uiMode, expectedContent) => {
        const store = createBusyStore();
        const sendMessage = mock(() => Promise.resolve());
        store.setState({
          uiMode,
          isProcessing: false,
          sendMessage: sendMessage as never,
        });

        await store.getState().handleUserInput('/summarize the diff below');

        expect(sendMessage).toHaveBeenCalledWith(
          expectedContent,
          undefined,
          '/summarize the diff below'
        );
        expect(store.getState().transientAlert).toBeNull();
      }
    );

    it('sends double-slash prose to the agent', async () => {
      const store = createBusyStore();
      const mockSteer = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteer;

      await store.getState().handleUserInput('// hello world');

      expect(mockSteer).toHaveBeenCalledWith('test-session', '// hello world');
      expect(store.getState().transientAlert).toBeNull();
    });
  });

  describe('turn-end drain', () => {
    it('dispatches a queued /model when the turn ends', async () => {
      const store = createBusyStore();
      await store.getState().handleUserInput('/model');
      expect(store.getState().queuedMessages).toEqual(['/model']);

      // Turn ends: the drain runs the command rather than sending it as chat.
      const mockStream = mock(() => Promise.resolve());
      (store.getState().kiro as any).streamMessage = mockStream;
      store.setState({ isProcessing: false });
      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
      expect(mockStream).not.toHaveBeenCalled();
      // The dim [queue] row is the record that the held command fired.
      const systemRows = store
        .getState()
        .messages.filter((m) => m.role === MessageRole.System);
      expect(systemRows.some((m) => m.content.includes('/model'))).toBe(true);
    });

    it('drains queued commands in FIFO order', async () => {
      const store = createBusyStore();
      await store.getState().handleUserInput('/model');
      await store.getState().handleUserInput('/compact');

      store.setState({ isProcessing: false });
      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
      const rows = store
        .getState()
        .messages.filter((m) => m.role === MessageRole.System)
        .map((m) => m.content);
      const modelAt = rows.findIndex((c) => c.includes('/model'));
      const compactAt = rows.findIndex((c) => c.includes('/compact'));
      expect(modelAt).toBeGreaterThanOrEqual(0);
      expect(compactAt).toBeGreaterThan(modelAt);
    });

    it('does not drain while the turn is still in flight', async () => {
      const store = createBusyStore();
      await store.getState().handleUserInput('/model');

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual(['/model']);
    });
  });

  describe('queued unknown commands', () => {
    it('sends pre-init slash-prefixed prose on startup drain', async () => {
      const store = createBusyStore();
      store.setState({
        isInitialized: false,
        sessionId: null,
        isProcessing: false,
      });

      await store
        .getState()
        .handleUserInput('/definitely-not-a-command-for-preinit deploy');
      expect(store.getState().queuedMessages).toEqual([
        '/definitely-not-a-command-for-preinit deploy',
      ]);

      const sendMessage = mock(() => Promise.resolve());
      store.setState({
        isInitialized: true,
        sessionId: 'test-session',
        sendMessage: sendMessage as never,
      });
      await store.getState().processQueue();

      expect(sendMessage).toHaveBeenCalledWith(
        '/definitely-not-a-command-for-preinit deploy',
        undefined,
        '/definitely-not-a-command-for-preinit deploy'
      );
      expect(store.getState().transientAlert).toBeNull();
    });

    it('refuses a never-known token admitted through the queue API', async () => {
      const store = createBusyStore();
      store.setState({ uiMode: 'lite', isProcessing: false });
      store.getState().queueMessage('/foozle');
      expect(store.getState().queuedMessages).toEqual(['/foozle']);

      const sendMessage = mock(() => Promise.resolve());
      store.setState({ sendMessage: sendMessage as never });

      await store.getState().processQueue();

      expect(sendMessage).not.toHaveBeenCalled();
      expect(store.getState().transientAlert?.message).toContain(
        'Unrecognized command: /foozle'
      );
    });

    it('refuses an edited unknown command and keeps draining', async () => {
      const store = createBusyStore();
      store.setState({
        uiMode: 'lite',
        isProcessing: false,
        activeInterruptMode: 'queue',
      });
      store.getState().queueMessage('edit me');
      store.getState().queueMessage('send me next');
      store.getState().replaceQueuedMessage(0, '/foozle');
      const sendMessage = mock(() => Promise.resolve());
      store.setState({ sendMessage: sendMessage as never });

      await store.getState().processQueue();

      expect(sendMessage).toHaveBeenCalledTimes(1);
      expect(sendMessage).toHaveBeenCalledWith(
        'send me next',
        undefined,
        'send me next'
      );
      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().transientAlert?.message).toContain(
        'Unrecognized command: /foozle'
      );
    });
  });
});
