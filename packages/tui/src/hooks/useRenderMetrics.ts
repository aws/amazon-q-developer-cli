import { useState, useEffect } from 'react';

const IS_DEV = process.env.KIRO_DEV === '1' || process.env.NODE_ENV !== 'production';

interface RenderMetrics {
  lastRenderMs: number;
  renderCount: number;
  fullRedrawCount: number;
  yogaNodeCount: number;
  heapUsedMB: number;
}

/**
 * Polls twinki render metrics every second.
 * Only active when KIRO_DEV=1 and twinki renderer is in use.
 * Call isDevMode() first to avoid mounting this hook unnecessarily.
 */
export function useRenderMetrics(): RenderMetrics | null {
  const [metrics, setMetrics] = useState<RenderMetrics | null>(null);

  useEffect(() => {
    const instance = (globalThis as any).__TWINKI_INSTANCE__;
    if (!instance?.getMetrics) return;

    const interval = setInterval(() => {
      const m = instance.getMetrics();
      setMetrics({
        lastRenderMs: m.lastRenderMs,
        renderCount: m.renderCount,
        fullRedrawCount: m.fullRedrawCount,
        yogaNodeCount: m.yogaNodeCount,
        heapUsedMB: m.heapUsedMB,
      });
    }, 1000);

    return () => clearInterval(interval);
  }, []);

  return metrics;
}

/** Check before calling useRenderMetrics to avoid unnecessary hook registration. */
export function isDevMode(): boolean {
  return IS_DEV;
}
