import { describe, test, expect } from 'bun:test';
import * as os from 'os';
import * as fs from 'fs';
import * as path from 'path';
import {
  buildSocketDir,
  checkoutScope,
  createTestDir,
} from './shared/test-paths';

const isWindows = os.platform() === 'win32';

function cleanup(paths: { baseDir: string; agentIpcSocket: string }): void {
  fs.rmSync(paths.baseDir, { recursive: true, force: true });
  if (!isWindows) {
    fs.rmSync(path.dirname(paths.agentIpcSocket), {
      recursive: true,
      force: true,
    });
  }
}

describe('test socket path isolation', () => {
  test('different checkout locations yield different socket dirs for the same test', () => {
    const scopeA = checkoutScope('/repos/kiro-cli/packages/tui');
    const scopeB = checkoutScope('/repos/kiro-cli-wt/fix-x/packages/tui');
    expect(scopeA).not.toBe(scopeB);
    expect(buildSocketDir('same-test', scopeA)).not.toBe(
      buildSocketDir('same-test', scopeB)
    );
  });

  test('socket paths stay under the sun_path limit for long test names', () => {
    const longName = 'x'.repeat(200);
    const paths = createTestDir(longName, { outputSubdir: 'integ' });
    try {
      if (!isWindows) {
        expect(paths.tuiIpcSocket.length).toBeLessThanOrEqual(103);
        expect(paths.agentIpcSocket.length).toBeLessThanOrEqual(103);
      }
    } finally {
      cleanup(paths);
    }
  });

  test('fails closed with a clear error when the temp root is too long', () => {
    // An oversized scope stands in for an oversized TMPDIR: both inflate the
    // socket dir past the sun_path budget.
    expect(() => buildSocketDir('any-test', 'a'.repeat(120))).toThrow(
      /Temp dir too long/
    );
  });
});
