import { describe, it, expect } from 'bun:test';

describe('graceful-exit', () => {
  it('registerInstance and isExiting work', async () => {
    const mod = await import('../utils/graceful-exit');
    expect(mod.isExiting()).toBe(false);
    mod.registerInstance(undefined, () => {});
  });
});

describe('acp-recorder', () => {
  it('maybeWrapStreamWithRecorder returns same stream when not recording', async () => {
    // Only run if KIRO_RECORD_ACP is not set (default)
    if (process.env.KIRO_RECORD_ACP) return;
    const mod = await import('../acp-recorder');
    const fakeStream = {
      readable: new ReadableStream(),
      writable: new WritableStream(),
    };
    const result = mod.maybeWrapStreamWithRecorder(fakeStream as any);
    expect(result).toBe(fakeStream);
  });
});
