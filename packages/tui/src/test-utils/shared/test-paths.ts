/**
 * Shared utility for test paths (logs, IPC sockets).
 * Used by both integration tests and E2E tests.
 */

import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';
import * as crypto from 'crypto';

// macOS `sockaddr_un.sun_path` is 104 bytes (including NUL), Linux is 108.
// Use the stricter limit with a small safety margin so sockets bind reliably
// across platforms.
const MAX_SOCKET_PATH_LEN = 103;

/**
 * Build a socket directory path that keeps the full socket path under the
 * platform's sun_path limit. If the natural path `{tmp}/kiro-cli-tests/{name}/`
 * plus the longest socket filename would exceed the limit, the test name is
 * replaced with a short hash-suffixed variant. The original test name is still
 * used for human-readable artifacts under `baseDir`.
 */
function buildSocketDir(testName: string): string {
  const tmpRoot = path.join(os.tmpdir(), 'kiro-cli-tests');
  // Longest socket filename we will place in this dir.
  const longestSockFile = 'agent.sock';
  const naturalPath = path.join(tmpRoot, testName, longestSockFile);
  if (naturalPath.length <= MAX_SOCKET_PATH_LEN) {
    return path.join(tmpRoot, testName);
  }

  // Too long — derive a shortened, collision-resistant directory name.
  // 8 hex chars of sha1 is plenty for test isolation.
  const hash = crypto
    .createHash('sha1')
    .update(testName)
    .digest('hex')
    .slice(0, 8);
  const overhead =
    tmpRoot.length + path.sep.length + 1 /* '/' */ + longestSockFile.length + 1;
  const available =
    MAX_SOCKET_PATH_LEN - overhead - (hash.length + 1); /* '-' */
  const prefix = testName.slice(0, Math.max(0, available));
  const shortName = prefix ? `${prefix}-${hash}` : hash;
  return path.join(tmpRoot, shortName);
}

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
export function createTestDir(
  testName: string,
  options: CreateTestDirOptions = {}
): TestPaths {
  const subdir = options.outputSubdir || 'e2e';
  const baseDir = path.join(
    __dirname,
    `../../../${subdir}_tests/test-outputs`,
    testName
  );

  // Clean and recreate directory
  if (fs.existsSync(baseDir)) {
    fs.rmSync(baseDir, { recursive: true });
  }
  fs.mkdirSync(baseDir, { recursive: true });

  // Sockets must be in temp dir due to path length limits
  // On Windows, use named pipes instead of Unix sockets
  const isWindows = os.platform() === 'win32';

  let tuiIpcSocket: string;
  let agentIpcSocket: string;

  if (isWindows) {
    // Windows named pipes: Node.js net.createServer supports \\?\pipe\ paths
    const pipePrefix = `\\\\.\\pipe\\kiro-test-${testName.replace(/[^a-zA-Z0-9-]/g, '-')}`;
    tuiIpcSocket = `${pipePrefix}-tui`;
    agentIpcSocket = `${pipePrefix}-agent`;
  } else {
    const socketDir = buildSocketDir(testName);
    // Clean and recreate socket directory to remove stale sockets from previous runs
    if (fs.existsSync(socketDir)) {
      fs.rmSync(socketDir, { recursive: true });
    }
    fs.mkdirSync(socketDir, { recursive: true });
    tuiIpcSocket = path.join(socketDir, 'tui.sock');
    agentIpcSocket = path.join(socketDir, 'agent.sock');
  }

  return {
    baseDir,
    tuiLogFile: path.join(baseDir, 'tui.log'),
    rustLogFile: path.join(baseDir, 'rust.log'),
    tuiIpcSocket,
    agentIpcSocket,
    snapshotHtmlFile: path.join(baseDir, 'snapshot.html'),
    testLogFile: path.join(baseDir, 'test.log'),
  };
}
