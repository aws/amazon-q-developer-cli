/**
 * E2E TestCase for full-stack testing of Kiro CLI.
 * 
 * Spawns the real `kiro-cli chat` command, enabling dual IPC connections
 * to both TUI (Zustand store) and Rust backend session state.
 */

import * as fs from 'fs';
import * as net from 'net';
import * as path from 'path';
import type { AppState } from '../src/stores/app-store';
import { PtyManager, TerminalSnapshot } from '../src/test-utils/shared/pty-manager';
import { createTestDir, type TestPaths } from '../src/test-utils/shared/test-paths';
import { TuiIpcConnection } from '../src/test-utils/shared/tui-ipc-connection';
import type { MockStreamItem } from './types/chat-cli';

interface E2ETestCaseOptions {
  terminalSize?: { width: number; height: number };
  timeout?: number;
  testName?: string;
}

/**
 * E2ETestCase provides full-stack E2E testing for the Kiro CLI application.
 * 
 * Unlike integration tests that mock the ACP layer, E2E tests run the complete
 * stack: TUI -> ACP -> Rust Agent.
 * 
 * Key features:
 * - Real PTY for authentic terminal behavior
 * - Dual IPC: TUI state (Zustand) + Rust backend session state (AgentSnapshot)
 */
export class E2ETestCase {
  private ptyManager: PtyManager;
  private tuiIpcServer: net.Server;
  private agentIpcServer: net.Server;
  private paths: TestPaths;
  private options: E2ETestCaseOptions;
  private tuiConnection?: TuiIpcConnection;
  private agentConnection?: TuiIpcConnection;

  constructor(options: E2ETestCaseOptions = {}) {
    this.options = {
      terminalSize: { width: 120, height: 40 },
      timeout: 30000,
      ...options,
    };

    const testName = this.options.testName || `e2e-${Date.now()}`;
    this.paths = createTestDir(testName);

    const chatPath = path.join(__dirname, '../../../target/debug/chat_cli_v2');
    const tuiJsPath = path.join(__dirname, '../dist/tui.js');

    this.ptyManager = new PtyManager({
      width: this.options.terminalSize!.width,
      height: this.options.terminalSize!.height,
      cwd: process.cwd(),
      env: {
        KIRO_TEST_MODE: '1',
        KIRO_INPUT_METRICS: 'true',
        KIRO_TEST_TUI_IPC_SOCKET_PATH: this.paths.tuiIpcSocket,
        KIRO_TEST_CHAT_IPC_SOCKET_PATH: this.paths.agentIpcSocket,
        KIRO_TEST_TUI_JS_PATH: tuiJsPath,
        KIRO_AGENT_PATH: chatPath,
        KIRO_TUI_LOG_FILE: this.paths.tuiLogFile,
        KIRO_TUI_LOG_LEVEL: 'trace',
        KIRO_CHAT_LOG_FILE: this.paths.rustLogFile,
        KIRO_LOG_LEVEL: 'chat_cli=debug,agent=debug',
      },
    });

    // TUI connects to this server
    this.tuiIpcServer = net.createServer((socket) => {
      this.tuiConnection = new TuiIpcConnection(socket);
    });

    // Agent connects to this server
    this.agentIpcServer = net.createServer((socket) => {
      this.agentConnection = new TuiIpcConnection(socket);
    });
  }

  static builder(): E2ETestCaseBuilder {
    return new E2ETestCaseBuilder();
  }

