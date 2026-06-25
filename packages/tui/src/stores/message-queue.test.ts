import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore, MessageRole } from './app-store';
import { Kiro } from '../kiro';
import { AgentEventType } from '../types/agent-events';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    sendMessage: mock(),
    steerMessage: mock(),
    clearSteering: mock(),
    cancel: mock(),
    close: mock(),
    // Slash-command dispatch emits frontend command-usage telemetry via
    // `ctx.kiro.sendChatSlashCommandTelemetry` (added on main). The drain-row
    // tests dispatch real slash commands through processQueue, so the mock
    // must stub it or the dispatcher throws "not a function".
    sendChatSlashCommandTelemetry: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

function createTestStore() {
  const mockKiro = new Kiro();
  const store = createAppStore({ kiro: mockKiro });
  // Register the slash commands these tests dispatch through processQueue's
  // known-slash branch. Without this, isKnownSlashCommandToken returns false
  // and `/help` / `/model` / `/verbosity` fall through to sendMessage as chat
  // messages — bypassing the [queue] drain row, picker-restore snapshot, and
  // mid-typed input preservation that this file pins down.
  const existing = store.getState().slashCommands;
  store.setState({
    isInitialized: true,
    slashCommands: [
      ...existing,
      { name: '/help', description: 'Show help', source: 'local' as const },
      { name: '/model', description: 'Switch model', source: 'local' as const },
      {
        name: '/verbosity',
        description: 'Verbosity',
        source: 'local' as const,
      },
    ],
  });
  return store;
}

/**
 * Boot a store in KAS mode. `createAppStore` seeds `kasCommands` from
 * KAS_COMMANDS when agentEngine === 'kas' (app-store.ts), so KAS-only
 * commands like `/rewind` live in the `kasCommands` slice and are NOT
 * mirrored into `slashCommands`. These tests pin that the lite submit/queue
 * gates recognize those commands via the merged `liteGateCommands` list
 * rather than leaking them to the model as chat text.
 */
function createKasTestStore() {
  const mockKiro = new Kiro();
  const store = createAppStore({ kiro: mockKiro, agentEngine: 'kas' });
  store.setState({ isInitialized: true });
  return store;
}

