/**
 * E2E TestCase for full-stack testing of Kiro CLI.
 *
 * Spawns the real `kiro-cli chat` command, enabling dual IPC connections
 * to both TUI (Zustand store) and Rust backend session state.
 */

import * as fs from 'fs';
import * as net from 'net';
import * as os from 'os';
import * as path from 'path';
import type { SerializedAppState } from '../src/test-utils/shared/ipc-types';
import { PtyManager, TerminalSnapshot } from '../src/test-utils/shared/pty-manager';
import type { CellAttributes } from '../src/test-utils/shared/pty-manager';
import { createTestDir, type TestPaths } from '../src/test-utils/shared/test-paths';
import { requireChatCliBin } from '../src/utils/chat-cli-bin';
import { TuiIpcConnection } from '../src/test-utils/shared/tui-ipc-connection';
import type { MockStreamItem } from './types/chat-cli';
import { AcpTestHelper } from './AcpTestHelper';

interface E2ETestCaseOptions {
  terminalSize?: { width: number; height: number };
  timeout?: number;
  testName?: string;
  extraEnv?: Record<string, string>;
  extraCliArgs?: string[];
  globalAgentConfigs?: Array<{ name: string; config: Record<string, unknown> }>;
  /** User settings written to $HOME/.kiro/settings/cli.json before launch. */
  settings?: Record<string, unknown>;
  /** Files to write into the sandbox HOME before the CLI spawns. Paths are relative to $HOME. */
  prelaunchFiles?: Array<{ path: string; content: string }>;
  /** Override the spawned process's working directory. Defaults to `process.cwd()`. */
  cwd?: string;
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
  private acpHelpers: AcpTestHelper[] = [];
  /** The sandbox env vars passed to the spawned CLI process. */
  private sandboxEnv: Record<string, string>;
  /** Temp directory for test isolation. Cleaned up on cleanup(). */
  readonly sandboxDir: string;

  constructor(options: E2ETestCaseOptions = {}) {
    this.options = {
      terminalSize: { width: 120, height: 40 },
      timeout: 30000,
      ...options,
    };

    const testName = this.options.testName || `e2e-${Date.now()}`;
    this.paths = createTestDir(testName);

    // Create isolated sandbox directory for sessions, DB, agents
    this.sandboxDir = fs.mkdtempSync(path.join(os.tmpdir(), `kiro-e2e-${testName}-`));
    const homeDir = this.sandboxDir;

    // Write custom agent configs into sandbox agents dir
    const agentsDir = path.join(homeDir, 'agents');
    fs.mkdirSync(agentsDir, { recursive: true });
    for (const agent of this.options.globalAgentConfigs ?? []) {
      fs.writeFileSync(path.join(agentsDir, `${agent.name}.json`), JSON.stringify(agent.config));
    }

    // Default to TUI mode so the first-launch UI-mode picker never blocks E2E
    // flows. Individual tests can still override this with withGlobalSettings().
    const settings = { 'chat.ui.mode': 'tui', ...(this.options.settings ?? {}) };
    const settingsPath = path.join(homeDir, '.kiro', 'settings', 'cli.json');
    fs.mkdirSync(path.dirname(settingsPath), { recursive: true });
    fs.writeFileSync(settingsPath, JSON.stringify(settings));

    // Write any prelaunch files into the sandbox HOME before the CLI spawns
    for (const file of this.options.prelaunchFiles ?? []) {
      const target = path.join(homeDir, file.path);
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, file.content);
    }

    const chatPath = requireChatCliBin();
    const tuiJsPath = path.join(__dirname, '../dist/tui.js');