  async launch(): Promise<E2ETestCase> {
    // Clean up existing sockets
    try { fs.unlinkSync(this.paths.tuiIpcSocket); } catch { /* ignore */ }
    try { fs.unlinkSync(this.paths.agentIpcSocket); } catch { /* ignore */ }

    // Start both IPC servers
    await Promise.all([
      new Promise<void>((resolve, reject) => {
        this.tuiIpcServer.listen(this.paths.tuiIpcSocket, (error?: Error) => {
          if (error) reject(error);
          else resolve();
        });
      }),
      new Promise<void>((resolve, reject) => {
        this.agentIpcServer.listen(this.paths.agentIpcSocket, (error?: Error) => {
          if (error) reject(error);
          else resolve();
        });
      }),
    ]);

    // Spawn the real CLI
    const chatPath = path.join(__dirname, '../../../target/debug/chat_cli_v2');
    this.ptyManager.spawn(chatPath, ['chat']);

    console.log(`TUI logs: ${this.paths.tuiLogFile}`);
    console.log(`Rust logs: ${this.paths.rustLogFile}`);
    console.log(`Snapshot: ${this.paths.snapshotHtmlFile}`);

    // Wait for both IPC connections
    await Promise.all([
      this.waitForTuiConnection(),
      this.waitForAgentConnection(),
    ]);

    return this;
  }

  async cleanup(): Promise<void> {
    // Save HTML snapshot before cleanup
    try {
      fs.writeFileSync(this.paths.snapshotHtmlFile, this.getSnapshotHtml());
    } catch { /* ignore if terminal already closed */ }

    this.ptyManager.kill();
    this.tuiConnection?.close();
    this.agentConnection?.close();
    this.tuiIpcServer?.close();
    this.agentIpcServer?.close();
  }

  async sendKeys(input: string | number[]): Promise<void> {
    return this.ptyManager.sendKeys(input);
  }

  /** Send Enter key */
  async pressEnter(): Promise<void> {
    return this.sendKeys('\r');
  }

  /** Send Escape key */
  async pressEscape(): Promise<void> {
    return this.sendKeys([0x1b]);
  }

  /** Send Ctrl+C */
  async pressCtrlC(): Promise<void> {
    return this.sendKeys([0x03]);
  }

  /** Send Ctrl+C twice to exit */
  async pressCtrlCTwice(): Promise<void> {
    return this.sendKeys([0x03, 0x03]);
  }

  async sleepMs(ms: number): Promise<void> {
    await new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Gets TUI application state (Zustand store).
   */
  async getStore(): Promise<AppState> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    const response = await this.tuiConnection.sendCommand({ kind: 'GET_STORE' });
    if (response.data.kind !== 'GET_STORE') {
      throw new Error(`Unexpected response: ${JSON.stringify(response)}`);
    }
    return response.data.data;
  }

