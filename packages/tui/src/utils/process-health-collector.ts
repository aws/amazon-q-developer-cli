/**
 * Process health telemetry collector.
 *
 * Every 60s, samples render metrics (twinki), memory, CPU, event loop delay,
 * and input latency. Sends a single snapshot via ACP notification
 * to the Rust telemetry pipeline.
 */

import { monitorEventLoopDelay, type IntervalHistogram } from 'perf_hooks';
import { cpus, totalmem } from 'os';
import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { inputMetrics } from './inputMetrics.js';
import { getCliVersion } from './version.js';

const INTERVAL_MS = 60_000;

export interface ProcessHealthSnapshot {
  agentKind?: 'v2' | 'kas';
  rssMb: number;
  heapUsedMb: number;
  peakRssMb: number;
  openFileDescriptorCount?: number | null;
  handleCount?: number | null;
  threadCount?: number | null;
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
/** Promote a snapshot to SDK metrics alongside the ACP notification. */
type EmitMetricsFn = (payload: ProcessHealthSnapshot) => void;
/** Flush batched SDK metrics before exit; best-effort, must not throw. */
type FlushFn = () => Promise<void>;

export interface ProcessHealthCollectorOpts {
  /**
   * Promote each snapshot to SDK metrics in addition to `sendFn`.
   * Called on every 60s tick AND once more on teardown with a final sample.
   */
  emitMetrics?: EmitMetricsFn;
  /**
   * Force-flush the SDK metric pipeline. Awaited (with a bounded timeout) on
   * teardown so the final delta window — including the monotonic peak_rss
   * high-water mark — is delivered before exit. Never blocks exit indefinitely.
   */
  flushMetrics?: FlushFn;
}

const EXIT_FLUSH_TIMEOUT_MS = 2000;

function openFileDescriptorCount(): number | null {
  if (process.platform !== 'linux' && process.platform !== 'darwin') {
    return null;
  }
  try {
    return readdirSync(
      process.platform === 'linux' ? '/proc/self/fd' : '/dev/fd'
    ).length;
  } catch {
    return null;
  }
}

function windowsHandleCount(): number | null {
  if (process.platform !== 'win32') return null;
  try {
    const ffi = require('bun:ffi') as typeof import('bun:ffi');
    const count = new Uint32Array(1);
    const kernel32 = ffi.dlopen('kernel32.dll', {
      GetCurrentProcess: { args: [], returns: ffi.FFIType.ptr },
      GetProcessHandleCount: {
        args: [ffi.FFIType.ptr, ffi.FFIType.ptr],
        returns: ffi.FFIType.i32,
      },
    });
    try {
      const processHandle = kernel32.symbols.GetCurrentProcess();
      const succeeded = kernel32.symbols.GetProcessHandleCount(
        processHandle,
        ffi.ptr(count)
      );
      return succeeded === 0 ? null : count[0]!;
    } finally {
      kernel32.close();
    }
  } catch {
    return null;
  }
}

function processThreadCount(): number | null {
  try {
    if (process.platform === 'linux') {
      const match = readFileSync('/proc/self/status', 'utf8').match(
        /^Threads:\s+(\d+)$/m
      );
      return match?.[1] ? Number(match[1]) : null;
    }

    const command =
      process.platform === 'darwin'
        ? {
            bin: '/bin/ps',
            args: ['-M', '-p', String(process.pid), '-o', 'tid='],
          }
        : process.platform === 'win32'
          ? {
              bin: 'powershell.exe',
              args: [
                '-NoProfile',
                '-NonInteractive',
                '-Command',
                `(Get-Process -Id ${process.pid}).Threads.Count`,
              ],
            }
          : undefined;
    if (!command) return null;

    const result = spawnSync(command.bin, command.args, {
      encoding: 'utf8',
      timeout: 1000,
      windowsHide: true,
    });
    if (result.status !== 0) return null;
    if (process.platform === 'darwin') {
      const count = result.stdout
        .split('\n')
        .filter((line) => line.trim().length > 0).length;
      return count > 0 ? count : null;
    }
    const count = Number(result.stdout.trim());
    return Number.isInteger(count) && count > 0 ? count : null;
  } catch {
    return null;
  }
}

export function startProcessHealthCollector(
  sendFn: SendFn,
  getSessionId?: GetSessionIdFn,
  opts?: ProcessHealthCollectorOpts
): () => void {
  let prevCpu = process.cpuUsage();
  let prevCpuSampleMs = performance.now();
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

  // Has side effects (resets the rolling CPU/render/event-loop/input
  // accumulators) so each sample reports the delta since the previous one —
  // call it exactly once per emission, including the final exit sample.
  const sample = (): ProcessHealthSnapshot => {
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

    // --- CPU usage (percentage of elapsed wall clock) ---
    const currentCpu = process.cpuUsage();
    const cpuSampleMs = performance.now();
    const elapsedCpuMs = cpuSampleMs - prevCpuSampleMs;
    const cpu = {
      user: currentCpu.user - prevCpu.user,
      system: currentCpu.system - prevCpu.system,
    };
    prevCpu = currentCpu;
    prevCpuSampleMs = cpuSampleMs;
    const cpuUserPct = elapsedCpuMs > 0 ? cpu.user / elapsedCpuMs / 10 : 0;
    const cpuSystemPct = elapsedCpuMs > 0 ? cpu.system / elapsedCpuMs / 10 : 0;

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

    return {
      rssMb,
      heapUsedMb,
      peakRssMb,
      openFileDescriptorCount: openFileDescriptorCount(),
      handleCount: windowsHandleCount(),
      threadCount: processThreadCount(),
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
    };
  };

  // A failure in one transport must not stop the other.
  const emit = (snapshot: ProcessHealthSnapshot): void => {
    try {
      sendFn(snapshot);
    } catch {
      // never let the log transport break metric emission or the tick
    }
    try {
      opts?.emitMetrics?.(snapshot);
    } catch {
      // ignore
    }
  };

  const timer = setInterval(() => {
    emit(sample());
  }, INTERVAL_MS);
  timer.unref();

  // Teardown runs at most once. It takes ONE final sample (so the monotonic
  // peak_rss and the crash-adjacent CPU/render window are not lost), emits it,
  // then force-flushes the SDK metric pipeline with a bounded timeout so exit
  // is never blocked indefinitely. SIGINT/SIGTERM are wired so Ctrl-C flushes.
  let torndown = false;
  const stop = (): void => {
    if (torndown) return;
    torndown = true;
    clearInterval(timer);
    process.off('SIGINT', stop);
    process.off('SIGTERM', stop);
    try {
      emit(sample());
    } catch {
      // a final-sample failure must never break exit
    }
    eld?.disable();

    // Best-effort, bounded flush — NOT awaited (teardown is sync and exit must
    // not block on the network); the promise races a timeout, errors swallowed.
    const flush = opts?.flushMetrics;
    if (flush) {
      const bounded = Promise.race([
        flush().catch(() => {}),
        new Promise<void>((resolve) =>
          setTimeout(resolve, EXIT_FLUSH_TIMEOUT_MS).unref?.()
        ),
      ]);
      void bounded;
    }
  };

  process.once('SIGINT', stop);
  process.once('SIGTERM', stop);

  return stop;
}
