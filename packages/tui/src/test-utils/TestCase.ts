import * as net from 'net';
import * as fs from 'fs';
import type { AgentStreamEvent } from '../types/agent-events';
import type { AppState } from '../stores/app-store';
import { PtyManager, TerminalSnapshot } from './shared/pty-manager';
import { getMockSessionClient } from './MockSessionClient';
import { TuiIpcConnection } from './shared/tui-ipc-connection';
import { createTestDir, type TestPaths } from './shared/test-paths';

interface TestCaseOptions {
  args?: string[];
  terminalSize?: { width: number; height: number };
  logLevel?: 'debug' | 'info' | 'warn' | 'error';
  timeout?: number;
  testName?: string;
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

    this.ptyManager = new PtyManager({
      width: this.options.terminalSize!.width,
      height: this.options.terminalSize!.height,
      cwd: process.cwd(), // TODO - use temp dir instead
      env: {
        KIRO_TEST_MODE: 'true',
        KIRO_MOCK_ACP: 'true',
        KIRO_TEST_TUI_IPC_SOCKET_PATH: this.paths.tuiIpcSocket,
        KIRO_TUI_LOG_FILE: this.paths.tuiLogFile,
        KIRO_AGENT_PATH: 'mock-agent-path',
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
    await new Promise<void>((resolve, reject) => {
      this.ipcServer!.listen(this.paths.tuiIpcSocket, (error?: Error) => {
        if (error) reject(error);
        else resolve();
      });
    });

    this.ptyManager.spawn('bun', ['run', 'src/index.tsx']);

    console.log(`TUI logs: ${this.paths.tuiLogFile}`);
    console.log(`Rust logs: ${this.paths.rustLogFile}`);
    console.log(`Snapshot: ${this.paths.snapshotHtmlFile}`);

    await this.waitForConnection();

    return this;
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
   * @returns Promise resolving to the current AppState
   * @throws Error if IPC communication fails
   * @example
   * ```typescript
   * const state = await testCase.getStore();
   * expect(state.input.lines[0]).toBe('hello');
   * expect(state.exitSequence).toBe(1);
   * ```
   */
  async getStore(): Promise<AppState> {
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
    const mockClient = getMockSessionClient();
    if (!mockClient) throw new Error('Mock client not available');
    mockClient.injectEvent(event);
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
  async expectExit(): Promise<number> {
    // Save HTML snapshot before exit
    try {
      fs.writeFileSync(this.paths.snapshotHtmlFile, this.getSnapshotHtml());
    } catch {
      /* ignore if terminal already closed */
    }

    return this.ptyManager.expectExit();
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
   * Creates and launches the configured TestCase.
   *
   * @returns Promise resolving to the launched TestCase instance
   */
  async launch(): Promise<TestCase> {
    const testCase = new TestCase(this.options);
    return testCase.launch();
  }
}
