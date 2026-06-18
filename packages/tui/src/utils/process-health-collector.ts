/**
 * Process health telemetry collector.
 *
 * Every 60s, samples render metrics (twinki), memory, CPU, event loop delay,
 * and input latency. Sends a single snapshot via ACP notification
 * to the Rust telemetry pipeline.
 */

import { monitorEventLoopDelay, type IntervalHistogram } from 'perf_hooks';
import { cpus, totalmem } from 'os';
import { inputMetrics } from './inputMetrics.js';
import { getCliVersion } from './version.js';

const INTERVAL_MS = 60_000;

export interface ProcessHealthSnapshot {
  agentKind?: 'v2' | 'kas';
  rssMb: number;
  heapUsedMb: number;
  peakRssMb: number;
  cpuUserPct: number;
  cpuSystemPct: number;
  lastRenderMs: number;
  maxRenderMs: number;
  rendersPerMin: number;
  fullRedrawsPerMin: number;
  yogaNodeCount: number;
  eventLoopP99Ms: number | null;
  inputLatencyP95Ms: number | null;
  sessionDurationSec: number;
  cpuCores: number;
  totalMemoryMb: number;
  terminal: string;
  sessionId: string | null;
  version: string;
  platform: string;
}

type SendFn = (payload: ProcessHealthSnapshot) => void;
type GetSessionIdFn = () => string | null;

export function startProcessHealthCollector(
  sendFn: SendFn,
  getSessionId?: GetSessionIdFn
): () => void {
  let prevCpu = process.cpuUsage();
  let prevRenderCount = 0;
  let prevFullRedrawCount = 0;

  // Enable input latency collection for health metrics
  inputMetrics.enable();

  // Static platform info (computed once)
  const cpuCores = cpus().length;
  const totalMemoryMb = Math.round(totalmem() / 1024 / 1024);
  const terminal =
    process.env.TERM_PROGRAM || process.env.TERMINAL_EMULATOR || 'unknown';

  let eld: IntervalHistogram | null = null;
  try {
    eld = monitorEventLoopDelay({ resolution: 20 });
    eld.enable();
  } catch {
    // monitorEventLoopDelay not available in older Bun versions
  }

  const startTime = Date.now();

  const timer = setInterval(() => {
    const rssMb = Math.round(process.memoryUsage().rss / 1024 / 1024);
    let heapUsedMb: number;
    let lastRenderMs = 0;
    let maxRenderMs = 0;
    let rendersPerMin = 0;
    let fullRedrawsPerMin = 0;
    let yogaNodeCount = 0;

    // --- Twinki render metrics ---
    const twinki = (globalThis as any).__TWINKI_INSTANCE__;
    if (twinki?.getMetrics) {
      const m = twinki.getMetrics();
      lastRenderMs = m.lastRenderMs;
      maxRenderMs = m.maxRenderMs;
      yogaNodeCount = m.yogaNodeCount;
      rendersPerMin = m.renderCount - prevRenderCount;
      fullRedrawsPerMin = m.fullRedrawCount - prevFullRedrawCount;
      prevRenderCount = m.renderCount;
      prevFullRedrawCount = m.fullRedrawCount;
    }

    // --- Heap (use bun:jsc for accurate JSC heap, fallback to process.memoryUsage) ---
    try {
      const jsc = require('bun:jsc');
      const jscMem = jsc.memoryUsage();
      heapUsedMb = Math.round(jscMem.current / 1024 / 1024);
    } catch {
      heapUsedMb = Math.round(process.memoryUsage().heapUsed / 1024 / 1024);
    }

    // --- Peak RSS ---
    // Bun exposes raw ru_maxrss: bytes on macOS, KB on Linux
    const ru = process.resourceUsage();
    const peakRssMb =
      process.platform === 'darwin'
        ? Math.round(ru.maxRSS / 1024 / 1024)
        : Math.round(ru.maxRSS / 1024);

    // --- CPU usage (percentage of 60s wall clock) ---
    const cpu = process.cpuUsage(prevCpu);
    prevCpu = process.cpuUsage();
    const cpuUserPct = cpu.user / INTERVAL_MS / 10;
    const cpuSystemPct = cpu.system / INTERVAL_MS / 10;

    // --- Event loop delay ---
    let eventLoopP99Ms: number | null = null;
    if (eld && eld.max > 0) {
      eventLoopP99Ms = eld.percentile(99) / 1e6;
      eld.reset();
    }

    // --- Input latency ---
    let inputLatencyP95Ms: number | null = null;
    const stats = inputMetrics.getStats();
    if (stats && stats.count > 0) {
      inputLatencyP95Ms = stats.p95Total;
      inputMetrics.clear();
    }

    sendFn({
      rssMb,
      heapUsedMb,
      peakRssMb,
      cpuUserPct,
      cpuSystemPct,
      lastRenderMs,
      maxRenderMs,
      rendersPerMin,
      fullRedrawsPerMin,
      yogaNodeCount,
      eventLoopP99Ms,
      inputLatencyP95Ms,
      sessionDurationSec: Math.round((Date.now() - startTime) / 1000),
      cpuCores,
      totalMemoryMb,
      terminal,
      sessionId: getSessionId?.() ?? null,
      version: getCliVersion(),
      platform: process.platform,
    });
  }, INTERVAL_MS);
  timer.unref();

  return () => {
    clearInterval(timer);
    eld?.disable();
  };
}