    this.sandboxEnv = {
      CI: 'false',
      KIRO_CHAT_UI: 'tui',
      // Rollout-gated debug/test builds can show the first-launch mode picker
      // when no mode is specified. E2E tests default to the full TUI and opt
      // into lite explicitly via withLite().
      KIRO_UI_MODE: 'tui',
      KIRO_TEST_MODE: '1',
      KIRO_DISABLE_TELEMETRY: '1',
      KIRO_INPUT_METRICS: 'true',
      FORCE_COLOR: '3',
      COLORTERM: 'truecolor',
      KIRO_TEST_TUI_IPC_SOCKET_PATH: this.paths.tuiIpcSocket,
      KIRO_TEST_CHAT_IPC_SOCKET_PATH: this.paths.agentIpcSocket,
      ...(process.platform === 'win32' ? {
        KIRO_TEST_CHAT_IPC_PIPE_NAME: this.paths.agentIpcSocket,
      } : {}),
      KIRO_TEST_TUI_JS_PATH: tuiJsPath,
      KIRO_CHAT_CLI_BIN: chatPath,
      KIRO_TUI_LOG_FILE: this.paths.tuiLogFile,
      KIRO_TUI_LOG_LEVEL: 'trace',
      KIRO_CHAT_LOG_FILE: this.paths.rustLogFile,
      KIRO_LOG_LEVEL: 'chat_cli=debug,agent=debug,semantic_search_client=trace',
      HOME: homeDir,
      USERPROFILE: homeDir,
      KIRO_TEST_SESSIONS_DIR: homeDir,
      KIRO_TEST_DB_PATH: path.join(homeDir, 'test.sqlite3'),
      KIRO_TEST_AGENTS_DIR: path.join(homeDir, 'agents'),
      ...this.options.extraEnv,
    };

