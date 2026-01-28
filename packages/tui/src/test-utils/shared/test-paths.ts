/**
 * Shared utility for test paths (logs, IPC sockets).
 * Used by both integration tests and E2E tests.
 */

import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';

export interface TestPaths {
  /** Base directory for all test artifacts */
  baseDir: string;
  /** TUI log file path */
  tuiLogFile: string;
  /** Rust backend log file path */
  rustLogFile: string;
  /** IPC socket for TUI connection */
  tuiIpcSocket: string;
  /** IPC socket for agent connection (E2E only) */
  agentIpcSocket: string;
  /** HTML snapshot file path */
  snapshotHtmlFile: string;
  /** Test script log file path */
  testLogFile: string;
}

export interface CreateTestDirOptions {
  /** Subdirectory under test-outputs (default: 'e2e') */
  outputSubdir?: string;
}

/**
 * Get all paths for a test.
 * Creates the test directory if it doesn't exist.
 * Cleans the directory if it already exists.
 */
export function createTestDir(testName: string, options: CreateTestDirOptions = {}): TestPaths {
  const subdir = options.outputSubdir || 'e2e';
  const baseDir = path.join(__dirname, `../../../${subdir}_tests/test-outputs`, testName);

  // Clean and recreate directory
  if (fs.existsSync(baseDir)) {
    fs.rmSync(baseDir, { recursive: true });
  }
  fs.mkdirSync(baseDir, { recursive: true });

  // Sockets must be in temp dir due to path length limits
  const socketDir = path.join(os.tmpdir(), 'kiro-cli-tests', testName);
  if (!fs.existsSync(socketDir)) {
    fs.mkdirSync(socketDir, { recursive: true });
  }

  return {
    baseDir,
    tuiLogFile: path.join(baseDir, 'tui.log'),
    rustLogFile: path.join(baseDir, 'rust.log'),
    tuiIpcSocket: path.join(socketDir, 'tui.sock'),
    agentIpcSocket: path.join(socketDir, 'agent.sock'),
    snapshotHtmlFile: path.join(baseDir, 'snapshot.html'),
    testLogFile: path.join(baseDir, 'test.log'),
  };
}
