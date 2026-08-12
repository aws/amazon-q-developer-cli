import { describe, expect, it, mock } from 'bun:test';
import { decideSend } from '../send-decision.js';
import { anchorFor, type ReviewAction } from '../review-actions.js';
import { createAppStore } from '../../../stores/app-store.js';

const LINES = ['## Glossary', '- **Duration**: an amount of time'];

const staged: ReviewAction[] = [
  {
    kind: 'comment',
    id: 'a',
    anchor: anchorFor(LINES, { start: 1, end: 1 }),
    body: 'drop the hour field',
  },
];

describe('decideSend', () => {
  it('composes the tagged request and a readable summary', () => {
    const decision = decideSend('requirements', staged, { busy: false });

    expect(decision.kind).toBe('send');
    if (decision.kind !== 'send') throw new Error('expected send');
    expect(decision.document).toBe('requirements');
    expect(decision.request).toContain('<comment on="Glossary"');
    expect(decision.request).toContain('Revise requirements.md');
    // The transcript stands in for the request, so it carries none of the tags.
    expect(decision.summary).not.toContain('<comment');
    expect(decision.summary).toContain('drop the hour field');
  });

  it('refuses while the agent is busy and says the comments are kept', () => {
    const decision = decideSend('requirements', staged, { busy: true });

    expect(decision.kind).toBe('refuse');
    if (decision.kind !== 'refuse') throw new Error('expected refuse');
    expect(decision.message).toContain('1 comment still staged');
    expect(decision.message).toContain('press S again');
  });

  it('has nothing to do without comments', () => {
    expect(decideSend('requirements', [], { busy: false }).kind).toBe(
      'nothing'
    );
    // Busy is irrelevant when there is nothing to send.
    expect(decideSend('requirements', [], { busy: true }).kind).toBe('nothing');
  });
});

/**
 * The busy check exists because of what the real store does with a message sent
 * mid-turn, so it is pinned against the real store rather than a mocked send.
 */
describe('what the store does with a send while a turn is in flight', () => {
  function makeStore() {
    const kiro = {
      setConfigOption: mock(() => Promise.resolve()),
      isCloudSessionActive: () => false,
      sendChatSlashCommandTelemetry: mock(),
      close: mock(),
    } as never;
    const store = createAppStore({ kiro });
    store.setState({ isInitialized: true, sessionId: 's' });
    return store;
  }

  it('keeps only the text the transcript shows, dropping the request', async () => {
    const store = makeStore();
    const queued: string[] = [];
    store.setState({
      isProcessing: true,
      queueMessage: ((text: string) => {
        queued.push(text);
      }) as never,
    });

    await store
      .getState()
      .sendMessage('TAGGED REQUEST', undefined, 'READABLE SUMMARY');

    // This is why `decideSend` refuses: the request never reaches the agent, and
    // nothing about the call reports that it didn't.
    expect(queued).toEqual(['READABLE SUMMARY']);
  });

  it('is the branch the busy check covers', () => {
    const store = makeStore();
    for (const state of [
      { isProcessing: true },
      { isCompacting: true },
      { loadingMessage: 'working' },
      { isInitialized: false },
    ]) {
      const probe = makeStore();
      probe.setState(state as never);
      const s = probe.getState();
      const busy =
        !s.isInitialized ||
        s.isProcessing ||
        s.isCompacting ||
        !!s.loadingMessage;
      expect(busy, JSON.stringify(state)).toBe(true);
    }
    // A settled session is not busy, so an ordinary send goes as written.
    const idle = store.getState();
    expect(
      !idle.isInitialized ||
        idle.isProcessing ||
        idle.isCompacting ||
        !!idle.loadingMessage
    ).toBe(false);
  });
});
