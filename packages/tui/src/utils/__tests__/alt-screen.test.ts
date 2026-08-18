import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  enterAltScreen,
  leaveAltScreen,
  isAltScreenActive,
  markAltScreenExited,
} from '../alt-screen';

const SMCUP = '\x1b[?1049h';
const RMCUP = '\x1b[?1049l';
const RESET_SGR = '\x1b[0m';

describe('alt-screen', () => {
  let writes: string[];
  let originalWrite: typeof process.stdout.write;

  beforeEach(() => {
    writes = [];
    originalWrite = process.stdout.write.bind(process.stdout);
    process.stdout.write = ((chunk: unknown) => {
      writes.push(typeof chunk === 'string' ? chunk : String(chunk));
      return true;
    }) as typeof process.stdout.write;
    markAltScreenExited();
  });

  afterEach(() => {
    process.stdout.write = originalWrite;
    markAltScreenExited();
  });

  it('tracks active state across enter/leave', () => {
    expect(isAltScreenActive()).toBe(false);
    enterAltScreen();
    expect(isAltScreenActive()).toBe(true);
    leaveAltScreen();
    expect(isAltScreenActive()).toBe(false);
  });

  it('enterAltScreen writes SMCUP', () => {
    enterAltScreen();
    expect(writes.some((w) => w.includes(SMCUP))).toBe(true);
  });

  it('leaveAltScreen writes the SGR reset AFTER RMCUP', () => {
    enterAltScreen();
    writes.length = 0;
    leaveAltScreen();

    const rmcupIdx = writes.findIndex((w) => w.includes(RMCUP));
    const resetIdx = writes.findIndex((w) => w.includes(RESET_SGR));

    expect(rmcupIdx).toBeGreaterThanOrEqual(0);
    expect(resetIdx).toBeGreaterThanOrEqual(0);
    // RMCUP restores the graphic rendition saved at SMCUP, so a reset written
    // before it would be undone and re-leak the attribute. The reset must land
    // strictly after the alt-screen exit.
    expect(resetIdx).toBeGreaterThan(rmcupIdx);
  });
});
