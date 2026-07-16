import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';
import { createAppStore } from '../app-store';
import { resetCloudDetachNoticeForTest } from '../../utils/cloud-detach-notice';

// The prompt-level /disconnect interception fires mid-turn (inside the
// "queue while processing" branch) so a detach isn't stranded behind a turn it
// doesn't interrupt. It calls process.exit directly, so the test stubs exit
// (throwing to halt fall-through) and stderr to assert the detach sequence.
describe('handleUserInput /disconnect (mid-turn prompt path)', () => {
  let written: string[];
  let origWrite: typeof process.stderr.write;
  let origExit: typeof process.exit;
  let exitCalls: number[];

  beforeEach(() => {
    resetCloudDetachNoticeForTest();
    written = [];
    exitCalls = [];
    origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = mock((chunk: string | Uint8Array) => {
      written.push(String(chunk));
      return true;
    }) as typeof process.stderr.write;
    origExit = process.exit;
    process.exit = mock((code?: number) => {
      exitCalls.push(code ?? 0);
      throw new Error('__EXIT__');
    }) as typeof process.exit;
  });

  afterEach(() => {
    process.stderr.write = origWrite;
    process.exit = origExit;
    resetCloudDetachNoticeForTest();
  });

  it('cloud-active: detaches (notice + close + onExit + exit 0) instead of queueing', async () => {
    const order: string[] = [];
    const close = mock(() => order.push('close'));
    const onExit = mock(() => order.push('onExit'));
    const kiro = {
      sessionId: 'sess-mid',
      close,
      isCloudSessionActive: () => true,
    };
    const store = createAppStore({ kiro: kiro as any, agentEngine: 'kas' });
    store.setState({ isProcessing: true, isInitialized: true, onExit });

    try {
      await store.getState().handleUserInput('/disconnect');
    } catch (err) {
      expect((err as Error).message).toBe('__EXIT__');
    }

    expect(written.some((w) => w.includes('Quit session sess-mid'))).toBe(true);
    expect(order).toEqual(['close', 'onExit']);
    expect(exitCalls).toEqual([0]);
  });

  it('not cloud-active: /disconnect does not detach mid-turn', async () => {
    const close = mock(() => {});
    const kiro = {
      sessionId: 'sess-mid',
      close,
      isCloudSessionActive: () => false,
    };
    const store = createAppStore({ kiro: kiro as any, agentEngine: 'kas' });
    store.setState({ isProcessing: true, isInitialized: true });

    await store.getState().handleUserInput('/disconnect');

    expect(exitCalls).toHaveLength(0);
    expect(close).not.toHaveBeenCalled();
    expect(written).toHaveLength(0);
  });
});
