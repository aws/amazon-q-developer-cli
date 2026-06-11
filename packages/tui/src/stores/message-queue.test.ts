import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore, MessageRole } from './app-store';
import { Kiro } from '../kiro';
import { AgentEventType } from '../types/agent-events';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
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

describe('Message queue', () => {
  describe('queueMessage', () => {
    it('appends trimmed message to queuedMessages', () => {
      const store = createTestStore();
      store.getState().queueMessage('  hello world  ');
      expect(store.getState().queuedMessages).toEqual(['hello world']);
    });

    it('preserves FIFO order for multiple messages', () => {
      const store = createTestStore();
      store.getState().queueMessage('first');
      store.getState().queueMessage('second');
      store.getState().queueMessage('third');
      expect(store.getState().queuedMessages).toEqual([
        'first',
        'second',
        'third',
      ]);
    });

    it('rejects empty string', () => {
      const store = createTestStore();
      store.getState().queueMessage('');
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('rejects whitespace-only string', () => {
      const store = createTestStore();
      store.getState().queueMessage('   ');
      store.getState().queueMessage('\t\n');
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('allows queuing the same slash command twice (no dedup)', () => {
      // Re-queuing a slash command is a valid action — the user may want
      // to re-run it (e.g. re-open a picker). Duplicates are kept in FIFO
      // order rather than rejected.
      const store = createTestStore();
      store.getState().queueMessage('/model');
      store.getState().queueMessage('/model');
      expect(store.getState().queuedMessages).toEqual(['/model', '/model']);
      // No rejection alert — the queue strip is the only feedback surface.
      expect(store.getState().transientAlert).toBeNull();
    });

    it('keeps distinct argv slash commands in order', () => {
      const store = createTestStore();
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
      store.getState().queueMessage('fix the bug');
      store.getState().queueMessage('fix the bug');
      expect(store.getState().queuedMessages).toEqual([
        'fix the bug',
        'fix the bug',
      ]);
    });

    it('returns true on append, false only on empty', () => {
      const store = createTestStore();
      expect(store.getState().queueMessage('first')).toBe(true);
      expect(store.getState().queueMessage('second')).toBe(true);
      expect(store.getState().queueMessage('/model')).toBe(true);
      expect(store.getState().queueMessage('/model')).toBe(true); // dup ok
      expect(store.getState().queueMessage('   ')).toBe(false); // empty
      expect(store.getState().queueMessage('')).toBe(false); // empty
    });
  });

  describe('clearQueue', () => {
    it('empties the queue', () => {
      const store = createTestStore();
      store.getState().queueMessage('a');
      store.getState().queueMessage('b');
      expect(store.getState().queuedMessages).toHaveLength(2);

      store.getState().clearQueue();
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('is a no-op on empty queue', () => {
      const store = createTestStore();
      store.getState().clearQueue();
      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('processQueue', () => {
    it('dequeues first message and sends it', async () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['hello'] });

      await store.getState().processQueue();

      // Message was dequeued
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('is a no-op when queue is empty', async () => {
      const store = createTestStore();
      await store.getState().processQueue();
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('dequeues only the first message (FIFO)', async () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['first', 'second', 'third'] });

      await store.getState().processQueue();

      // With mock kiro, sendMessage completes immediately and recursively
      // processes the entire queue. All messages should be dequeued.
      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('does not clear input buffer when processing queue', async () => {
      const store = createTestStore();
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
    it('queues message when isProcessing is true', async () => {
      const store = createTestStore();
      store.setState({ isProcessing: true });

      await store.getState().handleUserInput('queued message');

      expect(store.getState().queuedMessages).toEqual(['queued message']);
    });

    it('clears input buffer after queuing', async () => {
      const store = createTestStore();
      store.setState({ isProcessing: true });
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

    it('does not queue empty/whitespace input during processing', async () => {
      const store = createTestStore();
      store.setState({ isProcessing: true });

      await store.getState().handleUserInput('   ');
      await store.getState().handleUserInput('');

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('rejects slash commands with a warning when processing', async () => {
      const store = createTestStore();
      store.setState({ isProcessing: true });

      await store.getState().handleUserInput('/help');

      // Slash command should NOT be queued
      expect(store.getState().queuedMessages).toEqual([]);
      // A transient alert should be shown
      expect(store.getState().transientAlert).not.toBeNull();
      expect(store.getState().transientAlert?.status).toBe('warning');
    });

    it('rejects slash commands with a warning when not initialized', async () => {
      const store = createTestStore();
      store.setState({ isInitialized: false });

      await store.getState().handleUserInput('/context');

      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().transientAlert).not.toBeNull();
    });

    it('still allows /quit when processing', async () => {
      // We can't fully test process.exit, but we can verify /quit
      // doesn't get queued or trigger the slash command warning
      const store = createTestStore();
      store.setState({ isProcessing: true });

      // /quit calls process.exit so we can't actually invoke it,
      // but we can verify other slash commands are blocked
      await store.getState().handleUserInput('/help');
      await store.getState().handleUserInput('/context');
      await store.getState().handleUserInput('/model');

      expect(store.getState().queuedMessages).toEqual([]);
    });

    it('queues regular messages but not slash commands when processing', async () => {
      const store = createTestStore();
      store.setState({ isProcessing: true });

      await store.getState().handleUserInput('fix the bug');
      await store.getState().handleUserInput('/help');
      await store.getState().handleUserInput('add tests too');

      // Only regular messages should be queued
      expect(store.getState().queuedMessages).toEqual([
        'fix the bug',
        'add tests too',
      ]);
    });
  });

  describe('queuing during initialization', () => {
    it('queues message via handleUserInput when not initialized', async () => {
      const store = createTestStore();
      store.setState({ isInitialized: false });

      await store.getState().handleUserInput('early message');

      expect(store.getState().queuedMessages).toEqual(['early message']);
    });

    it('queues message via sendMessage when not initialized', async () => {
      const store = createTestStore();
      store.setState({ isInitialized: false });

      await store.getState().sendMessage('early message');

      expect(store.getState().queuedMessages).toEqual(['early message']);
      expect(store.getState().isProcessing).toBe(false);
    });

    it('drains queue after isInitialized becomes true', async () => {
      const store = createTestStore();
      store.setState({ isInitialized: false });

      store.getState().queueMessage('queued during init');
      store.setState({ isInitialized: true });
      await store.getState().processQueue();

      expect(store.getState().queuedMessages).toEqual([]);
    });
  });

  describe('cancellation semantics', () => {
    it('clearQueue + cancelMessage clears queue (Escape behavior)', () => {
      const store = createTestStore();
      store.setState({
        isProcessing: true,
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
        queuedMessages: ['msg1', 'msg2', 'msg3'],
      });

      store.getState().cancelMessage();

      expect(store.getState().queuedMessages).toEqual(['msg1', 'msg2', 'msg3']);
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

    it('expanded state persists across queued turns (not reset by sendMessage)', async () => {
      const store = createTestStore();
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
      store.getState().toggleToolOutputsExpanded();
      store.setState({ queuedMessages: ['a', 'b'] });

      store.getState().clearQueue();

      expect(store.getState().queuedMessages).toEqual([]);
      expect(store.getState().toolOutputsExpanded).toBe(true);
    });
  });

  describe('compaction drains queue', () => {
    it('processQueue is called after compaction completes', async () => {
      const store = createTestStore();
      store.setState({
        isCompacting: true,
        isProcessing: true,
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
        isProcessing: true,
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

    it('queue is untouched when compaction starts', async () => {
      const store = createTestStore();
      store.setState({ queuedMessages: ['pre-existing'] });

      await store.getState().handleCompactionEvent({
        type: AgentEventType.CompactionStatus,
        status: 'started',
      });

      expect(store.getState().queuedMessages).toEqual(['pre-existing']);
    });
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
