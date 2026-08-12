/**
 * TUI-side mock transport for acp_integ_tests.
 *
 * Connects to a local IPC endpoint controlled by a test-side `AcpMockServer` and
 * exposes the same `Stream` shape (`{ readable, writable }`) that
 * `@kiro/client` / `@agentclientprotocol/sdk` consume for stdio transports.
 * Enables tests to drive the real `KasAcpClient` end-to-end without
 * spawning a KAS subprocess.
 *
 * Activated via the `KIRO_ACP_MOCK_SOCKET` env var - see `acp-client.ts`.
 */
import { createConnection, type Socket } from 'node:net';
import type { Stream } from '@kiro/client';
import type { AnyMessage } from '@agentclientprotocol/sdk';
import { logger } from '../../utils/logger';
import { encodeFrame, FrameDecoder } from './framing';

/**
 * Connects to a local IPC endpoint and returns a `Stream` wiring:
 *   - Bytes received on the socket are decoded and enqueued on `readable`
 *     as individual `AnyMessage` objects.
 *   - Messages written to `writable` are JSON-encoded, newline-framed, and
 *     sent to the socket.
 *
 * Socket errors log but don't throw in consumer code; instead the streams
 * are closed/aborted so the ACP client surfaces a natural disconnect.
 */
export function connectMockTransport(socketPath: string): Stream {
  const socket = connect(socketPath);
  socket.setEncoding('utf8');

  let readCtrl: ReadableStreamDefaultController<AnyMessage> | null = null;
  const decoder = new FrameDecoder();

  const readable = new ReadableStream<AnyMessage>({
    start(controller) {
      readCtrl = controller;
    },
    cancel() {
      socket.destroy();
    },
  });

  socket.on('data', (chunk: string) => {
    if (!readCtrl) return;
    const msgs = decoder.feed(chunk, (line, err) => {
      logger.error(
        '[mock-transport] malformed frame from mock server:',
        line,
        err
      );
    });
    for (const msg of msgs) {
      readCtrl.enqueue(msg as AnyMessage);
    }
  });

  socket.on('end', () => {
    if (!readCtrl) return;
    const tail = decoder.finalize((line, err) => {
      logger.error('[mock-transport] malformed trailing frame:', line, err);
    });
    for (const msg of tail) readCtrl.enqueue(msg as AnyMessage);
    try {
      readCtrl.close();
    } catch {
      /* already closed */
    }
  });

  socket.on('error', (err) => {
    logger.error('[mock-transport] socket error:', err);
    if (readCtrl) {
      try {
        readCtrl.error(err);
      } catch {
        /* already closed */
      }
    }
  });

  const writable = new WritableStream<AnyMessage>({
    write(msg) {
      return new Promise<void>((resolve, reject) => {
        if (socket.destroyed || socket.writableEnded) {
          reject(new Error('mock transport socket closed'));
          return;
        }
        let framed: string;
        try {
          framed = encodeFrame(msg);
        } catch (err) {
          reject(err);
          return;
        }
        socket.write(framed, (err) => (err ? reject(err) : resolve()));
      });
    },
    close() {
      socket.end();
    },
    abort(reason) {
      socket.destroy(
        reason instanceof Error ? reason : new Error(String(reason))
      );
    },
  });

  return { readable, writable } as Stream;
}

// Exposed for tests that want to inspect or influence socket lifecycle.
export type { Socket };

function connect(endpoint: string): Socket {
  if (!endpoint.startsWith('tcp://')) {
    return createConnection(endpoint);
  }

  const url = new URL(endpoint);
  return createConnection({
    host: url.hostname,
    port: url.port ? Number(url.port) : 0,
  });
}
