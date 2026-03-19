/**
 * ACP test helper that spawns a separate `chat_cli acp` process sharing the
 * same sandboxed directories as the main E2E test case. Provides a clean ACP
 * client for creating sessions, sending prompts, and pushing mock responses.
 *
 * Not intended to be used directly — use `testCase.launchAcpHelper()` instead.
 */

import * as acp from '@agentclientprotocol/sdk';
import * as net from 'node:net';
import * as os from 'node:os';
import * as path from 'node:path';
import * as fs from 'node:fs';
import { spawn, type ChildProcess } from 'node:child_process';
import { Writable, Readable } from 'node:stream';
import { TuiIpcConnection } from '../src/test-utils/shared/tui-ipc-connection';
import type { MockStreamItem } from './types/chat-cli';

export interface AcpTestHelperOptions {
  /** Full env to pass to the ACP process (from E2ETestCase.sandboxEnv). */
  env: Record<string, string>;
  /** Test name for log/socket paths. */
  testName: string;
}

export class AcpTestHelper {
  private process: ChildProcess;
  private connection: acp.ClientSideConnection;
  private ipcServer: net.Server;
  private ipcConnection?: TuiIpcConnection;
  private socketPath: string;

  private constructor(
    proc: ChildProcess,
    connection: acp.ClientSideConnection,
    ipcServer: net.Server,
    socketPath: string,
  ) {
    this.process = proc;
    this.connection = connection;
    this.ipcServer = ipcServer;
    this.socketPath = socketPath;
  }

  static async spawn(options: AcpTestHelperOptions): Promise<AcpTestHelper> {
    const socketDir = path.join(os.tmpdir(), 'kiro-tests', options.testName);
    fs.mkdirSync(socketDir, { recursive: true });
    const ipcSocketPath = path.join(socketDir, 'h.sock');
    const logPath = path.join(socketDir, 'helper-rust.log');

    try { fs.unlinkSync(ipcSocketPath); } catch { /* ignore */ }

    // Start IPC server for mock response injection
    const ipcServer = net.createServer();
    await new Promise<void>((resolve, reject) => {
      ipcServer.listen(ipcSocketPath, (err?: Error) => {
        if (err) reject(err); else resolve();
      });
    });

    const chatPath = path.join(__dirname, '../../../target/debug/chat_cli');

    // Reuse the sandbox env but override IPC socket and log paths
    const proc = spawn(chatPath, ['acp'], {
      stdio: ['pipe', 'pipe', 'inherit'],
      env: {
        ...options.env,
        KIRO_TEST_CHAT_IPC_SOCKET_PATH: ipcSocketPath,
        KIRO_CHAT_LOG_FILE: logPath,
      },
    });

    if (!proc.stdin || !proc.stdout) {
      throw new Error('Failed to create ACP helper process stdio');
    }

    // Wait for IPC connection from the agent
    const ipcConnectionPromise = new Promise<TuiIpcConnection>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('Timeout waiting for helper IPC connection')), 15000);
      ipcServer.on('connection', (socket) => {
        clearTimeout(timer);
        resolve(new TuiIpcConnection(socket));
      });
    });

    const output = Writable.toWeb(proc.stdin) as WritableStream<Uint8Array>;
    const input = Readable.toWeb(proc.stdout) as ReadableStream<Uint8Array>;
    const stream = acp.ndJsonStream(output, input);

    const connection = new acp.ClientSideConnection(
      () => ({
        async requestPermission(params: acp.RequestPermissionRequest) {
          const opt = params.options?.find(o => o.kind === 'allow_once') ?? params.options?.[0];
          return { outcome: { outcome: 'selected' as const, optionId: opt?.optionId ?? '' } };
        },
        async sessionUpdate() {},
      }),
      stream,
    );

    await connection.initialize({
      protocolVersion: acp.PROTOCOL_VERSION,
      clientCapabilities: {},
    });

    const helper = new AcpTestHelper(proc, connection, ipcServer, ipcSocketPath);
    helper.ipcConnection = await ipcConnectionPromise;
    return helper;
  }

  /** Create a new session and return its ID. */
  async newSession(cwd?: string): Promise<string> {
    const result = await this.connection.newSession({
      cwd: cwd ?? process.cwd(),
      mcpServers: [],
    });
    return result.sessionId;
  }

  /** Push mock response events for a session. Pass null to signal end of stream. */
  async pushResponse(sessionId: string, events: MockStreamItem[] | null): Promise<void> {
    if (!this.ipcConnection) throw new Error('IPC not connected');
    const response = await this.ipcConnection.sendCommand({
      kind: 'PUSH_SEND_MESSAGE_RESPONSE',
      session_id: sessionId,
      events,
    });
    if (response.data.kind === 'ERROR') {
      throw new Error(`Failed to push response: ${(response.data as any).error}`);
    }
  }

  /** Send a text prompt and wait for the agent to finish processing. */
  async prompt(sessionId: string, text: string): Promise<void> {
    await this.connection.prompt({
      sessionId,
      prompt: [{ type: 'text', text }],
    });
  }

  /** Terminate a session. */
  async terminateSession(sessionId: string): Promise<void> {
    try {
      await this.connection.extMethod('kiro.dev/session/terminate', { sessionId });
    } catch { /* best-effort */ }
  }

  /** Switch the agent/mode for a session. */
  async setSessionMode(sessionId: string, modeId: string): Promise<void> {
    await this.connection.setSessionMode({ sessionId, modeId });
  }

  /** Shut down the helper process and clean up. */
  async close(): Promise<void> {
    this.ipcConnection?.close();
    this.ipcServer?.close();
    this.process.kill('SIGTERM');
    try { fs.unlinkSync(this.socketPath); } catch { /* ignore */ }
  }
}