describe('Message queue (backend-driven)', () => {
  describe('queueMessage', () => {
    it('calls kiro.steerMessage with sessionId and trimmed content', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        sessionId: 'session-abc',
        activeInterruptMode: 'steer',
      });

      store.getState().queueMessage('  hello world  ');

      expect(mockSteerMessage).toHaveBeenCalledWith(
        'session-abc',
        'hello world'
      );
    });

    it('rejects empty string', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ sessionId: 'session-abc' });

      store.getState().queueMessage('');

      expect(mockSteerMessage).not.toHaveBeenCalled();
    });

    it('rejects whitespace-only string', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ sessionId: 'session-abc' });

      store.getState().queueMessage('   ');
      store.getState().queueMessage('\t\n');

      expect(mockSteerMessage).not.toHaveBeenCalled();
    });

    it('buffers onto pendingSteerContent when sessionId is null (does not drop)', () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ sessionId: null, isInitialized: false });

      store.getState().queueMessage('hello');

      expect(mockSteerMessage).not.toHaveBeenCalled();
      expect(store.getState().pendingSteerContent).toBe('hello');
    });

    describe('mode-aware routing', () => {
      it('routes to steerMessage in steering mode', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'steer',
        });

        store.getState().queueMessage('steer this');

        expect(mockSteerMessage).toHaveBeenCalledWith(
          'session-abc',
          'steer this'
        );
        expect(store.getState().queuedMessages).toEqual([]);
      });

      it('appends to queuedMessages in queueing mode', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('queue this');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages).toEqual(['queue this']);
      });

      it('appends multiple messages to queuedMessages in order (queueing mode)', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('first');
        store.getState().queueMessage('second');
        store.getState().queueMessage('third');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages).toEqual([
          'first',
          'second',
          'third',
        ]);
      });

      it('trims whitespace before appending in queueing mode', () => {
        const store = createTestStore();
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('  padded  ');

        expect(store.getState().queuedMessages).toEqual(['padded']);
      });

      it('rejects empty/whitespace in queueing mode without modifying buffer', () => {
        const store = createTestStore();
        store.setState({
          sessionId: 'session-abc',
          activeInterruptMode: 'queue',
          queuedMessages: ['existing'],
        });

        store.getState().queueMessage('');
        store.getState().queueMessage('   ');
        store.getState().queueMessage('\t\n');

        expect(store.getState().queuedMessages).toEqual(['existing']);
      });

      it('buffers to pendingSteerContent pre-init regardless of mode (steering)', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: null,
          isInitialized: false,
          activeInterruptMode: 'steer',
        });

        store.getState().queueMessage('pre-init msg');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().pendingSteerContent).toBe('pre-init msg');
        expect(store.getState().queuedMessages).toEqual([]);
      });

      it('buffers to pendingSteerContent pre-init regardless of mode (queuing)', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          sessionId: null,
          isInitialized: false,
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('pre-init msg');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().pendingSteerContent).toBe('pre-init msg');
        expect(store.getState().queuedMessages).toEqual([]);
      });

      it('concatenates pre-init buffers with double newline', () => {
        const store = createTestStore();
        store.setState({
          sessionId: null,
          isInitialized: false,
          activeInterruptMode: 'queue',
        });

        store.getState().queueMessage('first');
        store.getState().queueMessage('second');

        expect(store.getState().pendingSteerContent).toBe('first\n\nsecond');
      });
    });

    describe('lite mode steering', () => {
      it('routes a mid-turn chat message to steerMessage in lite (matches TUI)', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          uiMode: 'lite',
          sessionId: 'session-abc',
          isProcessing: true,
          activeInterruptMode: 'steer',
        });

        store.getState().queueMessage('steer this mid-turn');

        expect(mockSteerMessage).toHaveBeenCalledWith(
          'session-abc',
          'steer this mid-turn'
        );
        // CRITICAL no-double-send guard: the steer is sent via the backend
        // transport ONLY. It must NOT also land in queuedMessages — if it did,
        // processQueue would drain it as a second send on top of the backend's
        // own injection. The unified preview list surfaces it from
        // pendingSteerContent (set by the SteeringQueued echo), not from here.
        expect(store.getState().queuedMessages).toEqual([]);
      });

      it('keeps a chat message local during a lite loading window (no active turn)', () => {
        // Regression guard: a chat message typed during a loadingMessage
        // window (/agent swap, /chat resume) has isProcessing=false and must
        // stay in the local queue — routing it to steerMessage would target a
        // stale/absent session and silently lose it.
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          uiMode: 'lite',
          sessionId: 'session-abc',
          isProcessing: false,
          loadingMessage: 'Loading session…',
          activeInterruptMode: 'steer',
        });

        store.getState().queueMessage('a chat message');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages).toEqual(['a chat message']);
      });

      it('keeps a known slash command local even mid-turn in lite', () => {
        const store = createTestStore();
        const mockSteerMessage = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteerMessage;
        store.setState({
          uiMode: 'lite',
          sessionId: 'session-abc',
          isProcessing: true,
          activeInterruptMode: 'steer',
        });

        store.getState().queueMessage('/verbosity');

        expect(mockSteerMessage).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages).toEqual(['/verbosity']);
      });
    });

    describe('replaceSteerMessage (clear-and-resteer)', () => {
      it('clears then resteers the edited text on a session-live queue', async () => {
        const store = createTestStore();
        const calls: string[] = [];
        const mockClear = mock(() => {
          calls.push('clear');
          return Promise.resolve();
        });
        const mockSteer = mock((_sid: string, content: string) => {
          calls.push(`steer:${content}`);
          return Promise.resolve();
        });
        (store.getState().kiro as any).clearSteering = mockClear;
        (store.getState().kiro as any).steerMessage = mockSteer;
        store.setState({
          sessionId: 'session-abc',
          isInitialized: true,
          pendingSteerContent: 'original steer',
        });

        store.getState().replaceSteerMessage('edited steer');

        // Optimistic local update lands immediately.
        expect(store.getState().pendingSteerContent).toBe('edited steer');
        // Edited steer is NEVER copied into queuedMessages (no double-send).
        expect(store.getState().queuedMessages).toEqual([]);

        // Let the chained promise (.then) resolve.
        await Promise.resolve();
        await Promise.resolve();

        // clear must precede the resteer so the backend ends with ONLY the
        // edited content rather than concatenating onto the old steer.
        expect(mockClear).toHaveBeenCalledWith('session-abc');
        expect(mockSteer).toHaveBeenCalledWith('session-abc', 'edited steer');
        expect(calls).toEqual(['clear', 'steer:edited steer']);
      });

      it('replaces in place pre-init without touching the backend', () => {
        const store = createTestStore();
        const mockClear = mock(() => Promise.resolve());
        const mockSteer = mock(() => Promise.resolve());
        (store.getState().kiro as any).clearSteering = mockClear;
        (store.getState().kiro as any).steerMessage = mockSteer;
        store.setState({
          sessionId: null,
          isInitialized: false,
          pendingSteerContent: 'pre-init original',
        });

        store.getState().replaceSteerMessage('pre-init edited');

        expect(store.getState().pendingSteerContent).toBe('pre-init edited');
        expect(store.getState().queuedMessages).toEqual([]);
        expect(mockClear).not.toHaveBeenCalled();
        expect(mockSteer).not.toHaveBeenCalled();
      });

      it('delegates an emptied edit to clearSteerMessage (discard)', () => {
        const store = createTestStore();
        const mockClear = mock(() => Promise.resolve());
        (store.getState().kiro as any).clearSteering = mockClear;
        store.setState({
          sessionId: 'session-abc',
          isInitialized: true,
          pendingSteerContent: 'to discard',
        });

        store.getState().replaceSteerMessage('   ');

        expect(store.getState().pendingSteerContent).toBeNull();
        expect(mockClear).toHaveBeenCalledWith('session-abc');
        expect(store.getState().queuedMessages).toEqual([]);
      });

      it('no-ops when nothing is staged', () => {
        const store = createTestStore();
        const mockSteer = mock(() => Promise.resolve());
        (store.getState().kiro as any).steerMessage = mockSteer;
        store.setState({
          sessionId: 'session-abc',
          isInitialized: true,
          pendingSteerContent: null,
        });

        store.getState().replaceSteerMessage('orphan edit');

        expect(store.getState().pendingSteerContent).toBeNull();
        expect(mockSteer).not.toHaveBeenCalled();
        expect(store.getState().queuedMessages).toEqual([]);
      });
    });

    it('allows queuing the same slash command twice (no dedup)', () => {
      // Re-queuing a slash command is a valid action — the user may want
      // to re-run it (e.g. re-open a picker). Duplicates are kept in FIFO
      // order rather than rejected.
      const store = createTestStore();
      store.setState({
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });
      store.getState().queueMessage('/model');
      store.getState().queueMessage('/model');
      expect(store.getState().queuedMessages).toEqual(['/model', '/model']);
      // No rejection alert — the queue strip is the only feedback surface.
      expect(store.getState().transientAlert).toBeNull();
    });

    it('keeps distinct argv slash commands in order', () => {
      const store = createTestStore();
      store.setState({
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });
      store.getState().queueMessage('/model gpt-4');
      store.getState().queueMessage('/model claude');
      expect(store.getState().queuedMessages).toEqual([
        '/model gpt-4',
        '/model claude',
      ]);
    });

    it('allows chat messages to repeat (legitimate workflow)', () => {
      // Asking the agent the same question twice is a real workflow —
      // sometimes you want the same prompt run against a now-different
      // codebase state.
      const store = createTestStore();
      store.setState({
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });
      store.getState().queueMessage('fix the bug');
      store.getState().queueMessage('fix the bug');
      expect(store.getState().queuedMessages).toEqual([
        'fix the bug',
        'fix the bug',
      ]);
    });
  });

  describe('pendingSteerContent state (notification-driven)', () => {
    it('starts as null', () => {
      const store = createTestStore();
      expect(store.getState().pendingSteerContent).toBeNull();
    });

    it('is set by SteeringQueued event', () => {
      const store = createTestStore();
      store.setState({ pendingSteerContent: 'fix the bug' });
      expect(store.getState().pendingSteerContent).toBe('fix the bug');
    });

    it('is cleared by SteeringConsumed (set to null)', () => {
      const store = createTestStore();
      store.setState({ pendingSteerContent: 'fix the bug' });
      store.setState({ pendingSteerContent: null });
      expect(store.getState().pendingSteerContent).toBeNull();
    });
  });

  describe('handleUserInput queuing', () => {
    it('queues message when isProcessing is true', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'steer',
      });

      await store.getState().handleUserInput('queued message');

      expect(mockSteerMessage).toHaveBeenCalledWith(
        'session-abc',
        'queued message'
      );
    });

    it('clears input buffer after queuing', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ isProcessing: true, sessionId: 'session-abc' });

      await store.getState().handleUserInput('queued message');

      const input = store.getState().input;
      expect(input.lines).toEqual(['']);
      expect(input.cursorCol).toBe(0);
    });

    it('does not queue empty/whitespace input during processing', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ isProcessing: true, sessionId: 'session-abc' });

      await store.getState().handleUserInput('   ');

      expect(mockSteerMessage).not.toHaveBeenCalled();
    });

    it('rejects slash commands with a warning when processing', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ isProcessing: true, sessionId: 'session-abc' });

      await store.getState().handleUserInput('/help');

      // Slash command should NOT be queued
      expect(mockSteerMessage).not.toHaveBeenCalled();
      // A transient alert should be shown
      expect(store.getState().transientAlert).not.toBeNull();
      expect(store.getState().transientAlert?.status).toBe('warning');
    });

    it('rejects shell escape commands with a warning when processing', async () => {
      const store = createTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({ isProcessing: true, sessionId: 'session-abc' });

      await store.getState().handleUserInput('!ls');

      expect(mockSteerMessage).not.toHaveBeenCalled();
      expect(store.getState().transientAlert).not.toBeNull();
      expect(store.getState().transientAlert?.status).toBe('warning');
    });
  });

  describe('unified expanded state', () => {
    it('toggleToolOutputsExpanded toggles the shared expanded state', () => {
      const store = createTestStore();
      expect(store.getState().toolOutputsExpanded).toBe(false);
      store.getState().toggleToolOutputsExpanded();
      expect(store.getState().toolOutputsExpanded).toBe(true);
      store.getState().toggleToolOutputsExpanded();
      expect(store.getState().toolOutputsExpanded).toBe(false);
    });
  });
});

