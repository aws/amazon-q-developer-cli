import { describe, it, expect, mock } from 'bun:test';
import { createAppStore, MessageRole, type MessageType } from '../app-store';
import { AgentEventType, ContentType } from '../../types/agent-events';
import { Kiro } from '../../kiro';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

describe('Streaming flush performance', () => {
  function createStoreWithHistory(turnCount: number) {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });
    store.setState({ isInitialized: true });

    const messages: MessageType[] = [];
    for (let i = 0; i < turnCount; i++) {
      messages.push({
        id: `u${i}`,
        role: MessageRole.User,
        content: `User message ${i}`,
      });
      messages.push({
        id: `m${i}`,
        role: MessageRole.Model,
        content: `Model response ${i} - `.repeat(20),
      });
    }
    // Active streaming turn
    messages.push({
      id: 'u-active',
      role: MessageRole.User,
      content: 'Tell me about computing history',
    });
    messages.push({
      id: 'm-active',
      role: MessageRole.Model,
      content: '',
    });

    store.setState({ messages });
    return store;
  }

  // Measure just the set() cost by forcing synchronous flushes via ToolCall
  // (ToolCall events flush pending content synchronously before adding the tool msg)
  function benchmarkFlush(label: string, turnCount: number) {
    it(label, () => {
      const store = createStoreWithHistory(turnCount);
      const msgCount = store.getState().messages.length;
      const iterations = 500;

      // We directly measure the cost of the store's set() during content flush.
      // To avoid setTimeout, we buffer content then trigger commitBufferedContent
      // by calling startBuffering + sending content + stopBuffering.
      const handler = store.getState().createStreamEventHandler();
      const { startBuffering, stopBuffering } =
        store.getState().streamingBuffer;
      if (!startBuffering || !stopBuffering)
        throw new Error('expected streamingBuffer methods');

      // Warm up
      for (let i = 0; i < 10; i++) {
        startBuffering();
        handler({
          type: AgentEventType.Content,
          id: 'm-active',
          content: { type: ContentType.Text, text: `w${i} ` },
        });
        stopBuffering();
      }

      // Benchmark: measure just the flush (startBuffer → content → stopBuffer)
      const start = performance.now();
      for (let i = 0; i < iterations; i++) {
        startBuffering();
        handler({
          type: AgentEventType.Content,
          id: 'm-active',
          content: { type: ContentType.Text, text: `t${i} ` },
        });
        stopBuffering(); // triggers synchronous set()
      }
      const elapsed = performance.now() - start;

      const avgUs = (elapsed / iterations) * 1000;
      console.log(
        `\n[${label}] ${msgCount} msgs, ${iterations} flushes: ` +
          `${elapsed.toFixed(1)}ms total, ${avgUs.toFixed(1)}µs/flush`
      );

      const lastMsg = store.getState().messages.at(-1);
      expect(lastMsg?.role).toBe(MessageRole.Model);
      expect(lastMsg?.content).toContain(`t${iterations - 1}`);
    });
  }

  benchmarkFlush('50 turns (102 msgs)', 50);
  benchmarkFlush('250 turns (502 msgs)', 250);
  benchmarkFlush('1000 turns (2002 msgs)', 1000);
});
