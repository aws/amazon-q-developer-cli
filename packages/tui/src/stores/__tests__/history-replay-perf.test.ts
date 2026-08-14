import { describe, it, expect, mock, afterAll } from 'bun:test';
import { AgentEventType, ContentType } from '../../types/agent-events';
import type { AgentStreamEvent } from '../../types/agent-events';
import { truncateToRecentTurns } from '../../utils/truncate-history';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../../kiro']);

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

import { createAppStore } from '../app-store';
import { Kiro } from '../../kiro';

/**
 * Simulates the resume path: generates N turns of history events
 * (user_message + content + tool_calls) and replays them into the store
 * via createStreamEventHandler — exactly as index.tsx does on resume.
 *
 * This reproduces the bug where a large session (e.g. 40+ turns with many
 * tool calls) causes 7000+ rendered lines and ~200ms per frame.
 */
function generateHistoryEvents(
  turnCount: number,
  toolsPerTurn: number = 5
): AgentStreamEvent[] {
  const events: AgentStreamEvent[] = [];
  for (let t = 0; t < turnCount; t++) {
    // User message
    events.push({
      type: AgentEventType.UserMessage,
      id: `user-${t}`,
      content: `User prompt for turn ${t} - ${'x'.repeat(100)}`,
    } as unknown as AgentStreamEvent);

    // Tool calls (the main source of rendered lines)
    for (let tc = 0; tc < toolsPerTurn; tc++) {
      events.push({
        type: AgentEventType.ToolCall,
        id: `tool-${t}-${tc}`,
        name: 'read',
        params: { path: `/src/file-${t}-${tc}.ts` },
      } as unknown as AgentStreamEvent);
      events.push({
        type: AgentEventType.ToolCallFinished,
        id: `tool-${t}-${tc}`,
        result: 'x'.repeat(500),
      } as unknown as AgentStreamEvent);
    }

    // Assistant content
    events.push({
      type: AgentEventType.Content,
      id: `content-${t}`,
      content: {
        type: ContentType.Text,
        text: `Response for turn ${t}\n${'Line of output\n'.repeat(20)}`,
      },
    } as AgentStreamEvent);
  }
  return events;
}

describe('History replay performance (resume path)', () => {
  it('replaying 40 turns with truncation should produce ≤10 turns of messages', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isInitialized: true });

    const allEvents = generateHistoryEvents(40, 5);
    // Apply truncation — same as the fixed index.tsx resume path
    const { events, omittedTurns } = truncateToRecentTurns(allEvents);

    expect(omittedTurns).toBe(30); // 40 - 10 = 30 omitted

    const handler = store.getState().createStreamEventHandler();
    const start = performance.now();
    for (const event of events) {
      handler(event);
    }
    (handler as any).flush?.();
    const elapsed = performance.now() - start;

    const messages = store.getState().messages;
    console.log(
      `\n[history-replay] ${allEvents.length} total events, ${events.length} after truncation → ${messages.length} messages in ${elapsed.toFixed(1)}ms`
    );

    // With truncation (MAX_DISPLAY_TURNS=10), only last 10 turns rendered
    expect(messages.length).toBeLessThanOrEqual(150);
    expect(elapsed).toBeLessThan(500);
  });

  it('replaying 100 turns with truncation should cap messages', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isInitialized: true });

    const allEvents = generateHistoryEvents(100, 8);
    const { events, omittedTurns } = truncateToRecentTurns(allEvents);

    expect(omittedTurns).toBe(90); // 100 - 10 = 90 omitted

    const handler = store.getState().createStreamEventHandler();
    const start = performance.now();
    for (const event of events) {
      handler(event);
    }
    (handler as any).flush?.();
    const elapsed = performance.now() - start;

    const messages = store.getState().messages;
    console.log(
      `\n[history-replay-large] ${allEvents.length} total events, ${events.length} after truncation → ${messages.length} messages in ${elapsed.toFixed(1)}ms`
    );

    expect(messages.length).toBeLessThanOrEqual(200);
    expect(elapsed).toBeLessThan(1000);
  });
});
