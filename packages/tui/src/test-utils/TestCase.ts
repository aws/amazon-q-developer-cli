import * as net from 'net';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import type { AgentStreamEvent } from '../types/agent-events';
import type { SerializedAppState } from './shared/ipc-types';
import { PtyManager, TerminalSnapshot } from './shared/pty-manager';
import type { CellAttributes } from './shared/pty-manager';
import { TuiIpcConnection } from './shared/tui-ipc-connection';
import { createTestDir, type TestPaths } from './shared/test-paths';
import { resolveChatCliBin } from './chat-cli-bin';

export interface TestCaseOptions {
  args?: string[];
  terminalSize?: { width: number; height: number };
  logLevel?: 'debug' | 'info' | 'warn' | 'error';
  timeout?: number;
  testName?: string;
  /** Extra environment variables merged into the spawned process env. */
  extraEnv?: Record<string, string>;
  /**
   * Working directory the TUI process is spawned in. The TUI calls
   * `process.cwd()` to bucket sessions and pass workspace paths to
   * the agent, so a controlled cwd lets a test exercise the same
   * paths a real shell would. Defaults to the test runner's cwd.
   */
  cwd?: string;
  /**
   * User settings written to a sandboxed `$KIRO_HOME/settings/cli.json`
   * before launch. Set via {@link TestCaseBuilder.withGlobalSettings}.
   * Use this instead of relying on the developer's real `~/.kiro` config
   * so tests don't depend on local environment.
   */
  settings?: Record<string, unknown>;
}

/**
 * TestCase provides E2E testing capabilities for the TUI application.
 *
 * It spawns the TUI in a real PTY (pseudo-terminal) for authentic terminal behavior
 * while maintaining IPC communication to inspect internal application state.
 * This dual approach allows tests to validate both terminal output and internal
 * state changes simultaneously.
 *
 * Key features:
 * - Real PTY allocation for authentic terminal interactions
 * - IPC communication for direct access to Zustand store state
 * - MockSessionClient integration for ACP protocol mocking
 * - Per-test log files for debugging
 * - Builder pattern for easy test configuration
 *
 * @example
 * ```typescript
 * const testCase = await TestCase.builder()
 *   .withTerminal({ width: 80, height: 24 })
 *   .withTimeout(10000)
 *   .launch();
 *
 * await testCase.sendKeys('hello');
 * const state = await testCase.getStore();
 * expect(state.input.lines[0]).toBe('hello');
 * ```
 */
export class TestCase {
  private ptyManager: PtyManager;
  private ipcServer: net.Server;
  private paths: TestPaths;
  private options: TestCaseOptions;
  private tuiConnection?: TuiIpcConnection;
  /**
   * Sandbox `$KIRO_HOME` directory, always created so the TUI's `cli-settings`
   * reader is fully isolated from the developer's real
   * `~/.kiro/settings/cli.json`. This test's settings (if any) are written
   * here; otherwise the sandbox stays empty so every setting resolves to its
   * built-in default, keeping snapshots deterministic across machines.
   */
  private sandboxDir?: string;

  constructor(options: TestCaseOptions = {}) {
    this.options = {
      terminalSize: { width: 120, height: 40 },
      timeout: 10000,
      ...options,
    };

    const testName = this.options.testName || `integ-${Date.now()}`;
    this.paths = createTestDir(testName, {
      outputSubdir: 'integ',
    });

    // Always sandbox $KIRO_HOME so the TUI's cli-settings reader never falls
    // through to the developer's real ~/.kiro/settings/cli.json. Otherwise
    // snapshots silently depend on local config — e.g. chat.allowAsciiArt
    // flips glyphs between Unicode and ASCII, and chat.showThinking changes
    // rendered rows. When the test provides settings we write them; otherwise
    // the sandbox stays empty so every setting resolves to its built-in
    // default.
    this.sandboxDir = fs.mkdtempSync(
      path.join(os.tmpdir(), `kiro-integ-${testName}-`)
    );
    const sandboxEnv: Record<string, string> = { KIRO_HOME: this.sandboxDir };
    if (this.options.settings) {
      const settingsPath = path.join(this.sandboxDir, 'settings', 'cli.json');
      fs.mkdirSync(path.dirname(settingsPath), { recursive: true });
      fs.writeFileSync(settingsPath, JSON.stringify(this.options.settings));
    }

    this.ptyManager = new PtyManager({
      width: this.options.terminalSize!.width,
      height: this.options.terminalSize!.height,
      cwd: this.options.cwd ?? process.cwd(),
      env: {
        KIRO_TEST_MODE: 'true',
        KIRO_MOCK_ACP: 'true',
        KIRO_TEST_TUI_IPC_SOCKET_PATH: this.paths.tuiIpcSocket,
        KIRO_TUI_LOG_FILE: this.paths.tuiLogFile,
        // Force TUI mode so tests render deterministically.
        // KIRO_LITE_ROLLOUT_ENABLED can leak in from the ambient env (node-pty
        // inherits process.env); without an explicit UI mode that would put
        // in-cohort runs into the "try Lite" welcome nudge path and make boot
        // output nondeterministic. withLite() overrides this to 'lite' via
        // extraEnv (spread last).
        KIRO_UI_MODE: 'tui',
        // Default to the locally-resolved chat_cli (env -> CARGO_TARGET_DIR
        // -> repo target/debug). Tests that explicitly set
        // KIRO_CHAT_CLI_BIN via extraEnv (e.g. stubbed binaries) override
        // this. Production launchers always set the env var explicitly.
        KIRO_CHAT_CLI_BIN: resolveChatCliBin(),
        ...sandboxEnv,
        ...options.extraEnv,
      },
    });

    this.ipcServer = net.createServer((socket) => {
      this.tuiConnection = new TuiIpcConnection(socket);
    });
  }

