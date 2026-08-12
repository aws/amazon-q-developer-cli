import { mkdtempSync, rmSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { SerializedAppState } from '../shared/ipc-types';
import type {
  CellAttributes,
  KittyStackState,
  TerminalSnapshot,
} from '../shared/pty-manager';
import type { TestPaths } from '../shared/test-paths';
import { TestCase, type TestCaseOptions } from '../TestCase';
import type { SessionInfoEntry } from '../../types/session-client';
import { AcpMockServer } from './AcpMockServer';

export interface AcpTestCaseOptions extends TestCaseOptions {
  mockKasSessionListResult?: SessionInfoEntry[];
}

export class AcpTestCase {
  public readonly mock: AcpMockServer;
  private readonly inner: TestCase;
  private mockSocketDir?: string;

  constructor(options: AcpTestCaseOptions = {}) {
    const mockSocketPath = this.createMockSocketPath();
    this.mock = new AcpMockServer(mockSocketPath);
    const { mockKasSessionListResult, extraEnv, ...rest } = options;
    this.inner = new TestCase({
      ...rest,
      settings: rest.settings ?? {},
      extraEnv: {
        ...extraEnv,
        KIRO_MOCK_ACP: '',
        KIRO_AGENT_ENGINE: 'kas',
        KIRO_ACP_MOCK_SOCKET: mockSocketPath,
        KIRO_TEST_MOCK_KAS_SESSIONS: JSON.stringify(
          mockKasSessionListResult ?? []
        ),
      },
    });
  }

  private createMockSocketPath(): string {
    if (process.platform === 'win32') {
      return `\\\\.\\pipe\\kiro-acp-mock-${randomUUID()}`;
    }

    const socketBaseDir =
      process.platform === 'darwin' ? '/private/tmp' : tmpdir();
    this.mockSocketDir = mkdtempSync(join(socketBaseDir, 'kiro-acp-mock-'));
    return join(this.mockSocketDir, 'acp.sock');
  }

  async launch(): Promise<this> {
    await this.mock.listen();
    await this.inner.launch();
    return this;
  }

  async launchWithoutWaiting(): Promise<this> {
    await this.mock.listen();
    await this.inner.launchWithoutWaiting();
    return this;
  }

  waitForReady(): Promise<void> {
    return this.inner.waitForReady();
  }

  async cleanup(): Promise<void> {
    await this.inner.cleanup();
    try {
      await this.mock.close();
    } catch {
      // Ignore close errors during teardown.
    }
    if (this.mockSocketDir) {
      try {
        rmSync(this.mockSocketDir, { recursive: true, force: true });
      } catch {
        // Ignore temp dir cleanup failures during teardown.
      }
    }
  }

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
  findTextCells(text: string): CellAttributes[] | null {
    return this.inner.findTextCells(text);
  }
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
  waitForRawOutput(sequences: string[], timeoutMs?: number): Promise<void> {
    return this.inner.waitForRawOutput(sequences, timeoutMs);
  }
  sendSignal(signal: NodeJS.Signals): void {
    this.inner.sendSignal(signal);
  }
  resize(cols: number, rows: number): void {
    this.inner.resize(cols, rows);
  }
  getKittyStack(): KittyStackState {
    return this.inner.getKittyStack();
  }
  expectExit(timeoutMs?: number): Promise<number> {
    return this.inner.expectExit(timeoutMs);
  }
  getTestPaths(): TestPaths {
    return this.inner.getTestPaths();
  }
}