    this.ptyManager = new PtyManager({
      width: this.options.terminalSize!.width,
      height: this.options.terminalSize!.height,
      cwd: this.options.cwd ?? process.cwd(),
      env: this.sandboxEnv,
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
    // Clean up existing sockets (not needed for Windows named pipes)
    if (process.platform !== 'win32') {
      try { fs.unlinkSync(this.paths.tuiIpcSocket); } catch { /* ignore */ }
      try { fs.unlinkSync(this.paths.agentIpcSocket); } catch { /* ignore */ }
    }

    // Start both IPC servers
    await Promise.all([
      new Promise<void>((resolve, reject) => {
        const onError = (error: Error) => {
          this.tuiIpcServer.off('error', onError);
          reject(error);
        };
        this.tuiIpcServer.once('error', onError);
        this.tuiIpcServer.listen(this.paths.tuiIpcSocket, () => {
          this.tuiIpcServer.off('error', onError);
          resolve();
        });
      }),
      new Promise<void>((resolve, reject) => {
        const onError = (error: Error) => {
          this.agentIpcServer.off('error', onError);
          reject(error);
        };
        this.agentIpcServer.once('error', onError);
        this.agentIpcServer.listen(this.paths.agentIpcSocket, () => {
          this.agentIpcServer.off('error', onError);
          resolve();
        });
      }),
    ]);

    // Spawn the process inside the PTY.
    // On Windows, spawn bun directly with the TUI bundle. The CLI binary
    // inside ConPTY cannot spawn bun as a child process that properly inherits
    // terminal handles (ConPTY limitation). Spawning bun directly as the
    // ConPTY process matches the production architecture where bun is the
    // outer process and the CLI is spawned as an ACP backend child.
    const chatPath = requireChatCliBin();
    if (process.platform === 'win32') {
      const tuiJsPath = path.join(__dirname, '../dist/tui.js');
      this.ptyManager.spawn('bun', [tuiJsPath, 'chat', ...(this.options.extraCliArgs ?? [])]);
    } else {
      this.ptyManager.spawn(chatPath, ['chat', ...(this.options.extraCliArgs ?? [])]);
    }
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
    // Clean up ACP helpers first
    for (const helper of this.acpHelpers) {
      await helper.close();
    }
    this.acpHelpers = [];

    // Save HTML snapshot before cleanup
    try {
      fs.writeFileSync(this.paths.snapshotHtmlFile, this.getSnapshotHtml());
    } catch { /* ignore if terminal already closed */ }

    this.ptyManager.kill();
    this.tuiConnection?.close();
    this.agentConnection?.close();
    this.tuiIpcServer?.close();
    this.agentIpcServer?.close();

    // Clean up sandbox directory (retry on Windows where files may still be locked)
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        fs.rmSync(this.sandboxDir, { recursive: true, force: true });
        break;
      } catch (e: any) {
        if (e.code === 'EBUSY' && attempt < 2) {
          await new Promise(resolve => setTimeout(resolve, 500));
          continue;
        }
        // Ignore cleanup errors on last attempt — don't fail the test
      }
    }
  }

  /**
   * Launch a separate ACP connection sharing the same sandbox directories.
   * Useful for creating sessions with history before the TUI loads them.
   * The helper is automatically cleaned up when the test case is cleaned up.
   */
  async launchAcpHelper(): Promise<AcpTestHelper> {
    const helper = await AcpTestHelper.spawn({
      env: this.sandboxEnv,
      testName: `${this.options.testName || 'e2e'}-helper-${this.acpHelpers.length}`,
    });
    this.acpHelpers.push(helper);
    return helper;
  }

  async sendKeys(input: string | number[]): Promise<void> {
    return this.ptyManager.sendKeys(input);
  }

  /**
   * Registers a listener for raw PTY output. Used by knight-rider
   * to broadcast terminal data to WebSocket viewers.
   */
  onPtyData(listener: (data: string) => void): void {
    this.ptyManager.onData(listener);
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
   * Waits for slash commands to be registered from the backend.
   * This avoids a race condition where typing a command before
   * the `commands/available` notification arrives results in "Unknown command".
   */
  async waitForSlashCommands(timeout = 10000): Promise<void> {
    const start = Date.now();
    while (Date.now() - start < timeout) {
      const store = await this.getStore();
      if (store.slashCommands.some((cmd) => cmd.source === 'backend')) return;
      await this.sleepMs(100);
    }
    throw new Error('Timeout waiting for slash commands to be registered');
  }


  /**
   * Gets TUI application state (Zustand store).
   */
  async getStore(): Promise<SerializedAppState> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    const response = await this.tuiConnection.sendCommand({ kind: 'GET_STORE' });
    if (response.data.kind !== 'GET_STORE') {
      throw new Error(`Unexpected response: ${JSON.stringify(response)}`);
    }
    return response.data.data;
  }

  /**
   * Polls the store until the predicate returns true, then returns the matching state.
   */
  async waitForStoreCondition(
    predicate: (state: SerializedAppState) => boolean,
    timeout = 10000
  ): Promise<SerializedAppState> {
    const start = Date.now();
    while (Date.now() - start < timeout) {
      const store = await this.getStore();
      if (predicate(store)) return store;
      await this.sleepMs(100);
    }
    throw new Error('Timeout waiting for store condition');
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
   * Wait for the first subagent (child) session to appear in the store — i.e. a
   * session other than the main one, or one carrying a `parentSession`. Throws
   * on timeout.
   */
  async waitForChildSession(timeoutMs = 15000): Promise<string> {
    const child = await this.waitForNewChildSession(new Set(), timeoutMs);
    if (child === null) {
      throw new Error('Timeout waiting for child subagent session to appear');
    }
    return child;
  }

  /**
   * Wait for a real (spawned) child session whose id is not in `known`. Skips
   * `pending:*` placeholder entries, which represent DAG stages that have not
   * spawned yet. Returns the new session id, or null if none appears within the
   * timeout (e.g. a downstream stage that never spawned).
   */
  async waitForNewChildSession(known: Set<string>, timeoutMs = 15000): Promise<string | null> {
    const mainId = await this.getSessionId();
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      const store = await this.getStore();
      const sessions = store.sessions ?? {};
      for (const [id, s] of Object.entries(sessions)) {
        if (id === mainId && !s.parentSession) continue;
        if (id.startsWith('pending:')) continue;
        if (!known.has(id)) return id;
      }
      await this.sleepMs(100);
    }
    return null;
  }

  /**
   * Push mock send_message response events for an explicit session id.
   *
   * Unlike {@link pushSendMessageResponse}, which targets the main TUI session,
   * this targets any session by id — needed for subagent/crew child sessions
   * whose ids are only known at runtime (random UUIDs).
   *
   * The agent-side mock registry blocks an unmocked session's `send_message`
   * until events are pushed, so this can be called lazily after the child
   * session id is discovered from the store.
   */
  async pushSendMessageResponseForSession(
    sessionId: string,
    events: MockStreamItem[] | null,
    options?: { silent?: boolean }
  ): Promise<void> {
    if (!this.agentConnection) throw new Error('Agent not connected');

    const cmd = {
      kind: 'PUSH_SEND_MESSAGE_RESPONSE' as const,
      session_id: sessionId,
      events,
    };
    const eventsDesc = events ? `${events.length} events` : 'null (end stream)';
    if (!options?.silent) {
      console.log(`Sending to agent [session ${sessionId}]: ${eventsDesc}`);
    }

    const response = await this.agentConnection.sendCommand(cmd);
    if (response.data.kind === 'ERROR') {
      throw new Error(`Failed to push send_message response: ${response.data.error}`);
    }
  }

  /**
   * Push mock send_message response events to the agent's IpcMockApiClient.
   * - `events`: Array of MockStreamItem events to add to the response stream
   * - `null`: Signal that the current response is complete (closes the stream)
   */
  async pushSendMessageResponse(events: MockStreamItem[] | null, options?: { silent?: boolean }): Promise<void> {
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
    if (!options?.silent) {
      console.log(`Sending to agent: ${eventsDesc}`);
    }

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
   * Finds the first occurrence of text on the terminal screen and returns
   * per-character formatting attributes (bold, italic, underline, etc.).
   * Returns null if text is not found.
   */
  findTextCells(text: string): CellAttributes[] | null {
    return this.ptyManager.findTextCells(text);
  }

  /**
   * Returns cell attributes for every line containing `text` (top-to-bottom).
   * Used by tests that compare older scrollback rows against newer live-region
   * rows — e.g. /theme reflow assertions where the old row's color must stay
   * frozen and the new row's color must update.
   */
  findAllTextCells(text: string): CellAttributes[][] {
    return this.ptyManager.findAllTextCells(text);
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

  expectExit(timeout_ms?: number): Promise<number> {
    return this.ptyManager.expectExit(timeout_ms);
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
    return this.waitForConnection(
      () => this.tuiConnection,
      this.tuiIpcServer,
      'TUI'
    );
  }

  private waitForAgentConnection(): Promise<void> {
    return this.waitForConnection(
      () => this.agentConnection,
      this.agentIpcServer,
      'agent'
    );
  }

  private waitForConnection(
    getConnection: () => TuiIpcConnection | undefined,
    server: net.Server,
    label: string
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      // Windows named pipes take longer to establish on CI runners
      const timeoutMs = process.platform === 'win32' ? 30000 : 15000;

      const cleanup = () => {
        clearTimeout(timer);
        server.off('connection', onConnection);
        server.off('error', onError);
      };

      const resolveIfConnected = () => {
        if (!getConnection()) return false;
        cleanup();
        resolve();
        return true;
      };

      const onConnection = () => {
        resolveIfConnected();
      };

      const onError = (error: Error) => {
        cleanup();
        reject(error);
      };

      const timer = setTimeout(() => {
        if (resolveIfConnected()) return;
        cleanup();
        reject(new Error(`Timeout waiting for ${label} IPC connection`));
      }, timeoutMs);

      server.once('connection', onConnection);
      server.once('error', onError);

      // Handle race: connection may have arrived before we registered the listener
      resolveIfConnected();
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

  withEnv(env: Record<string, string>): E2ETestCaseBuilder {
    this.options.extraEnv = { ...this.options.extraEnv, ...env };
    return this;
  }

  /**
   * Lite mode. KIRO_LITE_ROLLOUT_ENABLED=1 is required: without it
   * resolveUiMode() (index.tsx) silently falls back to 'tui' under the rollout
   * gate added in commit e4077111c.
   */
  withLite(): E2ETestCaseBuilder {
    return this.withEnv({ KIRO_UI_MODE: 'lite', KIRO_LITE_ROLLOUT_ENABLED: '1' });
  }

  withGlobalAgentConfig(name: string, config: Record<string, unknown>): E2ETestCaseBuilder {
    this.options.globalAgentConfigs = this.options.globalAgentConfigs ?? [];
    this.options.globalAgentConfigs.push({ name, config });
    return this;
  }

  withGlobalSettings(settings: Record<string, unknown>): E2ETestCaseBuilder {
    this.options.settings = { ...this.options.settings, ...settings };
    return this;
  }

  withCliArgs(...args: string[]): E2ETestCaseBuilder {
    this.options.extraCliArgs = [...(this.options.extraCliArgs ?? []), ...args];
    return this;
  }

  withPrelaunchFile(filePath: string, content: string): E2ETestCaseBuilder {
    this.options.prelaunchFiles = this.options.prelaunchFiles ?? [];
    this.options.prelaunchFiles.push({ path: filePath, content });
    return this;
  }

  withCwd(cwd: string): E2ETestCaseBuilder {
    this.options.cwd = cwd;
    return this;
  }

  async launch(): Promise<E2ETestCase> {
    const testCase = new E2ETestCase(this.options);
    return testCase.launch();
  }
}
