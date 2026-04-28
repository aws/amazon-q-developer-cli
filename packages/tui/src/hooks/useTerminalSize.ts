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
    const newWidth = process.stdout.columns || 60;
    const newHeight = process.stdout.rows || 20;
    // Skip invalid dimensions (iTerm can report 0 during transitions)
    if (newWidth < 1 || newHeight < 1) return;
    // Skip if dimensions haven't changed (e.g. tmux pane focus, attach/detach)
    if (newWidth === currentSize.width && newHeight === currentSize.height)
      return;
    currentSize = { width: newWidth, height: newHeight };
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
