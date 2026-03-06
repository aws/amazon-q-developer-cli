import { describe, it, expect, mock } from 'bun:test';
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

describe('Agent welcome message', () => {
  it('setCurrentAgent with welcomeMessage adds a Model message', () => {
    const store = createAppStore({ kiro: new Kiro() });

    store.getState().setCurrentAgent({
      name: 'vanilla',
      welcomeMessage: '👋 Hello from vanilla!',
    });

    expect(store.getState().currentAgent).toEqual({ name: 'vanilla' });

    const messages = store.getState().messages;
    expect(messages).toHaveLength(1);
    expect(messages[0]!.role).toBe(MessageRole.Model);
    expect(messages[0]!.content).toBe('👋 Hello from vanilla!');
    expect((messages[0] as any).agentName).toBe('vanilla');
  });

  it('setCurrentAgent without welcomeMessage adds no message', () => {
    const store = createAppStore({ kiro: new Kiro() });

    store.getState().setCurrentAgent({ name: 'default' });

    expect(store.getState().currentAgent).toEqual({ name: 'default' });
    expect(store.getState().messages).toHaveLength(0);
  });

  it('AgentSwitched event with welcomeMessage adds message via stream handler', () => {
    const store = createAppStore({ kiro: new Kiro() });
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.AgentSwitched,
      agentName: 'coder',
      welcomeMessage: '🔧 Coder mode activated!',
    });

    expect(store.getState().currentAgent).toEqual({ name: 'coder' });

    const messages = store.getState().messages;
    expect(messages).toHaveLength(1);
    expect(messages[0]!.role).toBe(MessageRole.Model);
    expect(messages[0]!.content).toBe('🔧 Coder mode activated!');
  });

  it('AgentSwitched event without welcomeMessage adds no message', () => {
    const store = createAppStore({ kiro: new Kiro() });
    const handler = store.getState().createStreamEventHandler();

    handler({
      type: AgentEventType.AgentSwitched,
      agentName: 'default',
    });

    expect(store.getState().currentAgent).toEqual({ name: 'default' });
    expect(store.getState().messages).toHaveLength(0);
  });

  it('switching agents twice shows both welcome messages', () => {
    const store = createAppStore({ kiro: new Kiro() });

    store.getState().setCurrentAgent({
      name: 'agent-a',
      welcomeMessage: 'Welcome A',
    });
    store.getState().setCurrentAgent({
      name: 'agent-b',
      welcomeMessage: 'Welcome B',
    });

    const messages = store.getState().messages;
    expect(messages).toHaveLength(2);
    expect(messages[0]!.content).toBe('Welcome A');
    expect(messages[1]!.content).toBe('Welcome B');
  });
});
