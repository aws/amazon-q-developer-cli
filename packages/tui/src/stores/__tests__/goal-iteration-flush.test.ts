import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore, MessageRole } from '../app-store';
import { AgentEventType, ContentType } from '../../types/agent-events';
import { Kiro } from '../../kiro';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

describe('Goal iteration buffer flush', () => {
  function createTestStore() {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isInitialized: true });
    return store;
  }

  it('new iteration content does not overwrite previous iteration Model message', async () => {
    const store = createTestStore();

    // Simulate a user message to start the turn
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: '/goal test' }],
      isProcessing: true,
    });

    // Set initial goal status (first goal set)
    store.getState().setGoalStatus({
      state: 'active',
      iteration: 0,
      maxIterations: 3,
      message: 'test goal',
    });

    const handler = store.getState().createStreamEventHandler();

    // --- Iteration 1: stream content ---
    handler({
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: ContentType.Text, text: 'Iteration 1 response' },
    });

    // Let the 16ms flush timer fire
    await new Promise((r) => setTimeout(r, 30));

    const msgsAfterIter1 = store.getState().messages;
    const iter1Model = msgsAfterIter1.find(
      (m) =>
        m.role === MessageRole.Model && m.content === 'Iteration 1 response'
    );
    expect(iter1Model).toBeDefined();

    // --- Goal iteration advances (GoalStatus with higher iteration) ---
    handler({
      type: AgentEventType.GoalStatus,
      state: 'active',
      iteration: 1,
      maxIterations: 3,
      message: 'test goal',
    });

    // --- ToolCall separator (goal iteration marker) ---
    handler({
      type: AgentEventType.ToolCall,
      id: 'goal-iter-1',
      name: '⟳ Goal iteration 2/3',
      kind: 'other',
      args: {},
    });

    // --- Iteration 2: stream content ---
    handler({
      type: AgentEventType.Content,
      id: 'c2',
      content: { type: ContentType.Text, text: 'Iteration 2 response' },
    });

    await new Promise((r) => setTimeout(r, 30));

    const msgsAfterIter2 = store.getState().messages;

    // Verify iteration 1 content is STILL present and not overwritten
    const iter1ModelAfter = msgsAfterIter2.find(
      (m) =>
        m.role === MessageRole.Model && m.content === 'Iteration 1 response'
    );
    expect(iter1ModelAfter).toBeDefined();

    // Verify iteration 2 content exists as a SEPARATE message
    const iter2Model = msgsAfterIter2.find(
      (m) =>
        m.role === MessageRole.Model && m.content === 'Iteration 2 response'
    );
    expect(iter2Model).toBeDefined();

    // Verify they are different messages
    expect(iter1ModelAfter!.id).not.toBe(iter2Model!.id);
  });

  it('GoalStatus iteration advance resets buffer so content does not accumulate across iterations', async () => {
    const store = createTestStore();

    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: '/goal test' }],
      isProcessing: true,
    });

    store.getState().setGoalStatus({
      state: 'active',
      iteration: 0,
      maxIterations: 3,
      message: 'test goal',
    });

    const handler = store.getState().createStreamEventHandler();

    // Stream iteration 1 content (unflushed — timer pending)
    handler({
      type: AgentEventType.Content,
      id: 'c1',
      content: { type: ContentType.Text, text: 'First iteration text' },
    });

    // GoalStatus advances — fix flushes pending content and resets buffer
    handler({
      type: AgentEventType.GoalStatus,
      state: 'active',
      iteration: 1,
      maxIterations: 3,
      message: 'test goal',
    });

    // ToolCall arrives (normal case — separates iterations in the message list)
    handler({
      type: AgentEventType.ToolCall,
      id: 'goal-iter-1',
      name: '⟳ Goal iteration 2/3',
      kind: 'other',
      args: {},
    });

    // New iteration content arrives
    handler({
      type: AgentEventType.Content,
      id: 'c2',
      content: { type: ContentType.Text, text: 'Second iteration text' },
    });

    await new Promise((r) => setTimeout(r, 30));

    const msgs = store.getState().messages;
    const modelMsgs = msgs.filter((m) => m.role === MessageRole.Model);

    // Both iterations have their own Model message
    expect(modelMsgs.length).toBe(2);
    expect(modelMsgs[0]!.content).toBe('First iteration text');
    expect(modelMsgs[1]!.content).toBe('Second iteration text');

    // Verify content wasn't merged (the bug: "First iteration textSecond iteration text")
    expect(modelMsgs[0]!.content).not.toContain('Second');
  });
});