  /**
   * Creates a new TestCaseBuilder for fluent configuration.
   * @returns A new TestCaseBuilder instance
   */
  static builder(): TestCaseBuilder {
    return new TestCaseBuilder();
  }

  /**
   * Launches the TUI process in a PTY and establishes IPC communication.
   * Creates a Unix socket for IPC, spawns the TUI with test environment variables,
   * and waits for the IPC connection to be established.
   *
   * @returns Promise that resolves to this TestCase instance when ready
   * @throws Error if IPC connection fails to establish within timeout
   */
  async launch(): Promise<TestCase> {
    await this.startIpcServer();
    this.spawnTui();
    await this.waitForConnection();
    return this;
  }

  /**
   * Launches the TUI process without waiting for IPC connection.
   *
   * Use this when the TUI shows a pre-render UI (e.g. --resume-picker) that
   * blocks before the React app renders and IPC connects. After interacting
   * with the pre-render UI, call {@link waitForReady} to wait for IPC.
   */
  async launchWithoutWaiting(): Promise<TestCase> {
    await this.startIpcServer();
    this.spawnTui();
    return this;
  }

  /**
   * Waits for the IPC connection to be established.
   * Call after {@link launchWithoutWaiting} once any pre-render UI interaction is done.
   */
  async waitForReady(): Promise<void> {
    await this.waitForConnection();
  }

  private async startIpcServer(): Promise<void> {
    await new Promise<void>((resolve, reject) => {
      this.ipcServer!.listen(this.paths.tuiIpcSocket, (error?: Error) => {
        if (error) reject(error);
        else resolve();
      });
    });
  }

  private spawnTui(): void {
    // Absolute path so the spawn works regardless of `options.cwd`.
    // `__dirname` resolves to `packages/tui/src/test-utils`; the TUI
    // entrypoint sits two levels up at `packages/tui/src/index.tsx`.
    const tuiEntry = path.resolve(__dirname, '..', 'index.tsx');
    this.ptyManager.spawn('bun', [
      'run',
      tuiEntry,
      ...(this.options.args || []),
    ]);

    console.log(`TUI logs: ${this.paths.tuiLogFile}`);
    console.log(`Rust logs: ${this.paths.rustLogFile}`);
    console.log(`Snapshot: ${this.paths.snapshotHtmlFile}`);
  }

  /**
   * Cleans up the test case by terminating the PTY process and closing IPC connections.
   * Should be called when the test is complete, though many tests may not need this
   * if the process exits naturally.
   */
  async cleanup(): Promise<void> {
    // Save HTML snapshot before cleanup
    try {
      fs.writeFileSync(this.paths.snapshotHtmlFile, this.getSnapshotHtml());
    } catch {
      /* ignore if terminal already closed */
    }

    this.ptyManager.kill();
    this.tuiConnection?.close();

    // Close the IPC listener so its socket / named pipe is released. Without
    // this the server leaks until the test process exits. On Unix that is
    // harmless (the next test recreates a fresh socket path), but on Windows
    // the leaked named pipe keeps its name reserved, so any later test that
    // reuses the same testName fails to listen (EADDRINUSE).
    await new Promise<void>((resolve) => {
      if (!this.ipcServer.listening) {
        resolve();
        return;
      }
      this.ipcServer.close(() => resolve());
    });

    // Clean up sandbox $KIRO_HOME directory if one was created.
    if (this.sandboxDir) {
      try {
        fs.rmSync(this.sandboxDir, { recursive: true, force: true });
      } catch {
        /* ignore cleanup errors */
      }
    }
  }

