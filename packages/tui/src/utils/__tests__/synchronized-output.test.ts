import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { resetCapabilityCache } from '../terminal-capabilities';
import {
  detectSynchronizedOutput,
  beginSynchronizedUpdate,
  endSynchronizedUpdate,
  withSynchronizedUpdate,
  enableAutoSync,
  disableAutoSync,
} from '../synchronized-output';

const BSU = '\x1b[?2026h';
const ESU = '\x1b[?2026l';

let writtenData: string[];
let originalWrite: typeof process.stdout.write;
let savedEnv: NodeJS.ProcessEnv;
let originalIsTTY: boolean | undefined;

function enableSyncCapability() {
  // iTerm.app supports synchronized output
  process.env.TERM_PROGRAM = 'iTerm.app';
  Object.defineProperty(process.stdout, 'isTTY', {
    value: true,
    writable: true,
    configurable: true,
  });
  resetCapabilityCache();
}

function disableSyncCapability() {
  // Non-TTY means no capabilities
  delete process.env.TERM_PROGRAM;
  delete process.env.TERM;
  delete process.env.TMUX;
  Object.defineProperty(process.stdout, 'isTTY', {
    value: false,
    writable: true,
    configurable: true,
  });
  resetCapabilityCache();
}

beforeEach(() => {
  savedEnv = { ...process.env };
  originalIsTTY = process.stdout.isTTY;
  writtenData = [];
  originalWrite = process.stdout.write;
  process.stdout.write = ((chunk: any) => {
    writtenData.push(String(chunk));
    return true;
  }) as any;
  // Default: capability enabled
  enableSyncCapability();
  disableAutoSync();
});

afterEach(() => {
  disableAutoSync();
  process.stdout.write = originalWrite;
  process.env = savedEnv;
  Object.defineProperty(process.stdout, 'isTTY', {
    value: originalIsTTY,
    writable: true,
    configurable: true,
  });
  resetCapabilityCache();
});

describe('detectSynchronizedOutput', () => {
  it('returns true when capable', () => {
    enableSyncCapability();
    expect(detectSynchronizedOutput()).toBe(true);
  });

  it('returns false when not capable', () => {
    disableSyncCapability();
    expect(detectSynchronizedOutput()).toBe(false);
  });
});

describe('beginSynchronizedUpdate', () => {
  it('writes BSU when capable', () => {
    enableSyncCapability();
    beginSynchronizedUpdate();
    expect(writtenData).toContain(BSU);
  });

  it('writes nothing when not capable', () => {
    disableSyncCapability();
    beginSynchronizedUpdate();
    expect(writtenData).toEqual([]);
  });
});

describe('endSynchronizedUpdate', () => {
  it('writes ESU when capable', () => {
    enableSyncCapability();
    endSynchronizedUpdate();
    expect(writtenData).toContain(ESU);
  });

  it('writes nothing when not capable', () => {
    disableSyncCapability();
    endSynchronizedUpdate();
    expect(writtenData).toEqual([]);
  });
});

describe('withSynchronizedUpdate', () => {
  it('calls fn and returns its result', () => {
    const result = withSynchronizedUpdate(() => 42);
    expect(result).toBe(42);
  });

  it('writes BSU before and ESU after fn', () => {
    let seenDuringFn: string[] = [];
    withSynchronizedUpdate(() => {
      seenDuringFn = [...writtenData];
    });
    // BSU should have been written before fn ran
    expect(seenDuringFn).toContain(BSU);
    // ESU should be written after fn returned
    expect(writtenData).toContain(ESU);
    // BSU should come before ESU
    const bsuIndex = writtenData.indexOf(BSU);
    const esuIndex = writtenData.indexOf(ESU);
    expect(bsuIndex).toBeLessThan(esuIndex);
  });

  it('writes ESU even when fn throws', () => {
    expect(() => {
      withSynchronizedUpdate(() => {
        throw new Error('boom');
      });
    }).toThrow('boom');
    expect(writtenData).toContain(ESU);
  });
});

describe('enableAutoSync', () => {
  it('wraps large writes (>5000 chars) with BSU/ESU', () => {
    enableAutoSync();
    const bigContent = 'x'.repeat(6000);
    process.stdout.write(bigContent);
    // Our interceptor should see BSU, then the big content
    expect(writtenData).toContain(BSU);
    expect(writtenData.some((d) => d.includes('x'.repeat(100)))).toBe(true);
  });

  it('does not wrap small writes', () => {
    enableAutoSync();
    const smallContent = 'hello';
    process.stdout.write(smallContent);
    expect(writtenData).not.toContain(BSU);
    expect(writtenData.some((d) => d.includes('hello'))).toBe(true);
  });

  it('is a no-op when not capable', () => {
    disableSyncCapability();
    const writeBefore = process.stdout.write;
    enableAutoSync();
    // stdout.write should not have been patched
    expect(process.stdout.write).toBe(writeBefore);
  });

  it('is idempotent -- second call does not double-patch', () => {
    enableAutoSync();
    const writeAfterFirst = process.stdout.write;
    enableAutoSync();
    expect(process.stdout.write).toBe(writeAfterFirst);
  });
});

describe('disableAutoSync', () => {
  it('restores original write after enableAutoSync', () => {
    enableAutoSync();
    const patchedWrite = process.stdout.write;
    disableAutoSync();
    // After disabling, the write function should differ from the patched version
    expect(process.stdout.write).not.toBe(patchedWrite);
    // Verify writes go through without BSU wrapping
    writtenData = [];
    const bigContent = 'y'.repeat(6000);
    process.stdout.write(bigContent);
    expect(writtenData).not.toContain(BSU);
  });

  it('is a no-op when not enabled', () => {
    const writeBefore = process.stdout.write;
    disableAutoSync();
    expect(process.stdout.write).toBe(writeBefore);
  });
});