  /**
   * Takes a heap snapshot from the TUI process.
   */
  async takeHeapSnapshot(filename: string): Promise<string> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    const response = await this.tuiConnection.sendCommand({ kind: 'HEAP_SNAPSHOT', filename });
    if (response.data.kind !== 'HEAP_SNAPSHOT') {
      throw new Error(`Unexpected response: ${JSON.stringify(response)}`);
    }
    return response.data.filename;
  }

  /**
   * Gets memory usage from within the TUI process (process.memoryUsage()).
   */
  async getMemoryUsage(): Promise<{ rss: number; heapUsed: number; heapTotal: number; external: number; arrayBuffers: number }> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    const response = await this.tuiConnection.sendCommand({ kind: 'MEMORY_USAGE' });
    if (response.data.kind !== 'MEMORY_USAGE') {
      throw new Error(`Unexpected response: ${JSON.stringify(response)}`);
    }
    return response.data.data;
  }

  /**
   * Forces garbage collection in the TUI process.
   */
  async forceGC(): Promise<void> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    await this.tuiConnection.sendCommand({ kind: 'FORCE_GC' });
  }

  /**
   * Gets Rust backend session state (AgentSnapshot).
   */
  async getAgentState(): Promise<unknown> {
    if (!this.agentConnection) throw new Error('Agent not connected');
    const response = await this.agentConnection.sendCommand({ kind: 'GET_AGENT_STATE' });
    if (response.data.kind !== 'GET_AGENT_STATE') {
      throw new Error(`Unexpected response: ${JSON.stringify(response)}`);
    }
    return response.data.data;
  }

  /**
   * Get the session ID, waiting for it to be available.
   */
  async getSessionId(timeout_ms: number = 10000): Promise<string> {
    const startTime = Date.now();
    while (Date.now() - startTime < timeout_ms) {
      const store = await this.getStore();
      if (store.sessionId) {
        return store.sessionId;
      }
      await this.sleepMs(50);
    }
    throw new Error('Timeout waiting for session ID');
  }

  /**
   * Push mock send_message response events to the agent's IpcMockApiClient.
   * - `events`: Array of MockStreamItem events to add to the response stream
   * - `null`: Signal that the current response is complete (closes the stream)
   */
  async pushSendMessageResponse(events: MockStreamItem[] | null): Promise<void> {
    if (!this.agentConnection) throw new Error('Agent not connected');

    // Get session ID from TUI store
    const store = await this.getStore();
    const sessionId = store.sessionId;
    if (!sessionId) throw new Error('No session ID available');

    const cmd = {
      kind: 'PUSH_SEND_MESSAGE_RESPONSE' as const,
      session_id: sessionId,
      events
    };
    const eventsDesc = events ? `${events.length} events (${JSON.stringify(cmd).length} bytes)` : 'null (end stream)';
    console.log(`Sending to agent: ${eventsDesc}`);

    const response = await this.agentConnection.sendCommand(cmd);

    if (response.data.kind === 'ERROR') {
      throw new Error(`Failed to push send_message response: ${response.data.error}`);
    }
  }

  async terminalSnapshot(): Promise<TerminalSnapshot> {
    return this.ptyManager.terminalSnapshot();
  }

  /**
   * Returns the current terminal screen as rendered by xterm.
   * Each element is one row of the terminal (no escape codes).
   */
  getSnapshot(): string[] {
    return this.ptyManager.getSnapshot();
  }

  /**
   * Returns the snapshot with a terminal border for display.
   */
  getSnapshotFormatted(): string {
    return this.ptyManager.getSnapshotFormatted();
  }

  /**
   * Returns the terminal screen as HTML with inline styles for colors.
   */
  getSnapshotHtml(): string {
    return this.ptyManager.getSnapshotHtml();
  }

  /**
   * Waits for text to be visible on the terminal screen.
   */
  waitForText(text: string, timeout?: number): Promise<void> {
    return this.ptyManager.waitForVisibleText(text, timeout ?? this.options.timeout);
  }

  /**
   * Waits for the TUI to finish processing (isProcessing becomes false).
   */
  async waitForIdle(timeout = 30000): Promise<void> {
    const start = Date.now();
    while (Date.now() - start < timeout) {
      const store = await this.getStore();
      if (!store.isProcessing) return;
      await this.sleepMs(100);
    }
    throw new Error('Timeout waiting for TUI to become idle');
  }

  expectExit(): Promise<number> {
    return this.ptyManager.expectExit();
  }

  /**
   * Returns the PID of the spawned CLI process.
   */
  getPid(): number | undefined {
    return this.ptyManager.getPid();
  }

  /**
   * Returns the path to the TUI log file.
   */
  getTuiLogPath(): string {
    return this.paths.tuiLogFile;
  }

  private waitForTuiConnection(): Promise<void> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error('Timeout waiting for TUI IPC connection'));
      }, 15000);

      this.tuiIpcServer.on('connection', () => {
        clearTimeout(timer);
        resolve();
      });
    });
  }

  private waitForAgentConnection(): Promise<void> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error('Timeout waiting for agent IPC connection'));
      }, 15000);

      this.agentIpcServer.on('connection', () => {
        clearTimeout(timer);
        resolve();
      });
    });
  }
}

export class E2ETestCaseBuilder {
  private options: E2ETestCaseOptions = {};

  withTerminal(size: { width: number; height: number }): E2ETestCaseBuilder {
    this.options.terminalSize = size;
    return this;
  }

  withTimeout(ms: number): E2ETestCaseBuilder {
    this.options.timeout = ms;
    return this;
  }

  withTestName(name: string): E2ETestCaseBuilder {
    this.options.testName = name;
    return this;
  }

  async launch(): Promise<E2ETestCase> {
    const testCase = new E2ETestCase(this.options);
    return testCase.launch();
  }
}
