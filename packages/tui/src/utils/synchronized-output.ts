/**
 * Synchronized Output for flicker-free terminal updates
 * Implements the synchronized output protocol for supported terminals
 * Spec: https://gist.github.com/christianparpart/d8a62cc1ab659194337d73e399004036
 */

const BSU = '\x1b[?2026h'; // Begin Synchronized Update
const ESU = '\x1b[?2026l'; // End Synchronized Update

let isSupported: boolean | null = null;

export function detectSynchronizedOutput(): boolean {
  if (isSupported !== null) {
    return isSupported;
  }

  if (!process.stdout.isTTY) {
    isSupported = false;
    return false;
  }

  // Detect terminals with synchronized output support
  const termProgram = process.env.TERM_PROGRAM;
  const term = process.env.TERM;

  isSupported = !!(
    termProgram === 'iTerm.app' ||
    termProgram === 'Alacritty' ||
    termProgram === 'WezTerm' ||
    termProgram === 'contour' ||
    termProgram === 'foot' ||
    term?.includes('kitty')
  );

  return isSupported;
}

export function beginSynchronizedUpdate(): void {
  if (detectSynchronizedOutput()) {
    process.stdout.write(BSU);
  }
}

export function endSynchronizedUpdate(): void {
  if (detectSynchronizedOutput()) {
    process.stdout.write(ESU);
  }
}

export function withSynchronizedUpdate<T>(fn: () => T): T {
  beginSynchronizedUpdate();
  try {
    return fn();
  } finally {
    endSynchronizedUpdate();
  }
}

// Auto-sync implementation
let originalWrite: typeof process.stdout.write | null = null;
let isAutoSyncEnabled = false;

export function enableAutoSync(): void {
  if (isAutoSyncEnabled || !detectSynchronizedOutput()) {
    return;
  }

  originalWrite = process.stdout.write.bind(process.stdout);
  isAutoSyncEnabled = true;

  process.stdout.write = function (chunk: any, encoding?: any, callback?: any): boolean {
    const content = chunk.toString();
    if (content.length > 5000) {
      beginSynchronizedUpdate();
      const result = originalWrite!(chunk, encoding, callback);
      setImmediate(() => endSynchronizedUpdate());
      return result;
    }

    return originalWrite!(chunk, encoding, callback);
  };
}

export function disableAutoSync(): void {
  if (!isAutoSyncEnabled || !originalWrite) {
    return;
  }

  process.stdout.write = originalWrite;
  originalWrite = null;
  isAutoSyncEnabled = false;
}
