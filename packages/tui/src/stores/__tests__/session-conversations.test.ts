import { describe, it, expect, mock, beforeEach } from 'bun:test';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({})),
}));

import { sessionConversationsStore } from '../session-conversations';
import { AgentEventType, ContentType } from '../../types/agent-events';
import { MessageRole } from '../app-store';

describe('sessionConversationsStore', () => {
  beforeEach(() => {
    // Reset store to initial state
    sessionConversationsStore.setState({ conversations: new Map() });
  });

  it('initial state has empty conversations Map', () => {
    const state = sessionConversationsStore.getState();
    expect(state.conversations.size).toBe(0);
  });

  it('createHandlerForSession returns a function', () => {
    const handler = sessionConversationsStore
      .getState()
      .createHandlerForSession('session-1');
    expect(typeof handler).toBe('function');
  });

  it('feeding Content events through handler adds messages to conversation', async () => {
    const handler = sessionConversationsStore
      .getState()
      .createHandlerForSession('session-1');

    handler({
      type: AgentEventType.Content,
      id: 'msg-1',
      content: { type: ContentType.Text, text: 'hello from session' },
    });

    // Wait for the 16ms batched flush
    await new Promise((r) => setTimeout(r, 50));

    const conversations = sessionConversationsStore.getState().conversations;
    const msgs = conversations.get('session-1');
    expect(msgs).toBeDefined();
    expect(msgs!.length).toBeGreaterThan(0);
    expect(msgs![0]!.role).toBe(MessageRole.Model);
    expect(msgs![0]!.content).toBe('hello from session');
  });

  it('clearSession removes a session messages', async () => {
    const handler = sessionConversationsStore
      .getState()
      .createHandlerForSession('session-1');

    handler({
      type: AgentEventType.Content,
      id: 'msg-1',
      content: { type: ContentType.Text, text: 'hello' },
    });

    await new Promise((r) => setTimeout(r, 50));
    expect(
      sessionConversationsStore.getState().conversations.has('session-1')
    ).toBe(true);

    sessionConversationsStore.getState().clearSession('session-1');

    expect(
      sessionConversationsStore.getState().conversations.has('session-1')
    ).toBe(false);
  });

  it('MAX_SESSION_MESSAGES cap (50) is enforced', async () => {
    const handler = sessionConversationsStore
      .getState()
      .createHandlerForSession('session-1');

    // Feed 55 ToolCall events (these are synchronous, no batching needed)
    for (let i = 0; i < 55; i++) {
      handler({
        type: AgentEventType.ToolCall,
        id: `tool-${i}`,
        name: 'test_tool',
        args: { idx: i },
      });
    }

    const msgs = sessionConversationsStore
      .getState()
      .conversations.get('session-1');
    expect(msgs).toBeDefined();
    expect(msgs!.length).toBeLessThanOrEqual(50);
  });

  it('multiple sessions are independent', async () => {
    const handler1 = sessionConversationsStore
      .getState()
      .createHandlerForSession('session-1');
    const handler2 = sessionConversationsStore
      .getState()
      .createHandlerForSession('session-2');

    handler1({
      type: AgentEventType.Content,
      id: 'msg-1',
      content: { type: ContentType.Text, text: 'from session 1' },
    });
    handler2({
      type: AgentEventType.Content,
      id: 'msg-2',
      content: { type: ContentType.Text, text: 'from session 2' },
    });

    await new Promise((r) => setTimeout(r, 50));

    const conversations = sessionConversationsStore.getState().conversations;
    expect(conversations.size).toBe(2);

    const msgs1 = conversations.get('session-1');
    const msgs2 = conversations.get('session-2');
    expect(msgs1![0]!.content).toBe('from session 1');
    expect(msgs2![0]!.content).toBe('from session 2');
  });
});