  /**
   * Sends keystrokes or raw bytes to the PTY.
   *
   * @param input - String to type or array of byte values (e.g., [0x03] for Ctrl+C)
   * @example
   * ```typescript
   * await testCase.sendKeys('hello world');
   * await testCase.sendKeys([0x03]); // Ctrl+C
   * ```
   */
  async sendKeys(input: string | number[]): Promise<void> {
    return this.ptyManager.sendKeys(input);
  }

  /** Send Enter key */
  async pressEnter(): Promise<void> {
    return this.sendKeys('\r');
  }

  /**
   * Type text, then Enter, with a render-cycle delay between. Without the
   * delay, text+Enter in a single PTY write makes Ink submit an empty input
   * (chars haven't rendered into state when the Enter handler reads them).
   */
  async typeAndSubmit(text: string, settleMs = 150): Promise<void> {
    await this.sendKeys(text);
    await this.sleepMs(settleMs);
    await this.sendKeys('\r');
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

  /**
   * Ends the mock turn (resolves the pending prompt() Promise), so
   * streamMessage() resolves, buffered content commits to the store, and
   * isProcessing flips to false. Tests that need isProcessing to STAY true
   * (e.g. subagent panel tests) must not call this until done mid-turn.
   */
  async completeTurn(): Promise<void> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    const response = await this.tuiConnection.sendCommand({
      kind: 'COMPLETE_TURN',
    });
    if (response.data.kind === 'ERROR') {
      throw new Error((response.data as any).error);
    }
  }

  /**
   * Pauses test execution for the specified duration.
   *
   * @param ms - Milliseconds to sleep
   */
  async sleepMs(ms: number): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Retrieves the current application state from the running TUI process via IPC.
   * This provides direct access to the Zustand store state for assertions.
   *
   * @returns Promise resolving to the current SerializedAppState
   * @throws Error if IPC communication fails
   * @example
   * ```typescript
   * const state = await testCase.getStore();
   * expect(state.input.lines[0]).toBe('hello');
   * expect(state.exitSequence).toBe(1);
   * ```
   */
  async getStore(): Promise<SerializedAppState> {
    if (!this.tuiConnection) throw new Error('TUI not connected');

    const response = await this.tuiConnection.sendCommand({
      kind: 'GET_STORE',
    });
    if (response.data.kind !== 'GET_STORE') {
      throw new Error(
        `Received unexpected response: ${JSON.stringify(response)}`
      );
    }

    return response.data.data;
  }

