import { useSyncExternalStore } from 'react';

// Shared terminal size state — a single resize listener updates all subscribers.
let currentSize = {
  width: process.stdout.columns || 60,
  height: process.stdout.rows || 20,
};
const listeners = new Set<() => void>();
let resizeListenerInstalled = false;

function installResizeListener() {
  if (resizeListenerInstalled) return;
  resizeListenerInstalled = true;

  process.stdout.on('resize', () => {
    currentSize = {
      width: process.stdout.columns || 60,
      height: process.stdout.rows || 20,
    };
    // Notify all subscribers so React re-renders with new dimensions
    for (const listener of listeners) {
      listener();
    }
  });
}

function subscribe(callback: () => void): () => void {
  installResizeListener();
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
