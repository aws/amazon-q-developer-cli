#!/usr/bin/env bun
/**
 * ACP Test Harness Daemon — internal, spawned by acp-test-harness.ts start.
 * Do not run directly.
 */
import * as acp from '@agentclientprotocol/sdk';
import { spawn } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as net from 'node:net';

const STATE_DIR = path.resolve(import.meta.dir, '../.acp-harness');
const SOCKET_PATH = path.join(STATE_DIR, 'harness.sock');
const PID_FILE = path.join(STATE_DIR, 'daemon.pid');
const BINARY = process.argv[2];

if (!BINARY) { console.error('Usage: acp-test-daemon.ts <binary-path>'); process.exit(1); }

// ---------------------------------------------------------------------------
// ACP Client that collects turn output
// ---------------------------------------------------------------------------
class CollectingClient implements acp.Client {
  output: string[] = [];
  private resolve: (() => void) | null = null;
  turnDone: Promise<void> = Promise.resolve();

  startTurn() {
    this.output = [];
    this.turnDone = new Promise((r) => { this.resolve = r; });
  }
  endTurn() { this.resolve?.(); }

  async requestPermission(params: acp.RequestPermissionRequest): Promise<acp.RequestPermissionResponse> {
    const desc = params.description ?? params.toolCall?.title ?? 'unknown';
    this.output.push(`\n🔐 Permission: ${desc}`);
    const opt = params.options?.find((o) => o.kind === 'allow_once') ?? params.options?.[0];
    if (opt) {
      this.output.push(`   ✅ Auto-approved (${opt.name})\n`);
      return { outcome: { outcome: 'selected', optionId: opt.optionId } };
    }
    return { outcome: { outcome: 'cancelled' } };
  }

  async sessionUpdate(params: acp.SessionNotification): Promise<void> {
    const u = params.update;
    if (!u) return;
    console.log(`[client] sessionUpdate: ${u.sessionUpdate}`);
    switch (u.sessionUpdate) {
      case 'agent_message_chunk': {
        const c = u.content as any;
        const text = c?.text ?? (typeof c === 'string' ? c : '');
        if (text) this.output.push(text);
        break;
      }
      case 'tool_call':
        this.output.push(`\n🔧 ${(u as any).title ?? JSON.stringify(u)}\n`);
        break;
      case 'tool_call_update': {
        const status = (u as any).fields?.status ?? (u as any).status;
        if (status === 'completed') this.output.push(`   ✅ Tool completed\n`);
        else if (status === 'errored') this.output.push(`   ❌ Tool errored\n`);
        break;
      }
    }
  }

  async writeTextFile(): Promise<acp.WriteTextFileResponse> { return {}; }
  async readTextFile(): Promise<acp.ReadTextFileResponse> { return { content: '' }; }
  async createTerminal(): Promise<acp.CreateTerminalResponse> { return { terminalId: '' }; }
  async terminalOutput(): Promise<acp.TerminalOutputResponse> { return { output: '' }; }
  async releaseTerminal(): Promise<acp.ReleaseTerminalResponse> { return {}; }
  async waitForTerminalExit(): Promise<acp.WaitForTerminalExitResponse> { return { exitCode: 0 }; }
  async killTerminal(): Promise<acp.KillTerminalCommandResponse> { return {}; }
  async extMethod(): Promise<acp.ExtResponse> { return {}; }
  async extNotification(): Promise<void> {}
}

