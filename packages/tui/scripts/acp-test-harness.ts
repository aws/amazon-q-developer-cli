#!/usr/bin/env bun
/**
 * ACP Test Harness — agent-driven interactive testing via Unix socket daemon.
 *
 * Commands:
 *   bun acp-test-harness.ts start [--binary <path>]   Start daemon (returns immediately)
 *   bun acp-test-harness.ts prompt "your message"      Send prompt, get response
 *   bun acp-test-harness.ts status                      Check daemon status
 *   bun acp-test-harness.ts stop                        Shut down daemon
 *
 * The `start` command spawns a detached daemon process and returns immediately.
 * The daemon manages the real ACP backend connection. `prompt` commands connect
 * to the daemon's Unix socket, send the message, and return the full response.
 */
import * as path from 'node:path';
import * as fs from 'node:fs';

const STATE_DIR = path.resolve(import.meta.dir, '../.acp-harness');
const SOCKET_PATH = path.join(STATE_DIR, 'harness.sock');
const PID_FILE = path.join(STATE_DIR, 'daemon.pid');
const LOG_FILE = path.join(STATE_DIR, 'daemon.log');
const DEFAULT_BINARY = path.resolve(import.meta.dir, '../../../target/debug/chat_cli');
const DAEMON_SCRIPT = path.resolve(import.meta.dir, 'acp-test-daemon.ts');

// ---------------------------------------------------------------------------
// Client: send a command to the daemon via Unix socket
// ---------------------------------------------------------------------------
function sendCommand(cmd: object, timeoutMs = 120_000): Promise<any> {
  return new Promise((resolve, reject) => {
    if (!fs.existsSync(SOCKET_PATH)) {
      reject(new Error('Daemon not running. Run: bun acp-test-harness.ts start'));
      return;
    }
    const timer = setTimeout(() => reject(new Error('Timed out waiting for response')), timeoutMs);
    let data = '';
    let resolved = false;
    const done = (result: any) => {
      if (resolved) return;
      resolved = true;
      clearTimeout(timer);
      resolve(result);
    };
    Bun.connect({
      unix: SOCKET_PATH,
      socket: {
        open(socket) {
          // Send command + newline delimiter, keep connection open for response
          socket.write(JSON.stringify(cmd) + '\n');
        },
        data(_socket, chunk) {
          data += new TextDecoder().decode(chunk);
          // Try to parse — server sends complete JSON then closes
          try { done(JSON.parse(data)); } catch { /* wait for more */ }
        },
        close() {
          clearTimeout(timer);
          if (!resolved) {
            try { done(JSON.parse(data)); } catch { done({ raw: data }); }
          }
        },
        error(_socket, err) {
          clearTimeout(timer);
          reject(err);
        },
      },
    }).catch((e) => { clearTimeout(timer); reject(e); });
  });
}

function waitForSocket(maxWaitMs = 30_000): Promise<void> {
  const start = Date.now();
  return new Promise((resolve, reject) => {
    const check = () => {
      if (fs.existsSync(SOCKET_PATH)) return resolve();
      if (Date.now() - start > maxWaitMs) {
        const log = fs.existsSync(LOG_FILE) ? fs.readFileSync(LOG_FILE, 'utf-8').slice(-500) : 'no log';
        return reject(new Error(`Daemon failed to start within ${maxWaitMs}ms.\nLog:\n${log}`));
      }
      setTimeout(check, 200);
    };
    check();
  });
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------
const [command, ...rest] = process.argv.slice(2);

switch (command) {
  case 'start': {
    const binaryIdx = rest.indexOf('--binary');
    const binary = binaryIdx >= 0 ? rest[binaryIdx + 1] : DEFAULT_BINARY;

    if (fs.existsSync(SOCKET_PATH)) {
      console.log('Daemon already running. Use "stop" first.');
      process.exit(1);
    }

    fs.mkdirSync(STATE_DIR, { recursive: true });

    // Spawn daemon as fully detached process
    const { spawn } = await import('node:child_process');
    const logFd = fs.openSync(LOG_FILE, 'w');
    const child = spawn('bun', [DAEMON_SCRIPT, binary], {
      detached: true,
      stdio: ['ignore', logFd, logFd],
      env: { ...process.env },
      cwd: path.resolve(import.meta.dir, '..'),
    });
    child.unref();
    fs.closeSync(logFd);

    // Wait for socket to appear
    try {
      await waitForSocket();
      const status = await sendCommand({ command: 'status' });
      console.log(`Daemon started. Session: ${status.sessionId}`);
    } catch (e: any) {
      console.error(e.message);
      process.exit(1);
    }
    break;
  }

  case 'prompt': {
    const text = rest.join(' ');
    if (!text) { console.error('Usage: prompt "your message"'); process.exit(1); }
    try {
      const result = await sendCommand({ command: 'prompt', text });
      if (result.error) { console.error('Error:', result.error); process.exit(1); }
      console.log(result.response);
    } catch (e: any) { console.error(e.message); process.exit(1); }
    break;
  }

  case 'status': {
    try {
      const result = await sendCommand({ command: 'status' });
      console.log(JSON.stringify(result, null, 2));
    } catch (e: any) { console.error(e.message); process.exit(1); }
    break;
  }

  case 'stop': {
    try {
      const result = await sendCommand({ command: 'stop' });
      console.log(result.message ?? 'Stopped');
    } catch (e: any) { console.error(e.message); process.exit(1); }
    break;
  }

  default:
    console.log(`ACP Test Harness — interactive agent testing with real LLM

Commands:
  start [--binary <path>]   Start daemon (default: target/debug/chat_cli)
  prompt "message"           Send prompt and get response
  status                     Check daemon status
  stop                       Shut down daemon`);
}
