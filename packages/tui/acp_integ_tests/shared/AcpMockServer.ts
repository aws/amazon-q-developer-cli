/**
 * Test-side ACP mock server for acp_integ_tests.
 *
 * Listens on a Unix socket; accepts a single connection from a TUI process
 * running with `KIRO_ACP_MOCK_SOCKET=<path>`. Routes JSON-RPC messages in
 * both directions:
 *
 *   - Requests from TUI   -> registered `.on(method, handler)` functions
 *   - Notifications       -> observed (via `.receivedNotifications`)
 *   - Responses           -> matched to in-flight `.request(...)` promises
 *
 *   - `.notify(m, p)`     -> send notification to TUI
 *   - `.request(m, p)`    -> send request to TUI, await its response
 *
 * The server buffers outbound traffic until the TUI connects so tests don't
 * need to race on connection establishment.
 *
 * TODO(permission-requests): scenarios requiring the TUI to surface a
 * confirmation dialog in response to a server `request(...)` need the
 * existing TestCase harness to grow hooks for scripting user input. The
 * plumbing here resolves whatever reply the TUI sends; the TUI-side
 * machinery to script that reply lands in a follow-up.
 */
import { createServer, type Server, type Socket } from 'node:net';
import {
  encodeFrame,
  FrameDecoder,
} from '../../src/test-utils/acp-mock/framing';

export type RequestHandler<Req = unknown, Resp = unknown> = (
  params: Req
) => Promise<Resp> | Resp;

export type ObservedRequest = { method: string; params: unknown };
export type ObservedNotification = { method: string; params: unknown };

// JSON-RPC message types. We add explicit `undefined` on absent discriminant
// fields so TypeScript narrows cleanly in control-flow: `'method' in m` and
// `'id' in m` are too ambiguous for `AnyJsonRpc` otherwise (all three shapes
// share `jsonrpc: '2.0'`, so there's no discriminant property).
type JsonRpcRequest = {
  jsonrpc: '2.0';
  id: number | string;
  method: string;
  params?: unknown;
};
type JsonRpcNotification = {
  jsonrpc: '2.0';
  id?: undefined;
  method: string;
  params?: unknown;
};
type JsonRpcResponse = {
  jsonrpc: '2.0';
  id: number | string;
  method?: undefined;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
};
type AnyJsonRpc = JsonRpcRequest | JsonRpcNotification | JsonRpcResponse;

function isRequest(m: AnyJsonRpc): m is JsonRpcRequest {
  return m.method !== undefined && m.id !== undefined;
}
function isNotification(m: AnyJsonRpc): m is JsonRpcNotification {
  return m.method !== undefined && m.id === undefined;
}
function isResponse(m: AnyJsonRpc): m is JsonRpcResponse {
  return m.method === undefined;
}

export class AcpMockServer {
  private server!: Server;
  private socket: Socket | null = null;
  private handlers = new Map<string, RequestHandler>();
  private observedRequests: ObservedRequest[] = [];
  private observedNotifications: ObservedNotification[] = [];
  private outboundBuffer: string[] = [];
  private nextRequestId = 1;
  private pendingRequests = new Map<
    number | string,
    { resolve: (v: unknown) => void; reject: (err: Error) => void }
  >();
  private decoder = new FrameDecoder();
  private closed = false;
  private connectionPromise: Promise<void>;
  private resolveConnection!: () => void;

  constructor(public readonly socketPath: string) {
    this.connectionPromise = new Promise((resolve) => {
      this.resolveConnection = resolve;
    });
  }

  async listen(): Promise<void> {
    this.server = createServer((sock) => this.onConnection(sock));
    await new Promise<void>((resolve, reject) => {
      this.server.once('error', reject);
      this.server.listen(this.socketPath, () => {
        this.server.off('error', reject);
        resolve();
      });
    });
  }

  /**
   * Resolves once a TUI process has connected. Useful when a test needs to
   * wait before calling `notify()` or `request()` to ensure the message
   * isn't buffered indefinitely (buffered messages flush automatically on
   * connect anyway, but waiting makes ordering deterministic).
   */
  async awaitConnection(): Promise<void> {
    await this.connectionPromise;
  }

  /** Register a handler for requests with the given method name. */
  on<Req = unknown, Resp = unknown>(
    method: string,
    handler: RequestHandler<Req, Resp>
  ): this {
    this.handlers.set(method, handler as RequestHandler);
    return this;
  }

