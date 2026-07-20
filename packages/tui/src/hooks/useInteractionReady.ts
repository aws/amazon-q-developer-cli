import { useEffect, useRef, useState } from 'react';
import { useKeypress } from './useKeypress.js';

export const INTERACTION_IDLE_MS = 2000;

export function useInteractionReady(
  pendingInteraction: object | null | undefined
): boolean {
  const lastKeypressRef = useRef(0);
  const [ready, setReady] = useState(true);
  const visibleRef = useRef(false);

  useKeypress(() => {
    // A visible interaction owns this key; do not delay the next queued item.
    if (visibleRef.current) return;
    lastKeypressRef.current = Date.now();
  });

  useEffect(() => {
    if (!pendingInteraction || ready) return;
    if (Date.now() - lastKeypressRef.current >= INTERACTION_IDLE_MS) {
      setReady(true);
      return;
    }
    const timer = setInterval(() => {
      if (Date.now() - lastKeypressRef.current >= INTERACTION_IDLE_MS) {
        setReady(true);
        clearInterval(timer);
      }
    }, 300);
    return () => clearInterval(timer);
  }, [pendingInteraction, ready]);

  useEffect(() => {
    if (!pendingInteraction) {
      setReady(true);
      return;
    }
    if (Date.now() - lastKeypressRef.current < INTERACTION_IDLE_MS) {
      setReady(false);
    }
  }, [pendingInteraction]);

  visibleRef.current = !!pendingInteraction && ready;
  return ready;
}