  /**
   * Injects a mock session event into the MockSessionClient.
   * This allows tests to simulate ACP protocol events like content chunks,
   * tool calls, and approval requests.
   *
   * @param event - The AgentStreamEvent to inject
   * @example
   * ```typescript
   * await testCase.mockSessionUpdate({
   *   type: AgentEventType.Content,
   *   content: { type: ContentType.Text, text: 'Hello!' }
   * });
   * ```
   */
  async mockSessionUpdate(event: AgentStreamEvent): Promise<void> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    const response = await this.tuiConnection.sendCommand({
      kind: 'MOCK_SESSION_UPDATE',
      event,
    });
    if (response.data.kind === 'ERROR') {
      throw new Error((response.data as any).error);
    }
  }

  /**
   * Test-only: drive `startEditingQueue` directly. The user-facing path
   * goes through the activity tray (Ctrl+X), which is gated on
   * tasks.length > 0 in lite mode. Tests asserting queue-edit semantics
   * shouldn't have to seed unrelated task state.
   */
  async mockStartEditingQueue(index: number): Promise<void> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    const response = await this.tuiConnection.sendCommand({
      kind: 'MOCK_START_EDITING_QUEUE',
      index,
    });
    if (response.data.kind === 'ERROR') {
      throw new Error((response.data as any).error);
    }
  }

  /**
   * Test-only: seed a subagent stage entry into the store's `sessions` map
   * so the lite layout's subagent panel + kill-ladder paths see it. Used
   * by tests that exercise subagentSessionIdByName lookups (Ctrl+X kill
   * ladder, panel auto-expand) without orchestrating a full real
   * subagent_list_update event.
   */
  async mockAddSession(session: {
    id: string;
    name: string;
    agentName?: string;
    status?: 'busy' | 'pending' | 'terminated';
  }): Promise<void> {
    if (!this.tuiConnection) throw new Error('TUI not connected');
    const response = await this.tuiConnection.sendCommand({
      kind: 'MOCK_ADD_SESSION',
      session,
    });
    if (response.data.kind === 'ERROR') {
      throw new Error((response.data as any).error);
    }
  }

  // /**
  //  * Injects an error into the test session.
  //  *
  //  * @param error - Error message to inject
  //  */
  // async mockError(error: string): Promise<void> {
  //   await this.sendIpcRequest({ kind: 'MOCK_ERROR', error });
  // }

  /**
   * Captures the current terminal output as a snapshot for analysis.
   *
   * @returns Promise resolving to a TerminalSnapshot with analysis methods
   */
  async terminalSnapshot(): Promise<TerminalSnapshot> {
    return this.ptyManager.terminalSnapshot();
  }

  /**
   * Waits for specific text to appear in the visible terminal screen.
   * Uses xterm to decode escape codes and returns the current viewport state.
   */
  waitForVisibleText(text: string, timeout?: number): Promise<void> {
    return this.ptyManager.waitForVisibleText(text, timeout);
  }

  /**
   * Waits for every given raw escape sequence to appear in the unstripped
   * PTY output. See {@link PtyManager.waitForRawOutput}.
   */
  waitForRawOutput(sequences: string[], timeoutMs?: number): Promise<void> {
    return this.ptyManager.waitForRawOutput(sequences, timeoutMs);
  }

  /**
   * Sends a POSIX signal to the TUI process. See {@link PtyManager.sendSignal}.
   */
  sendSignal(signal: NodeJS.Signals): void {
    this.ptyManager.sendSignal(signal);
  }

  /**
   * Polls the TUI's Zustand store until `predicate(state)` returns
   * truthy or `timeoutMs` elapses. Resolves with the matching state.
   * Throws on timeout. Useful for waiting on async TUI state changes
   * (e.g. session id appearing after `kiro.createSession` resolves)
   * without hand-rolling a polling loop.
   */
  async waitForStore(
    predicate: (state: SerializedAppState) => boolean,
    timeoutMs = 30_000,
    pollIntervalMs = 100
  ): Promise<SerializedAppState> {
    const deadline = Date.now() + timeoutMs;
    let state = await this.getStore();
    while (!predicate(state)) {
      if (Date.now() >= deadline) {
        throw new Error(
          `waitForStore timed out after ${timeoutMs}ms waiting for predicate`
        );
      }
      await this.sleepMs(pollIntervalMs);
      state = await this.getStore();
    }
    return state;
  }

  /**
   * Returns a formatted snapshot of the terminal screen with a border.
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
   * Returns the terminal screen as an array of lines.
   */
  getSnapshot(): string[] {
    return this.ptyManager.getSnapshot();
  }

  /**
   * Finds the first occurrence of `text` on the terminal screen and returns
   * the per-character formatting attributes for each cell of the match
   * (bold, italic, underline, fgColor, etc.). Returns null if not found.
   *
   * Used by markdown-rendering tests to assert that styling attributes
   * survive the markdown -> ANSI -> xterm pipeline.
   */
  findTextCells(text: string): CellAttributes[] | null {
    return this.ptyManager.findTextCells(text);
  }

  /**
   * Cell attributes for every line containing `text` (top-to-bottom). Used by
   * tests comparing old scrollback rows against newer live rows — e.g. /theme
   * reflow, where the old row's color must stay frozen and the new one update.
   */
  findAllTextCells(text: string): CellAttributes[][] {
    return this.ptyManager.findAllTextCells(text);
  }

  /**
   * Resets the xterm buffer (visible screen + scrollback) and the raw
   * output buffer, leaving the underlying TUI process untouched. Used
   * by tests that share a single TUI across multiple cases (e.g. the
   * markdown-rendering suites) so each case starts from a clean screen
   * for `findTextCells()` / `getSnapshot()` lookups.
   */
  clearTerminal(): void {
    this.ptyManager.clearTerminal();
  }

  /**
   * Returns the current terminal cursor position (0-indexed).
   */
  getCursorPosition(): { x: number; y: number } {
    return this.ptyManager.getCursorPosition();
  }

  /**
   * Returns the raw PTY output.
   */
  getOutput(): string {
    return this.ptyManager.getOutput();
  }

  /**
   * Returns the PTY output with ANSI escape codes stripped.
   */
  getOutputCleaned(): string {
    return this.ptyManager.getOutputCleaned();
  }

  /**
   * Waits for the TUI process to exit and returns the exit code.
   * Useful for testing exit scenarios like Ctrl+C sequences.
   *
   * @returns Promise resolving to the process exit code
   * @throws Error if process doesn't exit within timeout
   * @example
   * ```typescript
   * await testCase.sendKeys([0x03, 0x03]); // Double Ctrl+C
   * const exitCode = await testCase.expectExit();
   * expect(exitCode).toBe(0);
   * ```
   */
  async expectExit(timeoutMs?: number): Promise<number> {
    // Save HTML snapshot before exit
    try {
      fs.writeFileSync(this.paths.snapshotHtmlFile, this.getSnapshotHtml());
    } catch {
      /* ignore if terminal already closed */
    }

    return this.ptyManager.expectExit(timeoutMs);
  }

  /**
   * Returns the test paths for this test case (log files, output dir, etc.).
   */
  getTestPaths(): TestPaths {
    return this.paths;
  }

  private waitForConnection(): Promise<void> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error('Failed to establish IPC connection'));
      }, 5000);

      this.ipcServer.on('connection', () => {
        clearTimeout(timer);
        resolve();
      });
    });
  }
}