// ---------------------------------------------------------------------------
// Wire up ACP connection
// ---------------------------------------------------------------------------
function createAcpConnection(binary: string) {
  const proc = spawn(binary, ['acp'], {
    stdio: ['pipe', 'pipe', 'ignore'],
    env: { ...process.env },
  });
  if (!proc.stdin || !proc.stdout) throw new Error('Failed to get stdio');

  const stdin = proc.stdin;
  const stdout = proc.stdout;

  const writable = new WritableStream<Uint8Array>({
    async write(chunk) {
      return new Promise<void>((resolve, reject) => {
        stdin.write(chunk, (err) => (err ? reject(err) : resolve()));
      });
    },
    close() { stdin.end(); },
  });

  let ctrl: ReadableStreamDefaultController<any>;
  const readable = new ReadableStream<any>({
    start(c) { ctrl = c; },
    cancel() { stdout.destroy(); },
  });

  let buf = '';
  const dec = new TextDecoder();
  stdout.on('data', (chunk: Buffer) => {
    buf += dec.decode(new Uint8Array(chunk), { stream: true });
    const lines = buf.split('\n');
    buf = lines.pop() || '';
    for (const line of lines) {
      const t = line.trim();
      if (t) { try { ctrl.enqueue(JSON.parse(t)); } catch { /* skip */ } }
    }
  });
  stdout.on('end', () => {
    if (buf.trim()) try { ctrl.enqueue(JSON.parse(buf.trim())); } catch { /* */ }
    ctrl.close();
  });

  const dummyReadable = new ReadableStream<Uint8Array>({ start() {} });
  const ndJson = acp.ndJsonStream(writable, dummyReadable);
  const stream = { readable, writable: ndJson.writable };

  const client = new CollectingClient();
  const conn = new acp.ClientSideConnection((_agent) => client, stream);

  return { proc, conn, client };
}

// ---------------------------------------------------------------------------
// Main daemon
// ---------------------------------------------------------------------------
function cleanup() {
  try { fs.unlinkSync(SOCKET_PATH); } catch {}
  try { fs.unlinkSync(PID_FILE); } catch {}
}

const { proc, conn, client } = createAcpConnection(BINARY);

await conn.initialize({ protocolVersion: acp.PROTOCOL_VERSION, clientCapabilities: {} });
const session = await conn.newSession({ cwd: process.cwd(), mcpServers: [] });
const sessionId = session.sessionId;

console.log(`Session: ${sessionId}`);
if (session.modes?.currentModeId) console.log(`Agent: ${session.modes.currentModeId}`);

let busy = false;

const server = net.createServer((socket) => {
  let data = '';
  socket.on('data', (chunk) => {
    data += chunk.toString();
    // Newline delimiter signals end of request
    const nlIdx = data.indexOf('\n');
    if (nlIdx === -1) return;
    const line = data.slice(0, nlIdx);
    data = data.slice(nlIdx + 1);

    let req: { command: string; text?: string };
    try { req = JSON.parse(line); } catch { socket.end(JSON.stringify({ error: 'Invalid JSON' })); return; }

    handleRequest(req, socket);
  });
});

async function handleRequest(req: { command: string; text?: string }, socket: net.Socket) {
    console.log(`[daemon] received: ${req.command}`);

    if (req.command === 'prompt') {
      if (busy) {
        socket.end(JSON.stringify({ error: 'Turn in progress' }));
        return;
      }
      if (!req.text) {
        socket.end(JSON.stringify({ error: 'Missing text' }));
        return;
      }
      busy = true;
      try {
        client.output = [];
        await conn.prompt({ sessionId, prompt: [{ type: 'text', text: req.text }] });
        // prompt() resolves when the turn completes, output is already collected
        socket.end(JSON.stringify({ ok: true, response: client.output.join('') }));
      } catch (e: any) {
        socket.end(JSON.stringify({ error: e.message }));
      } finally {
        busy = false;
      }
    } else if (req.command === 'status') {
      socket.end(JSON.stringify({ ok: true, sessionId, busy }));
    } else if (req.command === 'stop') {
      socket.end(JSON.stringify({ ok: true, message: 'Shutting down' }));
      setTimeout(() => { proc.kill(); server.close(); cleanup(); process.exit(0); }, 100);
    } else {
      socket.end(JSON.stringify({ error: `Unknown command: ${req.command}` }));
    }
}

if (fs.existsSync(SOCKET_PATH)) fs.unlinkSync(SOCKET_PATH);
server.listen(SOCKET_PATH, () => {
  fs.writeFileSync(PID_FILE, process.pid.toString());
  console.log(`Listening on ${SOCKET_PATH}`);
});

proc.on('exit', (code) => {
  console.log(`Agent exited (code ${code})`);
  server.close();
  cleanup();
  process.exit(1);
});

process.on('SIGINT', () => { proc.kill(); server.close(); cleanup(); process.exit(0); });
process.on('SIGTERM', () => { proc.kill(); server.close(); cleanup(); process.exit(0); });
