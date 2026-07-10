/**
 * End-to-end test harness that runs the real `KasAcpClient` (not
 * `MockSessionClient`) against an in-process mock ACP server.
 *
 * Holds a `TestCase` as an inner field (composition, not inheritance).
 * `TestCase` handles all the generic TUI-under-PTY scaffolding (spawn,
 * IPC, snapshots, keys). This class layers on the wire-level mock:
 *
 *   1. Starts an `AcpMockServer` on a Unix socket before the TUI spawns.
 *   2. Constructs the inner `TestCase` with env vars that make the TUI
 *      select the real `KasAcpClient` and route its transport at that
 *      socket instead of spawning a KAS subprocess.
 *   3. Exposes `.mock` for scripting request handlers and notifications
 *      from the test body.
 *
 * Design note: composing with `TestCase` (vs. inheriting) avoids
 * duplicating its ~400 lines of PTY/IPC/snapshot/store-inspection
 * scaffolding, while keeping the two ACP-mocking strategies in
 * separate classes rather than one branching on env vars. Cost is
 * the ~15 one-liner delegation methods below, which is cheaper than
 * reimplementing the scaffolding or inheriting a mocking feature
 * this class has to explicitly turn off.
 *
 * Mocking-layer contrast with the base `TestCase`:
 *   - `TestCase` (default): mocks at the `SessionClient` interface via
 *     `MockSessionClient`. Events are injected as pre-parsed
 *     `AgentStreamEvent`s; the real ACP client is never constructed.
 *   - `AcpTestCase` (this class): mocks at the JSON-RPC wire level via
 *     `AcpMockServer`. Every byte the real `KasAcpClient` + `@kiro/client`
 *     SDK produce and consume flows through the test.
 *
 * This class intentionally does NOT expose `mockSessionUpdate()`; its
 * TUI instance has `KIRO_MOCK_ACP=''` so `MockSessionClient` isn't wired
 * in the first place. Use `.mock.notify(...)` / `.mock.on(...)` instead.
 *
 * Scope: KAS-only. V2 (RustAcpClient) is on its way out; no reason to
 * invest in mock-transport test infrastructure for it.
 */
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { SerializedAppState } from '../../src/test-utils/shared/ipc-types';
import type {
  CellAttributes,
  TerminalSnapshot,
} from '../../src/test-utils/shared/pty-manager';
import type { TestPaths } from '../../src/test-utils/shared/test-paths';
import { TestCase, type TestCaseOptions } from '../../src/test-utils/TestCase';
import type { SessionInfoEntry } from '../../src/types/session-client';
import { AcpMockServer } from './AcpMockServer';

export interface AcpTestCaseOptions extends TestCaseOptions {
  /**
   * Test-only KAS-side session listing. The spawned Rust binary's
   * merged `chat --list-sessions` always uses these entries instead of
   * spawning a real KAS child - this harness is KAS-only mock and a
   * real KAS spawn would defeat the test isolation. Forwarded to the
   * binary via the `KIRO_TEST_MOCK_KAS_SESSIONS` env var (see
   * `crates/chat-cli/src/util/consts.rs`). Defaults to `[]`; tests
   * that need cross-engine `/chat` picker rows pass entries here.
   */
  mockKasSessionListResult?: SessionInfoEntry[];
}

export class AcpTestCase {
  public readonly mock: AcpMockServer;
  private readonly inner: TestCase;
  private readonly mockSocketDir: string;

  constructor(options: AcpTestCaseOptions = {}) {
    this.mockSocketDir = mkdtempSync(join(tmpdir(), 'kiro-acp-mock-'));
    const mockSocketPath = join(this.mockSocketDir, 'acp.sock');
    this.mock = new AcpMockServer(mockSocketPath);
    const { mockKasSessionListResult, extraEnv, ...rest } = options;
    this.inner = new TestCase({
      ...rest,
      settings: rest.settings ?? {},
      extraEnv: {
        ...extraEnv,
        // Turn off MockSessionClient (base TestCase's default mock path)
        // so the real `createAcpClient()` branch runs.
        KIRO_MOCK_ACP: '',
        // Select KasAcpClient (not RustAcpClient).
        KIRO_AGENT_ENGINE: 'kas',
        // Route the KAS transport at our mock socket instead of spawning KAS.
        KIRO_ACP_MOCK_SOCKET: mockSocketPath,
        // Always hand the Rust binary a pre-built KAS session listing
        // so its `chat --list-sessions` merge never spawns a real KAS
        // child (this harness is KAS-only mock; spawning would defeat
        // the test isolation). Defaults to an empty list.
        KIRO_TEST_MOCK_KAS_SESSIONS: JSON.stringify(
          mockKasSessionListResult ?? []
        ),
      },
    });
  }

