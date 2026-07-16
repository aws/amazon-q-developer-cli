import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';
import { ASCII_GLYPHS } from '../glyphs';
import {
  formatCloudDetachNotice,
  emitCloudDetachNoticeOnce,
  hasEmittedCloudDetachNotice,
  suppressCloudDetachNotice,
  resetCloudDetachNoticeForTest,
  quitCloudSessionKeepRunning,
  quitCloudSessionTurnOff,
} from '../cloud-detach-notice';

describe('formatCloudDetachNotice', () => {
  it('confirms the quit and that work continues, surfacing the session id', () => {
    const msg = formatCloudDetachNotice('sess-abc123');
    expect(msg).toContain('sess-abc123');
    expect(msg).toContain('Quit session');
    expect(msg).toContain("Your work continues while you're away.");
  });

  it('includes the exact session id (so the user can find it again)', () => {
    const msg = formatCloudDetachNotice('spc-9f2');
    expect(msg).toContain('Quit session spc-9f2');
  });

  it('routes the leading checkmark through the glyph set (ASCII falls back)', () => {
    expect(formatCloudDetachNotice('s1')).toStartWith('✓ ');
    expect(formatCloudDetachNotice('s1', ASCII_GLYPHS)).toStartWith('+ ');
  });
});

describe('emitCloudDetachNoticeOnce', () => {
  let written: string[];
  let origWrite: typeof process.stderr.write;

  beforeEach(() => {
    resetCloudDetachNoticeForTest();
    written = [];
    origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = mock((chunk: string | Uint8Array) => {
      written.push(String(chunk));
      return true;
    }) as typeof process.stderr.write;
  });

  afterEach(() => {
    process.stderr.write = origWrite;
    resetCloudDetachNoticeForTest();
  });

  it('prints exactly once across overlapping exit paths', () => {
    emitCloudDetachNoticeOnce('sess-1');
    emitCloudDetachNoticeOnce('sess-1');
    expect(written.filter((w) => w.includes('Quit session'))).toHaveLength(1);
  });

  it('is a no-op for a falsy session id (local exits unaffected)', () => {
    emitCloudDetachNoticeOnce(null);
    emitCloudDetachNoticeOnce(undefined);
    expect(written).toHaveLength(0);
  });

  it('prints nothing after suppress — a turn-off must not claim work continues', () => {
    suppressCloudDetachNotice();
    emitCloudDetachNoticeOnce('sess-1');
    expect(written).toHaveLength(0);
  });
});

describe('hasEmittedCloudDetachNotice', () => {
  beforeEach(() => resetCloudDetachNoticeForTest());
  afterEach(() => resetCloudDetachNoticeForTest());

  it('is false before emission and true after — so the exit epilogue can honor a detach even after Kiro.close()', () => {
    const origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = mock(() => true) as typeof process.stderr.write;
    try {
      expect(hasEmittedCloudDetachNotice()).toBe(false);
      emitCloudDetachNoticeOnce('sess-1');
      expect(hasEmittedCloudDetachNotice()).toBe(true);
    } finally {
      process.stderr.write = origWrite;
    }
  });

  it('is true after suppress (turn-off) so no stray epilogue re-emits', () => {
    suppressCloudDetachNotice();
    expect(hasEmittedCloudDetachNotice()).toBe(true);
  });
});

describe('quitCloudSessionKeepRunning', () => {
  let written: string[];
  let origWrite: typeof process.stderr.write;

  beforeEach(() => {
    resetCloudDetachNoticeForTest();
    written = [];
    origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = mock((chunk: string | Uint8Array) => {
      written.push(String(chunk));
      return true;
    }) as typeof process.stderr.write;
  });

  afterEach(() => {
    process.stderr.write = origWrite;
    resetCloudDetachNoticeForTest();
  });

  it('emits the notice, then closes, then exits with 0', () => {
    const order: string[] = [];
    const kiro = {
      sessionId: 'sess-kr',
      close: mock(() => order.push('close')),
    };
    const exit = mock((code: number) => order.push(`exit:${code}`));
    quitCloudSessionKeepRunning(kiro, exit);
    expect(written.some((w) => w.includes('Quit session sess-kr'))).toBe(true);
    expect(order).toEqual(['close', 'exit:0']);
  });
});

describe('quitCloudSessionTurnOff', () => {
  let written: string[];
  let origWrite: typeof process.stderr.write;

  beforeEach(() => {
    resetCloudDetachNoticeForTest();
    written = [];
    origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = mock((chunk: string | Uint8Array) => {
      written.push(String(chunk));
      return true;
    }) as typeof process.stderr.write;
  });

  afterEach(() => {
    process.stderr.write = origWrite;
    resetCloudDetachNoticeForTest();
  });

  it('suppresses the notice before cancel settles — a mid-cancel signal prints nothing', async () => {
    let settle!: () => void;
    const kiro = {
      cancel: mock(() => new Promise<void>((r) => (settle = r))),
      close: mock(() => {}),
    };
    const exit = mock((_code: number) => {});
    quitCloudSessionTurnOff(kiro, exit);
    // Signal handler firing while the cancel is still in flight.
    emitCloudDetachNoticeOnce('sess-to');
    expect(written).toHaveLength(0);
    settle();
    await Promise.resolve();
  });

  it('waits for cancel to settle, then closes, then exits with 0', async () => {
    const order: string[] = [];
    let settle!: () => void;
    const kiro = {
      cancel: mock(() => {
        order.push('cancel');
        return new Promise<void>((r) => (settle = r));
      }),
      close: mock(() => order.push('close')),
    };
    const exit = mock((code: number) => order.push(`exit:${code}`));
    quitCloudSessionTurnOff(kiro, exit);
    expect(order).toEqual(['cancel']);
    settle();
    await Promise.resolve();
    await Promise.resolve();
    expect(order).toEqual(['cancel', 'close', 'exit:0']);
  });

  it('still closes and exits when cancel rejects', async () => {
    const order: string[] = [];
    const kiro = {
      cancel: mock(() => Promise.reject(new Error('kas gone'))),
      close: mock(() => order.push('close')),
    };
    const exit = mock((code: number) => order.push(`exit:${code}`));
    quitCloudSessionTurnOff(kiro, exit);
    await Promise.resolve();
    await Promise.resolve();
    expect(order).toEqual(['close', 'exit:0']);
  });

  it('still exits when close() throws — a turn-off must never leave the process alive', async () => {
    const kiro = {
      cancel: mock(() => Promise.resolve()),
      close: mock(() => {
        throw new Error('already closed');
      }),
    };
    const exit = mock((_code: number) => {});
    quitCloudSessionTurnOff(kiro, exit);
    await Promise.resolve();
    await Promise.resolve();
    expect(kiro.close).toHaveBeenCalled();
    expect(exit).toHaveBeenCalledWith(0);
  });
});