/**
 * Builder for configuring TestCase instances with a fluent API.
 * Provides methods to set terminal size, timeouts, and other test options
 * before launching the test case.
 */
export class TestCaseBuilder {
  /** Configuration options being built */
  private options: TestCaseOptions = {};

  /**
   * Sets command line arguments to pass to the TUI process.
   *
   * @param args - Array of command line arguments
   * @returns This builder for method chaining
   */
  withArgs(args: string[]): TestCaseBuilder {
    this.options.args = args;
    return this;
  }

  /**
   * Sets the terminal dimensions for the PTY.
   *
   * @param size - Terminal width and height in characters
   * @returns This builder for method chaining
   */
  withTerminal(size: { width: number; height: number }): TestCaseBuilder {
    this.options.terminalSize = size;
    return this;
  }

  /**
   * Sets the log level for the test session.
   *
   * @param level - Log level to use
   * @returns This builder for method chaining
   */
  withLogLevel(level: 'debug' | 'info' | 'warn' | 'error'): TestCaseBuilder {
    this.options.logLevel = level;
    return this;
  }

  /**
   * Sets the timeout for various test operations.
   *
   * @param ms - Timeout in milliseconds
   * @returns This builder for method chaining
   */
  withTimeout(ms: number): TestCaseBuilder {
    this.options.timeout = ms;
    return this;
  }

  /**
   * Sets the test name for output directory naming.
   *
   * @param name - Test name
   * @returns This builder for method chaining
   */
  withTestName(name: string): TestCaseBuilder {
    this.options.testName = name;
    return this;
  }

  /**
   * Sets extra environment variables for the spawned TUI process.
   *
   * @param env - Key-value pairs to merge into the process environment
   * @returns This builder for method chaining
   */
  withEnv(env: Record<string, string>): TestCaseBuilder {
    this.options.extraEnv = { ...this.options.extraEnv, ...env };
    return this;
  }

  /**
   * Launch the TUI in lite mode. Also sets KIRO_LITE_ROLLOUT_ENABLED=1 —
   * without it resolveUiMode() (index.tsx) silently falls back to 'tui' under
   * the rollout gate added in commit e4077111c, so KIRO_UI_MODE=lite alone has
   * no effect in tests.
   */
  withLite(): TestCaseBuilder {
    return this.withEnv({
      KIRO_UI_MODE: 'lite',
      KIRO_LITE_ROLLOUT_ENABLED: '1',
    });
  }

  /**
   * Writes user settings to a sandboxed `$KIRO_HOME/settings/cli.json`
   * before launch and points the spawned TUI at that sandbox via
   * `KIRO_HOME`. Use this when a test depends on a setting that the TUI
   * reads at module load (e.g. `chat.showThinking`) so it doesn't pick
   * up the developer's real `~/.kiro/settings/cli.json`.
   *
   * Calls compose: subsequent invocations merge into the prior settings
   * object.
   *
   * @example
   * ```ts
   * await TestCase.builder()
   *   .withGlobalSettings({ 'chat.showThinking': true })
   *   .launch();
   * ```
   */
  withGlobalSettings(settings: Record<string, unknown>): TestCaseBuilder {
    this.options.settings = { ...this.options.settings, ...settings };
    return this;
  }

  /**
   * Creates and launches the configured TestCase.
   *
   * @returns Promise resolving to the launched TestCase instance
   */
  async launch(): Promise<TestCase> {
    const testCase = new TestCase(this.options);
    return testCase.launch();
  }

  /**
   * Creates and launches the configured TestCase without waiting for IPC.
   * Use for tests that need to interact with pre-render UI (e.g. --resume-picker).
   */
  async launchWithoutWaiting(): Promise<TestCase> {
    const testCase = new TestCase(this.options);
    return testCase.launchWithoutWaiting();
  }
}
