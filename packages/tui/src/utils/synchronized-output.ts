/**
 * Synchronized Output for flicker-free terminal updates
 * Implements the synchronized output protocol for supported terminals
 * Spec: https://gist.github.com/christianparpart/d8a62cc1ab659194337d73e399004036
 */

import { hasCapability } from './terminal-capabilities.js';

const BSU = '\x1b[?2026h'; // Begin Synchronized Update
const ESU = '\x1b[?2026l'; // End Synchronized Update

export function detectSynchronizedOutput(): boolean {
  return hasCapability('synchronizedOutput');
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

  process.stdout.write = function (
    chunk: any,
    encoding?: any,
    callback?: any
  ): boolean {
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