  /** Send a notification to the TUI (fire-and-forget). */
  notify(method: string, params: unknown): void {
    this.send({ jsonrpc: '2.0', method, params });
  }

  /**
   * Send a request to the TUI and return a promise resolving with its
   * response. Used for agent -> client calls like
   * `session/request_permission`.
   */
  request<Resp = unknown>(method: string, params: unknown): Promise<Resp> {
    const id = this.nextRequestId++;
    const promise = new Promise<unknown>((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });
    });
    this.send({ jsonrpc: '2.0', id, method, params });
    return promise as Promise<Resp>;
  }

  /** Snapshot of observed inbound requests (method + params). Test helper. */
  receivedRequests(method?: string): ObservedRequest[] {
    return method
      ? this.observedRequests.filter((r) => r.method === method)
      : [...this.observedRequests];
  }

  /** Snapshot of observed inbound notifications. Test helper. */
  receivedNotifications(method?: string): ObservedNotification[] {
    return method
      ? this.observedNotifications.filter((n) => n.method === method)
      : [...this.observedNotifications];
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    // Reject any in-flight requests so tests don't hang.
    for (const pending of this.pendingRequests.values()) {
      pending.reject(new Error('AcpMockServer closed before response arrived'));
    }
    this.pendingRequests.clear();
    if (this.socket && !this.socket.destroyed) this.socket.destroy();
    if (this.server) {
      await new Promise<void>((resolve) => this.server.close(() => resolve()));
    }
  }

  // ─── Internal ────────────────────────────────────────────────────

  private onConnection(sock: Socket): void {
    if (this.socket) {
      // Only one TUI connection at a time.
      sock.destroy(new Error('AcpMockServer already has a client'));
      return;
    }
    this.socket = sock;
    sock.setEncoding('utf8');
    sock.on('data', (chunk: string) => this.onData(chunk));
    sock.on('end', () => this.onEnd());
    sock.on('error', (err) => {
      // Ignore EPIPE etc. on shutdown; surface anything else to stderr for
      // visibility in test runs.
      if ((err as NodeJS.ErrnoException).code !== 'EPIPE') {
        console.error('[acp-mock-server] socket error:', err);
      }
    });
    // Flush any buffered outbound messages now that we have a connection.
    for (const frame of this.outboundBuffer) sock.write(frame);
    this.outboundBuffer = [];
    this.resolveConnection();
  }

  private onData(chunk: string): void {
    const msgs = this.decoder.feed(chunk, (line, err) => {
      console.error('[acp-mock-server] malformed frame from TUI:', line, err);
    });
    for (const msg of msgs) this.dispatch(msg as AnyJsonRpc);
  }

  private onEnd(): void {
    const tail = this.decoder.finalize();
    for (const msg of tail) this.dispatch(msg as AnyJsonRpc);
  }

  private async dispatch(msg: AnyJsonRpc): Promise<void> {
    if (isResponse(msg)) {
      const pending = this.pendingRequests.get(msg.id);
      if (!pending) {
        console.error('[acp-mock-server] response for unknown id:', msg.id);
        return;
      }
      this.pendingRequests.delete(msg.id);
      if ('error' in msg && msg.error) {
        pending.reject(new Error(`TUI returned error: ${msg.error.message}`));
      } else {
        pending.resolve(msg.result);
      }
      return;
    }

    if (isNotification(msg)) {
      this.observedNotifications.push({
        method: msg.method,
        params: msg.params,
      });
      return;
    }

    if (isRequest(msg)) {
      this.observedRequests.push({ method: msg.method, params: msg.params });
      const handler = this.handlers.get(msg.method);
      if (!handler) {
        this.send({
          jsonrpc: '2.0',
          id: msg.id,
          error: {
            code: -32601,
            message: `No handler registered for method: ${msg.method}`,
          },
        });
        return;
      }
      try {
        const result = await handler(msg.params);
        this.send({ jsonrpc: '2.0', id: msg.id, result });
      } catch (err) {
        this.send({
          jsonrpc: '2.0',
          id: msg.id,
          error: {
            code: -32000,
            message: err instanceof Error ? err.message : String(err),
          },
        });
      }
      return;
    }

    console.error('[acp-mock-server] unrecognized JSON-RPC message:', msg);
  }

  private send(msg: AnyJsonRpc): void {
    const frame = encodeFrame(msg);
    if (this.socket && !this.socket.destroyed) {
      this.socket.write(frame);
    } else {
      this.outboundBuffer.push(frame);
    }
  }
}