  /**
   * Starts the mock server, spawns the TUI, and waits for IPC.
   *
   * The mock server MUST be listening before the TUI spawns so the
   * TUI-side mock transport can connect immediately.
   */
  async launch(): Promise<this> {
    await this.mock.listen();
    await this.inner.launch();
    return this;
  }

  /**
   * Starts the mock server and spawns the TUI without waiting for IPC.
   * Use when exercising pre-render UI (see `TestCase.launchWithoutWaiting`).
   */
  async launchWithoutWaiting(): Promise<this> {
    await this.mock.listen();
    await this.inner.launchWithoutWaiting();
    return this;
  }

  /** Waits for the TUI IPC connection to be established (post pre-render). */
  waitForReady(): Promise<void> {
    return this.inner.waitForReady();
  }

  /**
   * Stops the mock server, tears down the TUI, and cleans up the socket
   * tmpdir. Tolerant of the not-yet-launched state so `afterEach` cleanup
   * is safe even if the test threw before `launch()`.
   */
  async cleanup(): Promise<void> {
    await this.inner.cleanup();
    try {
      await this.mock.close();
    } catch {
      /* ignore close errors during teardown */
    }
    try {
      rmSync(this.mockSocketDir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }

  // --- Delegation of inner TestCase methods tests commonly need ---

  sendKeys(input: string | number[]): Promise<void> {
    return this.inner.sendKeys(input);
  }
  pressEnter(): Promise<void> {
    return this.inner.pressEnter();
  }
  pressEscape(): Promise<void> {
    return this.inner.pressEscape();
  }
  pressCtrlC(): Promise<void> {
    return this.inner.pressCtrlC();
  }
  pressCtrlCTwice(): Promise<void> {
    return this.inner.pressCtrlCTwice();
  }
  sleepMs(ms: number): Promise<void> {
    return this.inner.sleepMs(ms);
  }
  getStore(): Promise<SerializedAppState> {
    return this.inner.getStore();
  }
  terminalSnapshot(): Promise<TerminalSnapshot> {
    return this.inner.terminalSnapshot();
  }
  waitForVisibleText(text: string, timeout?: number): Promise<void> {
    return this.inner.waitForVisibleText(text, timeout);
  }
  /**
   * Polls the TUI's Zustand store until `predicate(state)` returns
   * truthy. See {@link TestCase.waitForStore}.
   */
  waitForStore(
    predicate: (state: SerializedAppState) => boolean,
    timeoutMs?: number,
    pollIntervalMs?: number
  ): Promise<SerializedAppState> {
    return this.inner.waitForStore(predicate, timeoutMs, pollIntervalMs);
  }
  getSnapshotFormatted(): string {
    return this.inner.getSnapshotFormatted();
  }
  getSnapshotHtml(): string {
    return this.inner.getSnapshotHtml();
  }
  getSnapshot(): string[] {
    return this.inner.getSnapshot();
  }
  /**
   * Finds the first occurrence of `text` on the terminal screen and
   * returns the per-character formatting attributes for each cell of
   * the match (bold, italic, underline, fgColor, etc.). Returns null
   * if not found. See {@link TestCase.findTextCells}.
   */
  findTextCells(text: string): CellAttributes[] | null {
    return this.inner.findTextCells(text);
  }
  /**
   * Resets the xterm buffer (visible screen + scrollback) without
   * touching the underlying TUI process. See {@link TestCase.clearTerminal}.
   */
  clearTerminal(): void {
    this.inner.clearTerminal();
  }
  getCursorPosition(): { x: number; y: number } {
    return this.inner.getCursorPosition();
  }
  getOutput(): string {
    return this.inner.getOutput();
  }
  getOutputCleaned(): string {
    return this.inner.getOutputCleaned();
  }
  expectExit(timeoutMs?: number): Promise<number> {
    return this.inner.expectExit(timeoutMs);
  }
  getTestPaths(): TestPaths {
    return this.inner.getTestPaths();
  }
}
