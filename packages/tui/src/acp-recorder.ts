/**
 * ACP wire recorder.
 *
 * Transparent pass-through wrapper on an ACP `Stream` that tees every
 * JSON-RPC message in both directions to a JSONL file. Enabled opt-in via
 * the `KIRO_ACP_RECORD_PATH` environment variable.
 *
 * Use for debugging protocol issues, reproducing production bugs, or
 * capturing traces to hand-author test scenarios from. Works for any ACP
 * client (KAS or V2) since it operates on the shared `Stream` seam.
 *
 * Trace format (one message per line):
 *   {"ts":1715187000000,"dir":"in","msg":{jsonrpc:"2.0",...}}
 *   {"ts":1715187000001,"dir":"out","msg":{jsonrpc:"2.0",...}}
 *
 * `dir` is `in` for messages from agent -> client, `out` for client -> agent.
 *
 * IMPORTANT: Traces contain full request/response bodies including user
 * prompts, tool inputs/outputs, and file contents. Treat them as sensitive.
 */
import { createWriteStream, type WriteStream } from 'node:fs';
import type { Stream } from '@kiro/client';
import type { AnyMessage } from '@agentclientprotocol/sdk';
import { logger } from './utils/logger';

type Direction = 'in' | 'out';

/**
 * Writes JSONL records of ACP wire traffic to a file in append mode.
 *
 * Writes are non-blocking; errors disable the recorder but never propagate
 * to the ACP stream. On close, pending buffered writes are flushed.
 */
export class AcpRecorder {
  private stream: WriteStream | null = null;
  private broken = false;

  constructor(private readonly path: string) {
    try {
      this.stream = createWriteStream(path, { flags: 'a' });
      this.stream.on('error', (err) => {
        logger.error('[acp-recorder] write stream error:', err);
        this.broken = true;
        this.stream = null;
      });
    } catch (err) {
      logger.error('[acp-recorder] failed to open recorder file:', err);
      this.broken = true;
    }
  }

  record(dir: Direction, msg: AnyMessage): void {
    if (this.broken || !this.stream) return;
    let line: string;
    try {
      line = JSON.stringify({ ts: Date.now(), dir, msg }) + '\n';
    } catch (err) {
      // JSON.stringify should never throw for AnyMessage but defend against
      // circular refs or BigInt values smuggled in by future protocol changes.
      logger.error('[acp-recorder] failed to stringify message:', err);
      return;
    }
    this.stream.write(line, (err) => {
      if (err) {
        logger.error('[acp-recorder] write error:', err);
        this.broken = true;
      }
    });
  }

  async close(): Promise<void> {
    const s = this.stream;
    this.stream = null;
    if (!s) return;
    await new Promise<void>((resolve) => s.end(() => resolve()));
  }
}

/**
 * Wraps a `Stream` so all messages in both directions are recorded without
 * changing observable behavior for consumers of the returned stream.
 */
export function wrapWithRecorder(
  stream: Stream,
  recorder: AcpRecorder
): Stream {
  const readable = stream.readable.pipeThrough(
    new TransformStream<AnyMessage, AnyMessage>({
      transform(msg, ctrl) {
        recorder.record('in', msg);
        ctrl.enqueue(msg);
      },
    })
  );

  const writable = new WritableStream<AnyMessage>({
    async write(msg) {
      recorder.record('out', msg);
      const writer = stream.writable.getWriter();
      try {
        await writer.write(msg);
      } finally {
        writer.releaseLock();
      }
    },
    close() {
      return stream.writable.close();
    },
    abort(reason) {
      return stream.writable.abort(reason);
    },
  });

  return { readable, writable } as Stream;
}

// ─── Singleton wiring via env var ────────────────────────────────────

let globalRecorder: AcpRecorder | null = null;
let installedListeners: Array<
  [NodeJS.Signals | 'beforeExit', () => Promise<void>]
> = [];

function registerExitHandlers(recorder: AcpRecorder): void {
  if (installedListeners.length > 0) return;
  const flush = async () => {
    try {
      await recorder.close();
    } catch (err) {
      logger.error('[acp-recorder] error during flush-on-exit:', err);
    }
  };
  // Graceful shutdown on signals. Async flush has a short window before the
  // process exits for real; this is best-effort.
  const signals: Array<NodeJS.Signals | 'beforeExit'> = [
    'SIGINT',
    'SIGTERM',
    'beforeExit',
  ];
  for (const sig of signals) {
    process.on(sig, flush);
    installedListeners.push([sig, flush]);
  }
}

/**
 * Returns a shared `AcpRecorder` if `KIRO_ACP_RECORD_PATH` is set, else null.
 * Subsequent calls return the same instance so multiple ACP clients in the
 * same process share one trace file.
 */
export function maybeCreateRecorder(): AcpRecorder | null {
  const path = process.env.KIRO_ACP_RECORD_PATH;
  if (!path) return null;
  if (globalRecorder) return globalRecorder;
  globalRecorder = new AcpRecorder(path);
  registerExitHandlers(globalRecorder);
  logger.info(`[acp-recorder] recording ACP traffic to ${path}`);
  return globalRecorder;
}

/**
 * Convenience: wrap `stream` with the shared recorder if one is active,
 * else return `stream` unchanged. Collapses the three otherwise-identical
 * call sites in `acp-client.ts`.
 */
export function maybeWrapStreamWithRecorder(stream: Stream): Stream {
  const recorder = maybeCreateRecorder();
  return recorder ? wrapWithRecorder(stream, recorder) : stream;
}

/**
 * Test-only: tear down the module-level singleton AND remove the signal
 * handlers it registered. Without the removal, repeated test cycles leak
 * listeners closed over stale recorders (hits Node's default max of 10).
 */
export function __resetRecorderForTests(): void {
  void globalRecorder?.close();
  for (const [sig, fn] of installedListeners) process.off(sig, fn);
  installedListeners = [];
  globalRecorder = null;
}
