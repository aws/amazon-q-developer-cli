import { useSyncExternalStore } from 'react';
import { logger } from '../utils/logger.js';
import type { Instance } from 'twinki';

// Current terminal dimensions. Updated synchronously from Twinki's
// onResize callback, before Twinki's requestRender(force) fires.
let currentSize = {
  width: process.stdout.columns || 60,
  height: process.stdout.rows || 20,
};

const listeners = new Set<() => void>();
let registered = false;

function updateSize() {
  const newWidth = process.stdout.columns || 60;
  const newHeight = process.stdout.rows || 20;
  if (newWidth < 1 || newHeight < 1) return;
  if (newWidth === currentSize.width && newHeight === currentSize.height)
    return;
  currentSize = { width: newWidth, height: newHeight };
  logger.debug(`[resize] ${newWidth}x${newHeight}`);
  for (const listener of listeners) {
    listener();
  }
}

/**
 * Connect useTerminalSize to Twinki's resize callback.
 * Call once after render() returns. The onResize callback fires
 * synchronously before requestRender(force), so React sees the
 * correct dimensions in a single render pass.
 */
export function connectResizeSource(instance: Instance): void {
  if (registered) return;
  registered = true;
  instance.onResize(updateSize);
}

function subscribe(callback: () => void): () => void {
  listeners.add(callback);
  return () => {
    listeners.delete(callback);
  };
}

function getSnapshot(): { width: number; height: number } {
  return currentSize;
}

export function useTerminalSize(): { width: number; height: number } {
  return useSyncExternalStore(subscribe, getSnapshot);
}