describe('Queueing mode behaviors', () => {
  describe('clearQueue', () => {
    it('is a no-op on empty queue', () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'queue' });
      store.getState().clearQueue();
      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('processQueue', () => {
    it('does not clear input buffer when processing queue', async () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'queue',
        sessionId: 'session-abc',
      });
      // Simulate user typing while queue processes
      const typedInput = store.getState().input;
      store.setState({
        queuedMessages: ['queued msg'],
        input: { ...typedInput, lines: ['user is typing'], cursorCol: 14 },
      });

      await store.getState().processQueue();

      // Input should be preserved — processQueue calls sendMessage directly
      const input = store.getState().input;
      expect(input.lines).toEqual(['user is typing']);
    });

    it('preserves mid-typed input across a queued slash-command drain', async () => {
      // Repro for P438912313's clobber half: when a queued slash command
      // drains via processQueue → handleUserInput, the dispatcher's
      // `commandInputValue: ''` reset would otherwise wipe whatever the
      // user has been typing in the input area while the prior turn was
      // running. (commandInputValue is what PromptInput's syncToStore
      // writes to on every keystroke — it's the field that actually
      // drives the visible input row in lite.) The snapshot/restore in
      // processQueue saves the buffer pre-dispatch and restores it after,
      // so the user's work-in-progress survives.
      const store = createTestStore();
      const init = store.getState().input;
      // Mid-typed extras BEYOND the queued command itself — the dedup-
      // aware restore only fires when the snapshot differs from the
      // queued message, so a snapshot of just '/help' would NOT be
      // restored (we don't want to put the dispatched command back in
      // the input). The realistic scenario the bug describes is a user
      // who queued '/help', then started composing a follow-up prompt
      // while the prior turn ran.
      store.setState({
        queuedMessages: ['/help'],
        commandInputValue: '/help notes I started writing while waiting',
        input: {
          ...init,
          lines: ['/help notes I started writing while waiting'],
          cursorCol: 43,
        },
      });

      await store.getState().processQueue();

      // commandInputValue is the user-visible field; verify it survived.
      expect(store.getState().commandInputValue).toBe(
        '/help notes I started writing while waiting'
      );
      // state.input is the lower-level buffer; same snapshot is restored
      // there so any future code path or test reading from it sees the
      // consistent state.
      const input = store.getState().input;
      expect(input.lines).toEqual([
        '/help notes I started writing while waiting',
      ]);
      expect(input.cursorCol).toBe(43);
    });

    it('does not put the queued command back when no extra typing occurred', async () => {
      // The dedup-aware restore: if the user typed exactly the queued
      // command and nothing else (the common case — they queued `/model`
      // and immediately stopped typing), DO NOT restore the snapshot.
      // Restoring would put `/model` back in the cleared input, defeating
      // the dispatch's clearing and leaving the user confused about
      // whether the queue actually drained. The dispatcher's
      // commandInputValue: '' reset stands in this case.
      const store = createTestStore();
      store.setState({
        queuedMessages: ['/help'],
        commandInputValue: '/help',
      });

      await store.getState().processQueue();

      // Empty after drain — the dispatcher's clear stands.
      expect(store.getState().commandInputValue).toBe('');
    });

    it('leaves input alone when no mid-typed content during slash-command drain', async () => {
      // Empty-snapshot path: the user hadn't typed anything, the queue
      // drains, the dispatcher's clear leaves the buffer empty, and our
      // restore is skipped (snapshot would equal an empty string, not
      // differing from the queued message either).
      const store = createTestStore();
      store.setState({ queuedMessages: ['/help'] });

      await store.getState().processQueue();

      expect(store.getState().commandInputValue).toBe('');
      const input = store.getState().input;
      expect(input.lines).toEqual(['']);
      expect(input.cursorCol).toBe(0);
    });

    it('does not clobber typing that lands during async slash dispatch', async () => {
      // The dispatch is async (handleUserInput awaits the command's
      // executor — for RPC-bound commands like /agent or /model, that
      // await can take seconds). If the user types into the cleared
      // buffer during the await, those keystrokes are MORE RECENT than
      // our pre-dispatch snapshot. Blindly restoring the snapshot would
      // overwrite the new typing with stale data — the exact bug this
      // fix exists to prevent, just with our snapshot now playing the
      // dispatcher's clearing role. Pin the race-aware behavior here.
      const store = createTestStore();
      // Pre-dispatch: user had typed an extended message after queueing
      // /help — a snapshot worth caring about.
      store.setState({
        queuedMessages: ['/help'],
        commandInputValue: '/help old draft',
      });

      // Simulate the user typing during the dispatch: monkey-patch
      // handleUserInput to clear commandInputValue (matching its real
      // behavior), then write new content into the buffer before
      // returning. The restore guard should detect the post-dispatch
      // content and bail.
      const originalHandleUserInput = store.getState().handleUserInput;
      store.setState({
        handleUserInput: async (input: string) => {
          // Mirror dispatcher: wipe to empty, then yield to "user typing".
          store.setState({ commandInputValue: '' });
          // Simulated user keystrokes during await.
          store.setState({ commandInputValue: 'typed during swap' });
          // Don't actually invoke the original — we're testing the
          // restore guard, not the real /help dispatch.
          void originalHandleUserInput;
          void input;
        },
      });

      await store.getState().processQueue();

      // The user's mid-dispatch typing wins — snapshot restore is skipped.
      expect(store.getState().commandInputValue).toBe('typed during swap');
    });

    it('defers restore via queuedInputRestore when dispatch opens a picker', async () => {
      // P438912313 follow-up: the inline restore from the prior fix is
      // invisible while a picker is up (PromptInput renders
      // `activeCommand.command.name` instead of segments) and gets
      // wiped the instant the picker dismisses, because both close
      // paths (`handleActiveCommandClose` Esc handler and `onSelect`'s
      // no-hint branch) call `clearCommandInput()`. The user types
      // pending text in the prompt, queues `/model`, the turn ends,
      // the picker opens, the user backs out — and their pending text
      // is gone with no signal.
      //
      // Fix: when the dispatch opens a picker (`activeCommand` non-null
      // after handleUserInput resolves), processQueue stashes the
      // snapshot in `queuedInputRestore` instead of inline restoring.
      // LiteLayout's useLayoutEffect on activeCommand → null then
      // applies the restore via `applyQueuedInputRestore`. Pin the
      // store-level half of the contract here; the layout half is
      // covered by knight-rider visual verification.
      const store = createTestStore();
      const init = store.getState().input;
      store.setState({
        queuedMessages: ['/model'],
        // User had typed pending text completely unrelated to the
        // queued slash command (the realistic scenario — they queued
        // `/model` with no extra args, then started composing a fresh
        // prompt while the prior turn ran).
        commandInputValue: 'pending message',
        input: {
          ...init,
          lines: ['pending message'],
          cursorCol: 15,
        },
      });

      // Simulate /model: a dispatcher that clears commandInputValue
      // (matching handleUserInput's main path) then opens a picker via
      // setActiveCommand. The real dispatcher does this via
      // executeCommand → kiro.getCommandOptions → ctx.setActiveCommand;
      // we shortcut it here so the test doesn't need a real RPC mock.
      store.setState({
        handleUserInput: async () => {
          store.setState({ commandInputValue: '' });
          // Open the picker — this is the trigger for the deferred
          // restore path.
          store.setState({
            activeCommand: {
              command: { name: '/model' } as never,
              options: [],
            },
          });
        },
      });

      await store.getState().processQueue();

      // commandInputValue is left empty (the picker is on top of it;
      // PromptInput renders `/model` from activeCommand.command.name
      // anyway, so commandInputValue's value isn't user-visible right
      // now).
      expect(store.getState().commandInputValue).toBe('');
      // The snapshot lives in queuedInputRestore, waiting for the
      // picker to close.
      expect(store.getState().queuedInputRestore).not.toBeNull();
      expect(store.getState().queuedInputRestore?.commandInputValue).toBe(
        'pending message'
      );
      expect(store.getState().queuedInputRestore?.input.lines).toEqual([
        'pending message',
      ]);
    });

    it('applyQueuedInputRestore restores the snapshot and clears the slot', () => {
      // Pure store action — exercises the consumer half of
      // queuedInputRestore independently of processQueue. Called by
      // LiteLayout's useLayoutEffect when activeCommand transitions
      // non-null → null after a picker close.
      const store = createTestStore();
      const init = store.getState().input;
      store.setState({
        queuedInputRestore: {
          commandInputValue: 'pending message',
          input: {
            ...init,
            lines: ['pending message'],
            cursorCol: 15,
          },
        },
        // Simulate the moment after handleActiveCommandClose has run:
        // activeCommand is null, commandInputValue was clobbered by
        // clearCommandInput.
        commandInputValue: '',
        activeCommand: null,
      });

      store.getState().applyQueuedInputRestore();

      expect(store.getState().commandInputValue).toBe('pending message');
      expect(store.getState().input.lines).toEqual(['pending message']);
      expect(store.getState().input.cursorCol).toBe(15);
      // Slot cleared so the next drain doesn't re-apply a stale snapshot.
      expect(store.getState().queuedInputRestore).toBeNull();
    });

    it('applyQueuedInputRestore is a no-op when no snapshot is stashed', () => {
      // Defensive: the layout effect fires on every activeCommand
      // transition, including ones unrelated to a queue drain (typed
      // /model picker, sub-command navigation). Without a stashed
      // snapshot the action must leave commandInputValue alone — a
      // false reset would clobber whatever the user is currently
      // typing. The early `if (!restore) return;` guard is what makes
      // this safe; this test pins it.
      const store = createTestStore();
      store.setState({ commandInputValue: 'typed by user' });

      store.getState().applyQueuedInputRestore();

      expect(store.getState().commandInputValue).toBe('typed by user');
      expect(store.getState().queuedInputRestore).toBeNull();
    });

    it('inline restore (not deferred) fires when dispatch did NOT open a picker', async () => {
      // Companion to the picker test: non-picker slash commands
      // (/verbosity, /clear, etc.) finish synchronously without setting
      // activeCommand. The user's pending text should be restored
      // inline — same behavior as before the picker fix, no
      // queuedInputRestore involved. Pinning this so a future
      // refactor that removes the inline branch would fail loudly
      // instead of silently breaking the common case.
      const store = createTestStore();
      const init = store.getState().input;
      store.setState({
        queuedMessages: ['/help'],
        commandInputValue: 'pending message',
        input: {
          ...init,
          lines: ['pending message'],
          cursorCol: 15,
        },
      });
      // Simulate a non-picker dispatch — clears commandInputValue but
      // leaves activeCommand null.
      store.setState({
        handleUserInput: async () => {
          store.setState({ commandInputValue: '' });
        },
      });

      await store.getState().processQueue();

      // Inline restore landed; queuedInputRestore stays null.
      expect(store.getState().commandInputValue).toBe('pending message');
      expect(store.getState().queuedInputRestore).toBeNull();
    });

    it('emits a System "[queue] /command" row when draining a slash command', async () => {
      // P438912852: picker-opening commands like /model leave zero
      // scrollback evidence when the user dismisses the picker — the
      // dispatcher only announces (via ctx.announceSystem) when the user
      // actually picks a value. processQueue's drain emits a row up front
      // so scrollback always records that a queued slash command ran,
      // independent of what the dispatcher does next. Pin the row's
      // shape (role + content prefix) so future renderer changes can't
      // silently drop it.
      const store = createTestStore();
      const messagesBefore = store.getState().messages.length;
      store.setState({ queuedMessages: ['/help'] });

      await store.getState().processQueue();

      const messages = store.getState().messages;
      // At least one new message landed.
      expect(messages.length).toBeGreaterThan(messagesBefore);
      // The drain row is the FIRST new entry — it goes in before the
      // dispatcher does any work, so anything the command itself adds
      // (e.g. /verbosity's "set density to minimal" announcement) appears
      // after it. Locking the order here keeps the row functioning as a
      // turn-boundary marker rather than a trailing footnote.
      const drainRow = messages[messagesBefore];
      expect(drainRow?.role).toBe(MessageRole.System);
      // The row is dim-styled, so the raw content includes ANSI escapes
      // around `[queue] /help`. Match on the unstripped substring rather
      // than the exact string so a future style tweak (different chalk
      // call, additional decoration) doesn't break the test.
      expect(drainRow?.content).toContain('[queue] /help');
    });

    it('emits the drain row with full args for argv-style slash commands', async () => {
      // /verbosity density minimal, /chat <id>, /agent <name>: the row
      // should record the full invocation so users can scroll back and
      // see exactly what was applied — not just the bare command name.
      const store = createTestStore();
      const messagesBefore = store.getState().messages.length;
      store.setState({ queuedMessages: ['/verbosity density minimal'] });

      await store.getState().processQueue();

      const drainRow = store.getState().messages[messagesBefore];
      expect(drainRow?.content).toContain('[queue] /verbosity density minimal');
    });

    it('does NOT emit a drain row for chat messages (User row + agent response cover it)', async () => {
      // Chat messages flow through sendMessage which produces a User row
      // (visible in scrollback) and the agent's response. Adding a
      // [queue] row on top would be redundant and noisy. Lock the
      // chat-only branch as drain-row-free.
      const store = createTestStore();
      const messagesBefore = store.getState().messages;
      store.setState({ queuedMessages: ['fix the bug'] });

      await store.getState().processQueue();

      // No System row with our [queue] prefix landed.
      const newSystemRows = store
        .getState()
        .messages.slice(messagesBefore.length)
        .filter(
          (m) =>
            m.role === MessageRole.System &&
            typeof m.content === 'string' &&
            m.content.includes('[queue]')
        );
      expect(newSystemRows).toEqual([]);
    });
  });

  describe('handleUserInput queuing', () => {
    it('queues message when isProcessing is true (queueing mode)', async () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });

      await store.getState().handleUserInput('queued message');

      expect(store.getState().queuedMessages).toEqual(['queued message']);
    });

    it('clears input buffer after queuing (queueing mode)', async () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });
      // Pre-populate commandInputValue to mirror what PromptInput's
      // syncToStore writes on every keystroke. Without seeding this, the
      // initial state happens to be '' and the assertion below passes
      // trivially — but the live bug was specifically that
      // commandInputValue persisted across a queue submit.
      store.setState({ commandInputValue: 'queued message' });

      await store.getState().handleUserInput('queued message');

      const input = store.getState().input;
      expect(input.lines).toEqual(['']);
      expect(input.cursorCol).toBe(0);
      // commandInputValue is the field PromptInput renders from. clearInput
      // alone does NOT touch it — the queueing path has to call
      // clearCommandInput too. Without this assertion the original bug
      // (visible `> /model` left in the input row, second `/model`
      // submission becoming `/model/model`) was invisible to the test
      // suite even though all existing assertions passed. Both queue
      // branches (chat-message here, slash-command in lite mode) follow
      // the same pattern; this test pins the chat side, the slash side
      // is structurally identical.
      expect(store.getState().commandInputValue).toBe('');
    });

    it('rejects slash commands with a warning when not initialized', async () => {
      const store = createTestStore();
      store.setState({ isInitialized: false });

      await store.getState().handleUserInput('/context');

      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().transientAlert).not.toBeNull();
    });

    it('queues regular messages but not slash commands when processing (queueing mode)', async () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });

      await store.getState().handleUserInput('fix the bug');
      await store.getState().handleUserInput('/help');
      await store.getState().handleUserInput('add tests too');

      // Only regular messages should be queued
      expect(store.getState().queuedMessages).toEqual([
        'fix the bug',
        'add tests too',
      ]);
    });

    it('does not queue slash commands when processing (queueing mode)', async () => {
      // We can't fully test /quit since it calls process.exit, but we can
      // verify that slash commands are never added to the queue while the
      // agent is processing — they pass through the slash-command handler.
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
      });

      await store.getState().handleUserInput('/help');
      await store.getState().handleUserInput('/context');
      await store.getState().handleUserInput('/model');

      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('queuing during initialization', () => {
    it('queues message via handleUserInput when not initialized (queueing mode)', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: false,
        activeInterruptMode: 'queue',
      });

      await store.getState().handleUserInput('early message');

      // Pre-init buffers to pendingSteerContent regardless of mode
      expect(store.getState().pendingSteerContent).toBe('early message');
    });

    it('buffers message via sendMessage when not initialized (pre-init path)', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: false,
        activeInterruptMode: 'queue',
      });

      await store.getState().sendMessage('early message');

      // Pre-init sendMessage calls queueMessage which buffers to pendingSteerContent
      expect(store.getState().pendingSteerContent).toBe('early message');
      expect(store.getState().isProcessing).toBe(false);
    });

    it('drains queue after isInitialized becomes true', async () => {
      const store = createTestStore();
      store.setState({
        isInitialized: true,
        activeInterruptMode: 'queue',
        sessionId: 'session-abc',
        queuedMessages: ['queued during init'],
      });

      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('cancellation semantics', () => {
    it('clearQueue + cancelMessage clears queue (Escape behavior)', () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        activeInterruptMode: 'queue',
        queuedMessages: ['msg1', 'msg2', 'msg3'],
      });

      store.getState().clearQueue();
      store.getState().cancelMessage();

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('cancelMessage alone preserves queue (Ctrl+C behavior)', () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
        activeInterruptMode: 'queue',
        queuedMessages: ['msg1', 'msg2', 'msg3'],
      });

      store.getState().cancelMessage();

      expect(store.getState().queuedMessages).toEqual(['msg1', 'msg2', 'msg3']);
    });
  });

  describe('unified expanded state', () => {
    it('expanded state persists across queued turns (not reset by sendMessage)', async () => {
      const store = createTestStore();
      store.setState({
        activeInterruptMode: 'queue',
        sessionId: 'session-abc',
      });
      // User expands outputs
      store.getState().toggleToolOutputsExpanded();
      expect(store.getState().toolOutputsExpanded).toBe(true);

      // Queue a message and process it — sendMessage will be called
      store.setState({ queuedMessages: ['next message'] });
      await store.getState().processQueue();

      // Expanded state should still be true
      expect(store.getState().toolOutputsExpanded).toBe(true);
    });

    it('clearQueue does not affect expanded state', () => {
      const store = createTestStore();
      store.setState({ activeInterruptMode: 'queue' });
      store.getState().toggleToolOutputsExpanded();
      store.setState({ queuedMessages: ['a', 'b'] });

      store.getState().clearQueue();

      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().toolOutputsExpanded).toBe(true);
    });
  });
});

