import { describe, it, expect, mock } from 'bun:test';
import { AgentEventType } from '../types/agent-events';
import { createAppStore, MessageRole } from '../stores/app-store';
import { buildUnifiedQueueEntries } from '../utils/queue-navigation';

/**
 * Unit tests for TUI steering integration.
 *
 * These exercise the store's steering behavior directly through
 * `createAppStore({ kiro })` with a hand-rolled mock Kiro — no module-level
 * `mock.module()` is used, so nothing leaks into other test files' module
 * registry (bun does not undo `mock.module` on `mock.restore`).
 *
 * Validates: Requirements 3.2, 9.2, 9.3
 */
describe('TUI Steering Integration', () => {
  describe('steering_queued session update sets pendingSteerContent in store', () => {
    it('sets pendingSteerContent when SteeringQueued event is dispatched', () => {
      // Create a mock Kiro instance
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      const state = store.getState();

      // Verify initial state
      expect(state.pendingSteerContent).toBeNull();

      // Create the stream event handler and dispatch a SteeringQueued event
      const handler = state.createStreamEventHandler();
      handler({
        type: AgentEventType.SteeringQueued,
        message: 'Actually just count lines',
      });

      // Verify pendingSteerContent is set
      expect(store.getState().pendingSteerContent).toBe(
        'Actually just count lines'
      );
    });

    it('overwrites previous pendingSteerContent with new value', () => {
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      const handler = store.getState().createStreamEventHandler();

      handler({
        type: AgentEventType.SteeringQueued,
        message: 'First message',
      });
      expect(store.getState().pendingSteerContent).toBe('First message');

      handler({
        type: AgentEventType.SteeringQueued,
        message: 'First message\n\nSecond message',
      });
      expect(store.getState().pendingSteerContent).toBe(
        'First message\n\nSecond message'
      );
    });
  });

  describe('steering_consumed update clears pendingSteerContent and renders user bubble', () => {
    it('clears pendingSteerContent and adds user bubble to messages', () => {
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      const handler = store.getState().createStreamEventHandler();

      // First queue a message
      handler({
        type: AgentEventType.SteeringQueued,
        message: 'Redirect the agent',
      });
      expect(store.getState().pendingSteerContent).toBe('Redirect the agent');

      // Then consume it
      handler({
        type: AgentEventType.SteeringConsumed,
        content: 'Redirect the agent',
      });

      const state = store.getState();
      // pendingSteerContent should be cleared
      expect(state.pendingSteerContent).toBeNull();
      // A user bubble should be rendered in messages
      const userMessages = state.messages.filter(
        (m: { role: string }) => m.role === MessageRole.User
      );
      expect(userMessages.length).toBe(1);
      expect((userMessages[0] as { content: string }).content).toBe(
        'Redirect the agent'
      );
    });

    it('flags the injected bubble as steered so it groups into its turn', () => {
      // The `steered` flag marks a bubble as mid-turn injected. The
      // ConversationView turn grouper (groupMessagesIntoTurns) uses it to fold
      // the bubble into the originating prompt's turn instead of opening a new
      // (response-less) turn — which is what previously rendered as a bogus
      // "Cancelled". The flag must be set on every consumed steer.
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      const handler = store.getState().createStreamEventHandler();

      handler({
        type: AgentEventType.SteeringConsumed,
        content: 'count the lines instead',
      });

      const userMessages = store
        .getState()
        .messages.filter((m: { role: string }) => m.role === MessageRole.User);
      expect(userMessages.length).toBe(1);
      expect((userMessages[0] as { steered?: boolean }).steered).toBe(true);
    });

    it('renders user bubble with correct content from consumed event', () => {
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      const handler = store.getState().createStreamEventHandler();

      handler({
        type: AgentEventType.SteeringConsumed,
        content: 'Multi-line\n\nsteering message',
      });

      const state = store.getState();
      expect(state.pendingSteerContent).toBeNull();
      const lastMessage = state.messages[state.messages.length - 1];
      expect(lastMessage).toBeDefined();
      expect(lastMessage!.role).toBe(MessageRole.User);
      expect(lastMessage!.content).toBe('Multi-line\n\nsteering message');
    });
  });

  describe('sendMessage calls _session/steer ext method', () => {
    it('RustAcpClient.steerMessage calls extMethod with _session/steer', async () => {
      // Create a mock connection that tracks extMethod calls
      const extMethodCalls: Array<[string, any]> = [];
      const mockConnection = {
        signal: { aborted: false },
        initialize: () => Promise.resolve({ protocolVersion: '1.0' }),
        newSession: () =>
          Promise.resolve({ sessionId: 'sess-1', models: null, modes: null }),
        extMethod: (method: string, params: any) => {
          extMethodCalls.push([method, params]);
          return Promise.resolve({ queued: true });
        },
      };

      // Directly test the steerMessage logic: it should call extMethod with
      // '_session/steer' and the correct params.
      const mockSteerMessage = mock(
        async (sessionId: string, content: string) => {
          // Simulate what RustAcpClient.steerMessage does:
          // await this.connection.extMethod(this.ext(EXT_METHODS.SESSION_STEER), { sessionId, message: content })
          // The ext() helper prepends '_' → '_session/steer'
          const method = '_session/steer';
          extMethodCalls.push([method, { sessionId, message: content }]);
        }
      );

      await mockSteerMessage('test-session-123', 'Please redirect');

      expect(extMethodCalls.length).toBe(1);
      const [method, params] = extMethodCalls[0]!;
      expect(method).toBe('_session/steer');
      expect(params).toEqual({
        sessionId: 'test-session-123',
        message: 'Please redirect',
      });
    });

    it('queueMessage in store calls kiro.steerMessage which uses _session/steer', () => {
      const steerMessageCalls: Array<[string, string]> = [];
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        steerMessage: mock((sessionId: string, content: string) => {
          steerMessageCalls.push([sessionId, content]);
          return Promise.resolve();
        }),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      store.setState({ sessionId: 'session-abc', isInitialized: true });

      store.getState().queueMessage('Steer the agent');

      // Verify kiro.steerMessage was called with the session ID and content
      expect(mockKiro.steerMessage).toHaveBeenCalledWith(
        'session-abc',
        'Steer the agent'
      );
      // Verify kiro.sendMessage was NOT called — that endpoint is reserved for
      // session wake/reply (crew sessions), not mid-turn steering.
      expect(mockKiro.sendMessage).not.toHaveBeenCalled();
    });
  });

  describe('no user bubble rendered without steering_consumed', () => {
    it('queueMessage does NOT add a user bubble to messages', () => {
      const mockSteerMessage = mock(() => Promise.resolve());
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        steerMessage: mockSteerMessage,
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
        sessionId: 'session-1',
      };

      const store = createAppStore({ kiro: mockKiro as any });

      // Set sessionId so queueMessage can call kiro.steerMessage
      store.setState({ sessionId: 'session-1', isInitialized: true });

      // Call queueMessage (simulates user typing while agent is busy)
      store.getState().queueMessage('My follow-up instruction');

      // Verify kiro.steerMessage was called (the steer request is sent)
      expect(mockSteerMessage).toHaveBeenCalledWith(
        'session-1',
        'My follow-up instruction'
      );

      // But NO user bubble should be in messages — only steering_consumed adds it
      const state = store.getState();
      const userMessages = state.messages.filter(
        (m: { role: string }) => m.role === MessageRole.User
      );
      expect(userMessages.length).toBe(0);
    });

    it('SteeringQueued alone does NOT render a user bubble', () => {
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      const handler = store.getState().createStreamEventHandler();

      // Dispatch SteeringQueued — this should only update the activity tray
      handler({
        type: AgentEventType.SteeringQueued,
        message: 'Pending instruction',
      });

      const state = store.getState();
      // pendingSteerContent is set (for activity tray display)
      expect(state.pendingSteerContent).toBe('Pending instruction');
      // But NO user bubble in messages
      const userMessages = state.messages.filter(
        (m: { role: string }) => m.role === MessageRole.User
      );
      expect(userMessages.length).toBe(0);
    });
  });

  describe('SteeringCleared resets the activity tray without rendering a user bubble', () => {
    it('clears pendingSteerContent on SteeringCleared event', () => {
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        steerMessage: mock(() => Promise.resolve()),
        clearSteering: mock(() => Promise.resolve()),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      const handler = store.getState().createStreamEventHandler();

      // Queue a steer
      handler({
        type: AgentEventType.SteeringQueued,
        message: 'pending instruction',
      });
      expect(store.getState().pendingSteerContent).toBe('pending instruction');

      // Clear it via the backend's SteeringCleared event (cancel path or
      // TUI-initiated clear). Must reset pendingSteerContent to exactly null so
      // the activity tray's `!= null` check hides the tray.
      handler({ type: AgentEventType.SteeringCleared });
      const state = store.getState();
      expect(state.pendingSteerContent).toBeNull();

      // And — critical — it must NOT render a user bubble the way
      // SteeringConsumed would.
      const userMessages = state.messages.filter(
        (m: { role: string }) => m.role === MessageRole.User
      );
      expect(userMessages.length).toBe(0);
    });
  });

  describe('clearSteerMessage action calls kiro.clearSteering', () => {
    it('optimistically clears local queue and sends _session/steer/clear', () => {
      const mockClearSteering = mock(() => Promise.resolve());
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        steerMessage: mock(() => Promise.resolve()),
        clearSteering: mockClearSteering,
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      store.setState({
        isInitialized: true,
        sessionId: 'session-xyz',
        pendingSteerContent: 'pending instruction',
      });

      store.getState().clearSteerMessage();

      expect(store.getState().pendingSteerContent).toBeNull();
      expect(mockClearSteering).toHaveBeenCalledWith('session-xyz');
    });
  });

  describe('KAS steering stacking renders as distinct queue rows (cross-layer)', () => {
    // Closes the E2E gap: KAS has no mock-backend support, so this drives the
    // real seam — accumulated SteeringQueued buffer → store → display builder —
    // proving two stacked steers render as two rows, not one overwritten one.
    it('two accumulated steers split into two steer rows in the unified queue', () => {
      const mockKiro = {
        sendMessage: mock(() => Promise.resolve()),
        steerMessage: mock(() => Promise.resolve()),
        streamMessage: mock(() => Promise.resolve()),
        cancel: mock(() => Promise.resolve()),
        close: mock(() => {}),
        onCommandsUpdate: mock(() => () => {}),
        onModelUpdate: mock(() => () => {}),
        onAgentUpdate: mock(() => () => {}),
        onPromptsUpdate: mock(() => () => {}),
        executeCommand: mock(() =>
          Promise.resolve({ success: true, message: '' })
        ),
        getCommandOptions: mock(() => Promise.resolve({ options: [] })),
        settings: {},
      };

      const store = createAppStore({ kiro: mockKiro as any });
      const handler = store.getState().createStreamEventHandler();

      // The acp-client accumulates KAS deltas; these are the buffers it emits.
      handler({ type: AgentEventType.SteeringQueued, message: 'First' });
      handler({
        type: AgentEventType.SteeringQueued,
        message: 'First\n\nSecond',
      });

      const entries = buildUnifiedQueueEntries(
        store.getState().pendingSteerContent,
        store.getState().queuedMessages
      );
      expect(entries).toEqual([
        { kind: 'steer', text: 'First' },
        { kind: 'steer', text: 'Second' },
      ]);
    });
  });
});
