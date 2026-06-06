/**
 * Lite-local hook for tracking an in-flight `/agent <name>` swap.
 *
 * The dispatcher writes "Agent changing to <name>" into `loadingMessage` before
 * issuing the executeCommand RPC and clears it on response. We latch onto that
 * string and hold the pending state until either:
 *   - `currentAgent.name` actually moves off its value at swap-issue time, OR
 *   - 30s elapses (safety timeout in case the RPC silently never resolves).
 *
 * Why latch instead of derive each render: a *second* slash command landing
 * mid-swap (e.g. /agent's options-fetch writes "Loading agent options...")
 * overwrites loadingMessage, which would make a pure-derived footer flicker
 * back to the still-old `currentAgent` until the swap settled.
 *
 * Shared between LiteLayout (footer chip) and LiteLiveRegion (so the live
 * region can render "queued — waiting for agent switch" instead of a
 * misleading "thinking" while the agent isn't actually responding yet).
 */
import { useEffect, useState } from 'react';
import { useAppStore } from '../../../stores/app-store.js';

export interface PendingSwap {
  name: string;
  baseAgent: string | null;
}

export function usePendingSwap(): PendingSwap | null {
  const loadingMessage = useAppStore((s) => s.loadingMessage);
  const currentAgent = useAppStore((s) => s.currentAgent);
  const [pendingSwap, setPendingSwap] = useState<PendingSwap | null>(null);

  useEffect(() => {
    if (!loadingMessage) return;
    const match = /^Agent changing to (.+)$/.exec(loadingMessage);
    if (!match) return;
    const target = match[1]!.trim();
    setPendingSwap((prev) => ({
      name: target,
      // Preserve the *original* base across re-issued swaps so we still know
      // when the backend has truly settled (not just rotated to an interim).
      baseAgent: prev?.baseAgent ?? currentAgent?.name ?? null,
    }));
  }, [loadingMessage, currentAgent]);

  useEffect(() => {
    if (!pendingSwap) return;
    if ((currentAgent?.name ?? null) !== pendingSwap.baseAgent) {
      setPendingSwap(null);
    }
  }, [currentAgent, pendingSwap]);

  useEffect(() => {
    if (!pendingSwap) return;
    const t = setTimeout(() => setPendingSwap(null), 30_000);
    return () => clearTimeout(t);
  }, [pendingSwap]);

  return pendingSwap;
}