describe('Compaction drains queue', () => {
  it('processQueue is called after compaction completes', async () => {
    const store = createTestStore();
    store.setState({
      isCompacting: true,
      isProcessing: false,
      activeInterruptMode: 'queue',
      sessionId: 'session-abc',
      queuedMessages: ['queued during compaction'],
    });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'completed',
    });

    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('processQueue is called after compaction fails', async () => {
    const store = createTestStore();
    store.setState({
      isCompacting: true,
      isProcessing: false,
      activeInterruptMode: 'queue',
      sessionId: 'session-abc',
      queuedMessages: ['queued during compaction'],
    });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'failed',
      error: 'test error',
    });

    expect(store.getState().isCompacting).toBe(false);
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('processQueue does not drain while compaction is active', async () => {
    const store = createTestStore();
    const sendMessage = mock(async () => {});
    store.setState({
      isCompacting: true,
      isProcessing: false,
      activeInterruptMode: 'queue',
      sessionId: 'session-abc',
      queuedMessages: ['queued during compaction'],
      sendMessage: sendMessage as any,
    });

    await store.getState().processQueue();

    expect(sendMessage).not.toHaveBeenCalled();
    expect(store.getState().queuedMessages).toEqual([
      'queued during compaction',
    ]);
  });

  it('sendMessage queues locally while compaction is active', async () => {
    const store = createTestStore();
    store.setState({
      uiMode: 'lite',
      isCompacting: true,
      activeInterruptMode: 'steer',
      sessionId: 'session-abc',
    });

    await store.getState().sendMessage('follow up');

    expect(store.getState().queuedMessages).toEqual(['follow up']);
  });

  it('queue is untouched when compaction starts', async () => {
    const store = createTestStore();
    store.setState({
      activeInterruptMode: 'queue',
      queuedMessages: ['pre-existing'],
    });

    await store.getState().handleCompactionEvent({
      type: AgentEventType.CompactionStatus,
      status: 'started',
    });

    expect(store.getState().queuedMessages).toEqual(['pre-existing']);
  });

  describe('queueing slash commands during a loading window', () => {
    it('does not surface a transient "queued" alert — strip already shows it', async () => {
      const store = createTestStore();
      store.setState({
        uiMode: 'lite',
        loadingMessage: 'Agent changing to coder',
      });

      await store.getState().handleUserInput('/verbosity');

      expect(store.getState().queuedMessages).toEqual(['/verbosity']);
      // Pre-fix: a transient alert echoed the same message the queue strip
      // already renders. The alert is gone now; the strip is the single
      // surface for "this is queued".
      expect(store.getState().transientAlert).toBeNull();
    });

    it('queues a duplicate slash command without any alert', async () => {
      // Re-issuing a command already in the queue is allowed — it appends
      // a second copy and surfaces no rejection alert (dedup removed).
      const store = createTestStore();
      store.setState({
        uiMode: 'lite',
        loadingMessage: 'Agent changing to coder',
        queuedMessages: ['/verbosity'],
      });

      await store.getState().handleUserInput('/verbosity');

      expect(store.getState().queuedMessages).toEqual([
        '/verbosity',
        '/verbosity',
      ]);
      expect(store.getState().transientAlert).toBeNull();
    });
  });
});

