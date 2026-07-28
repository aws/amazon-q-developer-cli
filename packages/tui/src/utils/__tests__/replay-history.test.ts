import { describe, it, expect, mock } from 'bun:test';
import { replayBufferedHistory } from '../replay-history';
import {
  AgentEventType,
  type AgentStreamEvent,
} from '../../types/agent-events';

/** Build `n` single-event user turns (each UserMessage starts a new turn). */
function userTurns(n: number): AgentStreamEvent[] {
  return Array.from(
    { length: n },
    () => ({ type: AgentEventType.UserMessage }) as AgentStreamEvent
  );
}

function mockReplayCtx() {
  const handler = Object.assign(
    mock((_e: AgentStreamEvent) => {}),
    {
      flush: mock(() => {}),
    }
  );
  const createStreamEventHandler = mock(() => handler);
  const addSystemMessage = mock((_c: string, _s: boolean) => {});
  return {
    ctx: { addSystemMessage, createStreamEventHandler },
    handler,
    addSystemMessage,
    createStreamEventHandler,
  };
}

describe('replayBufferedHistory', () => {
  it('emits the omitted-turns marker and replays only the last N turns when over the cap', () => {
    const { ctx, handler, addSystemMessage } = mockReplayCtx();

    replayBufferedHistory(ctx as any, userTurns(15), 10);

    expect(addSystemMessage).toHaveBeenCalledTimes(1);
    expect(addSystemMessage.mock.calls[0]![0]).toBe(
      '⋯ 5 earlier turns not shown'
    );
    expect(addSystemMessage.mock.calls[0]![1]).toBe(true);
    // only the last 10 turns are replayed
    expect(handler.mock.calls.length).toBe(10);
  });

  it('replays with fromHistory so old tool rows keep no fresh timing chips', () => {
    const { ctx, createStreamEventHandler } = mockReplayCtx();

    replayBufferedHistory(ctx as any, userTurns(3), 10);

    expect(createStreamEventHandler).toHaveBeenCalledWith({
      fromHistory: true,
    });
  });

  it('does not emit a marker when turns fit within the cap', () => {
    const { ctx, handler, addSystemMessage } = mockReplayCtx();

    replayBufferedHistory(ctx as any, userTurns(5), 10);

    expect(addSystemMessage).not.toHaveBeenCalled();
    expect(handler.mock.calls.length).toBe(5);
  });

  it('uses the singular "turn" when exactly one turn is omitted', () => {
    const { ctx, addSystemMessage } = mockReplayCtx();

    replayBufferedHistory(ctx as any, userTurns(11), 10);

    expect(addSystemMessage.mock.calls[0]![0]).toBe(
      '⋯ 1 earlier turn not shown'
    );
  });

  it('flushes the stream handler after replay', () => {
    const { ctx, handler } = mockReplayCtx();

    replayBufferedHistory(ctx as any, userTurns(3), 10);

    expect(handler.flush).toHaveBeenCalledTimes(1);
  });

  it('does nothing for an empty buffer (no handler, no marker)', () => {
    const { ctx, addSystemMessage, createStreamEventHandler } = mockReplayCtx();

    replayBufferedHistory(ctx as any, [], 10);

    expect(addSystemMessage).not.toHaveBeenCalled();
    expect(createStreamEventHandler).not.toHaveBeenCalled();
  });
});
