import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { createServer, type Server, type Socket } from 'node:net';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectMockTransport } from '../test-utils/acp-mock/MockAcpTransport';
import { encodeFrame, FrameDecoder } from '../test-utils/acp-mock/framing';

describe('MockAcpTransport', () => {
  let dir: string;
  let socketPath: string;
  let server: Server;
  let connected: Socket | null = null;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'mock-transport-'));
    socketPath = join(dir, 'acp.sock');
  });

  afterEach(async () => {
    if (connected && !connected.destroyed) connected.destroy();
    if (server) await new Promise<void>((r) => server.close(() => r()));
    rmSync(dir, { recursive: true, force: true });
  });

  function startServer(onConn: (sock: Socket) => void): Promise<void> {
    return new Promise((resolve) => {
      server = createServer((sock) => {
        connected = sock;
        sock.setEncoding('utf8');
        onConn(sock);
      });
      server.listen(socketPath, () => resolve());
    });
  }

  it('enqueues decoded messages from the server onto readable', async () => {
    await startServer((sock) => {
      sock.write(
        encodeFrame({
          jsonrpc: '2.0',
          method: 'session/update',
          params: { a: 1 },
        })
      );
      sock.write(encodeFrame({ jsonrpc: '2.0', id: 1, result: { ok: true } }));
    });

    const stream = connectMockTransport(socketPath);
    const reader = stream.readable.getReader();
    const m1 = await reader.read();
    const m2 = await reader.read();
    expect((m1.value as { method: string }).method).toBe('session/update');
    expect((m2.value as { result: { ok: boolean } }).result).toEqual({
      ok: true,
    });
    reader.releaseLock();
  });

  it('forwards messages written to writable onto the socket as framed JSON', async () => {
    const received: unknown[] = [];
    await startServer((sock) => {
      const decoder = new FrameDecoder();
      sock.on('data', (chunk: string) => {
        for (const msg of decoder.feed(chunk)) received.push(msg);
      });
    });

    const stream = connectMockTransport(socketPath);
    const writer = stream.writable.getWriter();
    await writer.write({
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {},
    });
    await writer.write({
      jsonrpc: '2.0',
      id: 2,
      method: 'session/new',
      params: { cwd: '/t' },
    });
    writer.releaseLock();

    // Give the server a tick to receive.
    await new Promise((r) => setTimeout(r, 20));
    expect(received).toHaveLength(2);
    expect((received[0] as { method: string }).method).toBe('initialize');
    expect((received[1] as { method: string }).method).toBe('session/new');
  });

  it('roundtrips bidirectionally in order', async () => {
    const serverReceived: unknown[] = [];
    await startServer((sock) => {
      const decoder = new FrameDecoder();
      sock.on('data', (chunk: string) => {
        for (const msg of decoder.feed(chunk)) {
          serverReceived.push(msg);
          // Echo a response based on the request id.
          if (typeof (msg as { id?: unknown }).id === 'number') {
            sock.write(
              encodeFrame({
                jsonrpc: '2.0',
                id: (msg as { id: number }).id,
                result: { echoed: true },
              })
            );
          }
        }
      });
    });

    const stream = connectMockTransport(socketPath);
    const writer = stream.writable.getWriter();
    const reader = stream.readable.getReader();

    await writer.write({ jsonrpc: '2.0', id: 1, method: 'x' });
    const resp1 = await reader.read();
    expect(
      (resp1.value as { id: number; result: { echoed: boolean } }).id
    ).toBe(1);

    await writer.write({ jsonrpc: '2.0', id: 2, method: 'y' });
    const resp2 = await reader.read();
    expect((resp2.value as { id: number }).id).toBe(2);

    writer.releaseLock();
    reader.releaseLock();
    expect(serverReceived).toHaveLength(2);
  });

  it('closes readable when the server ends the connection', async () => {
    await startServer((sock) => {
      sock.write(encodeFrame({ jsonrpc: '2.0', method: 'final', params: {} }));
      sock.end();
    });

    const stream = connectMockTransport(socketPath);
    const reader = stream.readable.getReader();
    const m1 = await reader.read();
    expect((m1.value as { method: string }).method).toBe('final');
    const m2 = await reader.read();
    expect(m2.done).toBe(true);
    reader.releaseLock();
  });

  it('write rejects after the writable is closed', async () => {
    await startServer(() => {
      /* just accept */
    });
    const stream = connectMockTransport(socketPath);
    const writer = stream.writable.getWriter();
    await writer.write({ jsonrpc: '2.0', id: 1, method: 'hi' });
    // Close the writable cleanly.
    await writer.close();
    // Re-acquire to attempt a write after close.
    await expect(
      (async () => {
        const w2 = stream.writable.getWriter();
        await w2.write({ jsonrpc: '2.0', id: 2, method: 'too-late' });
      })()
    ).rejects.toThrow();
  });
});