describe('KAS mode lite command gating (regression: KAS-only commands must not leak to the model)', () => {
  // `/rewind` is the canonical repro: it lives in KAS_COMMANDS → the
  // `kasCommands` slice, and is NOT advertised into `slashCommands`. Before
  // the fix, the four lite submit/queue gates checked `slashCommands` only,
  // so `/rewind` (and /effort, /spec, /model, /knowledge, /plan, …) fell
  // through and got sent to the LLM as chat text — the panel never opened and
  // a billed turn was wasted each time. The gates now consult
  // `liteGateCommands`, which in KAS mode is the merged visible list
  // (kasCommands ∪ slashCommands ∪ projections) — exactly what the dispatcher
  // can resolve.
  const KAS_ONLY_CMD = '/rewind';

  it('sanity: the KAS-only command is in kasCommands but NOT slashCommands', () => {
    const store = createKasTestStore();
    const { kasCommands, slashCommands } = store.getState();
    expect(kasCommands.some((c) => c.name === KAS_ONLY_CMD)).toBe(true);
    expect(slashCommands.some((c) => c.name === KAS_ONLY_CMD)).toBe(false);
  });

  describe('Gate 1: queueMessage mid-turn (keeps KAS command in the local queue, not steered)', () => {
    it('queues a KAS-only slash command locally instead of steering it to the backend', () => {
      const store = createKasTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        uiMode: 'lite',
        sessionId: 'session-abc',
        isProcessing: true,
        activeInterruptMode: 'steer',
      });

      store.getState().queueMessage(KAS_ONLY_CMD);

      // A known KAS command is treated as a command: it stays in the local
      // queue (to fire at turn-end) and is NOT routed to steerMessage as a
      // mid-turn chat message.
      expect(mockSteerMessage).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual([KAS_ONLY_CMD]);
    });

    it('still steers a genuine chat message mid-turn (KAS command gating does not catch plain text)', () => {
      const store = createKasTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        uiMode: 'lite',
        sessionId: 'session-abc',
        isProcessing: true,
        activeInterruptMode: 'steer',
      });

      store.getState().queueMessage('please keep going');

      expect(mockSteerMessage).toHaveBeenCalledWith(
        'session-abc',
        'please keep going'
      );
      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('Gate 2: processQueue drain (dispatches the KAS command, does not send as chat)', () => {
    it('dispatches a drained KAS-only slash command via handleUserInput instead of sendMessage', async () => {
      const store = createKasTestStore();
      const mockSendMessage = mock(() => Promise.resolve());
      const mockHandleUserInput = mock(async () => {});
      store.setState({
        uiMode: 'lite',
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
        queuedMessages: [KAS_ONLY_CMD],
        sendMessage: mockSendMessage as never,
        handleUserInput: mockHandleUserInput as never,
      });

      await store.getState().processQueue();

      // The drain branch recognizes the KAS command and routes it through the
      // slash-command dispatch path (handleUserInput), NOT sendMessage (which
      // would deliver it to the model as chat text).
      expect(mockHandleUserInput).toHaveBeenCalledWith(KAS_ONLY_CMD);
      expect(mockSendMessage).not.toHaveBeenCalled();
    });

    it('emits a [queue] drain row for a KAS-only slash command', async () => {
      const store = createKasTestStore();
      const messagesBefore = store.getState().messages.length;
      store.setState({
        uiMode: 'lite',
        sessionId: 'session-abc',
        activeInterruptMode: 'queue',
        queuedMessages: [KAS_ONLY_CMD],
      });

      await store.getState().processQueue();

      const drainRow = store.getState().messages[messagesBefore];
      expect(drainRow?.role).toBe(MessageRole.System);
      expect(drainRow?.content).toContain(`[queue] ${KAS_ONLY_CMD}`);
    });
  });

  describe('Gate 3: handleUserInput while processing (queues KAS command, does not reject or leak)', () => {
    it('queues a KAS-only slash command locally when processing in lite mode', async () => {
      const store = createKasTestStore();
      const mockSteerMessage = mock(() => Promise.resolve());
      (store.getState().kiro as any).steerMessage = mockSteerMessage;
      store.setState({
        uiMode: 'lite',
        sessionId: 'session-abc',
        isProcessing: true,
        activeInterruptMode: 'steer',
      });

      await store.getState().handleUserInput(KAS_ONLY_CMD);

      // Recognized as a command → queued for turn-end, not steered as chat
      // and not rejected with the "can't be queued" warning.
      expect(mockSteerMessage).not.toHaveBeenCalled();
      expect(store.getState().queuedMessages).toEqual([KAS_ONLY_CMD]);
    });
  });

  describe('Gate 4: handleUserInput idle (dispatches KAS command, does not send as chat)', () => {
    it('dispatches a KAS-only slash command via executeCommand instead of sendMessage', async () => {
      const store = createKasTestStore();
      const mockSendMessage = mock(() => Promise.resolve());
      store.setState({
        uiMode: 'lite',
        sessionId: 'session-abc',
        isProcessing: false,
        sendMessage: mockSendMessage as never,
      });

      const messagesBefore = store.getState().messages.length;
      await store.getState().handleUserInput(KAS_ONLY_CMD);

      // The idle lite gate recognizes the KAS command and routes it to
      // executeCommand (which dispatches via kasCommands ∪ slashCommands).
      // It must NOT fall through to sendMessage (the leak-to-model path).
      expect(mockSendMessage).not.toHaveBeenCalled();
      // Positive signal that the command actually dispatched: handleRewind
      // with no args + no prior turns surfaces "No previous turns to rewind
      // to". In lite mode warnings route to a System scrollback row (via
      // applyLiteAlertRouting → addSystemMessage), not transientAlert. (If
      // the gate had leaked, sendMessage would have fired and no such row
      // would appear.)
      const newRows = store.getState().messages.slice(messagesBefore);
      expect(
        newRows.some(
          (m) =>
            typeof m.content === 'string' &&
            m.content.includes('No previous turns')
        )
      ).toBe(true);
    });

    it('still sends an unknown slash token as chat (lite contract preserved in KAS mode)', async () => {
      const store = createKasTestStore();
      const mockSendMessage = mock(() => Promise.resolve());
      store.setState({
        uiMode: 'lite',
        sessionId: 'session-abc',
        isProcessing: false,
        sendMessage: mockSendMessage as never,
      });

      await store.getState().handleUserInput('/foozle');

      // Typos and pasted paths are not commands in either slice → they remain
      // chat messages, exactly as in v2. The fix widens the known set to
      // include KAS commands; it does not turn every slash token into a
      // command.
      expect(mockSendMessage).toHaveBeenCalled();
    });
  });

  describe('v2 no-op guard: KAS gating must not change v2 behavior', () => {
    it('a /rewind-style token unknown to v2 still goes to chat (v2 has no kasCommands)', async () => {
      // In v2 mode `kasCommands` is empty and `liteGateCommands` returns the
      // raw `slashCommands` slice — byte-identical to the pre-fix gate. A
      // token that is not a backend/host v2 command must still be sent as a
      // chat message.
      const store = createTestStore(); // v2 store (default engine)
      const mockSendMessage = mock(() => Promise.resolve());
      store.setState({
        uiMode: 'lite',
        sessionId: 'session-abc',
        isProcessing: false,
        sendMessage: mockSendMessage as never,
      });

      await store.getState().handleUserInput('/rewind');

      expect(store.getState().agentEngine).toBe('v2');
      expect(mockSendMessage).toHaveBeenCalled();
    });
  });
});

