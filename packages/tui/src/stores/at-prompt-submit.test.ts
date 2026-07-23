import { describe, it, expect, mock, beforeAll, afterAll } from 'bun:test';
import { tmpdir } from 'os';
import { join } from 'path';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';
import { InterruptMode } from '../constants/interrupt-mode';
import { CommandHistory } from '../utils/command-history';
import type { PromptEntry } from '../types/commands';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    streamMessage: mock(),
    sendMessage: mock(),
    steerMessage: mock(),
    clearSteering: mock(),
    cancel: mock(),
    close: mock(),
    sendChatSlashCommandTelemetry: mock(),
  })),
}));

beforeAll(() => {
  // Keep test submissions out of the user's real history file.
  CommandHistory.getInstance().switchToFile(
    join(tmpdir(), `at-prompt-submit-test-${process.pid}.history`)
  );
});

afterAll(() => {
  mock.restore();
});

const sopPrompt: PromptEntry = {
  name: 'agent-sop:pdd',
  description: 'PDD SOP',
  arguments: [],
  source: { kind: 'mcp', serverName: 'builder-mcp' },
};

function createTestStore() {
  const mockKiro = new Kiro();
  const store = createAppStore({ kiro: mockKiro });
  store.setState({
    isInitialized: true,
    prompts: [sopPrompt],
  });
  return { store, mockKiro };
}

function streamedContents(mockKiro: Kiro): string[] {
  return (mockKiro.streamMessage as ReturnType<typeof mock>).mock.calls.map(
    (call: unknown[]) => call[0] as string
  );
}

describe('submit-time @prompt interception', () => {
  it('dispatches a typed @prompt as the /name form when the name matches a known prompt', async () => {
    const { store, mockKiro } = createTestStore();

    await store.getState().handleUserInput('@agent-sop:pdd');

    expect(streamedContents(mockKiro)).toEqual(['/agent-sop:pdd']);
  });

  it('preserves arguments after the prompt name', async () => {
    const { store, mockKiro } = createTestStore();

    await store.getState().handleUserInput('@agent-sop:pdd fix the login bug');

    expect(streamedContents(mockKiro)).toEqual([
      '/agent-sop:pdd fix the login bug',
    ]);
  });

  it('intercepts prompts that live only in the prompts slice, not slashCommands', async () => {
    const { store, mockKiro } = createTestStore();
    // Explicitly pin: the prompt is NOT in the slashCommands slice.
    expect(
      store.getState().slashCommands.some((c) => c.name === '/agent-sop:pdd')
    ).toBe(false);

    await store.getState().handleUserInput('@agent-sop:pdd');

    expect(streamedContents(mockKiro)).toEqual(['/agent-sop:pdd']);
  });

  it('sends unknown @names verbatim', async () => {
    const { store, mockKiro } = createTestStore();

    await store.getState().handleUserInput('@unknown-thing do stuff');

    expect(streamedContents(mockKiro)).toEqual(['@unknown-thing do stuff']);
  });

  it('sends @file-paths verbatim', async () => {
    const { store, mockKiro } = createTestStore();

    await store.getState().handleUserInput('@src/utils/foo.ts');

    expect(streamedContents(mockKiro)).toEqual(['@src/utils/foo.ts']);
  });

  it('leaves mid-message @prompt references untouched', async () => {
    const { store, mockKiro } = createTestStore();

    await store.getState().handleUserInput('hello @agent-sop:pdd world');

    expect(streamedContents(mockKiro)).toEqual(['hello @agent-sop:pdd world']);
  });

  it('passes multiline @prompt input through verbatim', async () => {
    const { store, mockKiro } = createTestStore();

    await store.getState().handleUserInput('@agent-sop:pdd\nsecond line');

    expect(streamedContents(mockKiro)).toEqual(['@agent-sop:pdd\nsecond line']);
  });

  it('dispatches a differently-cased @prompt as the canonical /name form', async () => {
    const { store, mockKiro } = createTestStore();

    await store.getState().handleUserInput('@Agent-SOP:PDD fix it');

    expect(streamedContents(mockKiro)).toEqual(['/agent-sop:pdd fix it']);
  });

  it('sends a prompt whose name the command parser rejects verbatim, not slash-stripped', async () => {
    const { store, mockKiro } = createTestStore();
    const dottedPrompt: PromptEntry = {
      name: 'my.prompt',
      description: 'Dotted name',
      arguments: [],
      source: { kind: 'mcp', serverName: 'builder-mcp' },
    };
    store.setState({ prompts: [sopPrompt, dottedPrompt] });
    const history = CommandHistory.getInstance();
    history.clear();

    await store.getState().handleUserInput('@my.prompt some args');

    expect(streamedContents(mockKiro)).toEqual(['@my.prompt some args']);
    expect(history.getAll()).toEqual(['@my.prompt some args']);
  });

  it('records exactly one history entry per typed @prompt submission', async () => {
    const { store } = createTestStore();
    const history = CommandHistory.getInstance();
    history.clear();

    await store.getState().handleUserInput('@agent-sop:pdd');

    expect(history.getAll()).toEqual(['/agent-sop:pdd']);
  });

  it('queues a typed @prompt verbatim while processing, then normalizes on drain', async () => {
    const { store, mockKiro } = createTestStore();
    store.setState({
      isProcessing: true,
      sessionId: 'test-session',
      activeInterruptMode: InterruptMode.QUEUE,
    });

    await store.getState().handleUserInput('@agent-sop:pdd from queue');

    expect(store.getState().queuedMessages).toEqual([
      '@agent-sop:pdd from queue',
    ]);
    expect(streamedContents(mockKiro)).toEqual([]);

    store.setState({ isProcessing: false });
    await store.getState().processQueue();

    expect(streamedContents(mockKiro)).toEqual(['/agent-sop:pdd from queue']);
    const userRows = store.getState().messages.filter((m) => m.role === 'user');
    expect(userRows.at(-1)?.content).toBe('@agent-sop:pdd from queue');
  });

  it('normalizes a pending steer @prompt when replayed as a fresh prompt', async () => {
    const { store, mockKiro } = createTestStore();
    store.setState({ pendingSteerContent: '@agent-sop:pdd steered' });

    await store.getState().processQueue();

    expect(streamedContents(mockKiro)).toEqual(['/agent-sop:pdd steered']);
    const userRows = store.getState().messages.filter((m) => m.role === 'user');
    expect(userRows.at(-1)?.content).toBe('@agent-sop:pdd steered');
  });

  it('drains queued non-prompt messages verbatim', async () => {
    const { store, mockKiro } = createTestStore();
    store.setState({
      isProcessing: true,
      sessionId: 'test-session',
      activeInterruptMode: InterruptMode.QUEUE,
    });

    await store.getState().handleUserInput('plain queued message');
    store.setState({ isProcessing: false });
    await store.getState().processQueue();

    expect(streamedContents(mockKiro)).toEqual(['plain queued message']);
  });
});
