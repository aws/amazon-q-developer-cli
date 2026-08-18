import { useLayoutEffect, useRef } from 'react';
import { useAppStore } from '../stores/app-store.js';

/**
 * Restore the prompt buffer a queued slash command displaced.
 *
 * Keyed on the picker-close edge rather than the picker's own close handlers so
 * that Esc-dismissal and selection are both covered without either handler
 * knowing a queue drain happened. useLayoutEffect, not useEffect: the restore
 * has to commit in the same frame the picker disappears, or the empty input
 * paints first and the text visibly flickers back. The ref narrows it to the
 * non-null → null transition, since the effect also runs when a picker opens.
 */
export function useQueuedInputRestore(): void {
  const activeCommand = useAppStore((s) => s.activeCommand);
  const queuedInputRestore = useAppStore((s) => s.queuedInputRestore);
  const applyQueuedInputRestore = useAppStore((s) => s.applyQueuedInputRestore);

  const prevActiveCommandRef = useRef(activeCommand);
  useLayoutEffect(() => {
    const prev = prevActiveCommandRef.current;
    prevActiveCommandRef.current = activeCommand;
    if (prev != null && activeCommand == null && queuedInputRestore != null) {
      applyQueuedInputRestore();
    }
  }, [activeCommand, queuedInputRestore, applyQueuedInputRestore]);
}