// Ralph hunt-3 — multi-steer line-aware edit/delete at the store boundary.
// Pre-init (sessionId null) the steer buffer is a purely local first-prompt
// staging slot; successive submissions concatenate with "\n\n". Editing or
// deleting ONE staged steer row must preserve the others — the old
// whole-buffer replace/clear silently dropped sibling lines (lost prompts).
describe('multi-steer line-aware edit/delete (pre-init, local)', () => {
  function stagedTwo() {
    const store = createTestStore();
    // Pre-init: not initialized + no sessionId → buffers locally, concatenating.
    store.setState({ isInitialized: false, sessionId: null });
    store.getState().queueMessage('first message');
    store.getState().queueMessage('second message');
    return store;
  }

  it('concatenates successive pre-init submissions with the \\n\\n separator', () => {
    const store = stagedTwo();
    expect(store.getState().pendingSteerContent).toBe(
      'first message\n\nsecond message'
    );
  });

  it('editing one steer row preserves the sibling line', () => {
    const store = stagedTwo();
    // Edit the FIRST row; the second must survive.
    store.getState().replaceSteerMessage('first-edited', 'first message');
    expect(store.getState().pendingSteerContent).toBe(
      'first-edited\n\nsecond message'
    );
  });

  it('deleting one steer row preserves the sibling line', () => {
    const store = stagedTwo();
    // Delete the first row (empty edit) targeting its original text.
    store.getState().clearSteerMessage('first message');
    expect(store.getState().pendingSteerContent).toBe('second message');
  });

  it('deleting the last remaining steer row clears the buffer', () => {
    const store = createTestStore();
    store.setState({ isInitialized: false, sessionId: null });
    store.getState().queueMessage('only message');
    store.getState().clearSteerMessage('only message');
    expect(store.getState().pendingSteerContent).toBeNull();
  });

  it('whole-buffer replace (no target) still works for the single-steer case', () => {
    const store = createTestStore();
    store.setState({ isInitialized: false, sessionId: null });
    store.getState().queueMessage('solo');
    store.getState().replaceSteerMessage('solo-edited');
    expect(store.getState().pendingSteerContent).toBe('solo-edited');
  });
});
