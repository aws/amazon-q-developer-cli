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

function setSize(newWidth: number, newHeight: number) {
  if (newWidth < 1 || newHeight < 1) return;
  if (newWidth === currentSize.width && newHeight === currentSize.height)
    return;
  currentSize = { width: newWidth, height: newHeight };
  logger.debug(`[resize] ${newWidth}x${newHeight}`);
  for (const listener of listeners) {
    listener();
  }
}

function updateSize() {
  setSize(process.stdout.columns || 60, process.stdout.rows || 20);
}

/** Keep render harness dimensions aligned with its mock terminal. */
export function setTerminalSizeForTests(width: number, height: number): void {
  setSize(width, height);
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
  // The module-load snapshot can predate the terminal being sized (or use
  // a stale stdout value); resize callbacks only fire on CHANGES, so sync
  // once now or a wrong initial height persists until a physical resize.
  updateSize();
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
