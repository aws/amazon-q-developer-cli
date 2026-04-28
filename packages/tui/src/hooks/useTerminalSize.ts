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
    const width = process.stdout.columns || 60;
    const height = process.stdout.rows || 20;
    // Skip invalid dimensions (iTerm can report 0 during transitions)
    if (width < 1 || height < 1) return;
    // Skip extremely small dimensions — yoga layout can spiral
    if (width < 10) return;
    // Skip if dimensions haven't changed — avoids re-rendering 21 components
    // on no-op SIGWINCH (tmux pane focus, attach/detach, scrollbar oscillation).
    if (width === currentSize.width && height === currentSize.height) return;
    currentSize = { width, height };
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
