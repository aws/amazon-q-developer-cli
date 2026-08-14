import { describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

const scriptPath = path.resolve('.github/scripts/run-with-retry.sh');

describe('run-with-retry.sh', () => {
  it('preserves the original exit code on the final failing attempt', () => {
    const result = Bun.spawnSync({
      cmd: [
        'bash',
        scriptPath,
        '--attempts',
        '1',
        '--delay-seconds',
        '0',
        '--',
        'bash',
        '-lc',
        'exit 2',
      ],
      stdout: 'pipe',
      stderr: 'pipe',
    });

    expect(result.exitCode).toBe(2);
  });

  it('preserves the last exit code after exhausting retries', () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'run-with-retry-'));
    const statusPath = path.join(tempDir, 'status.txt');
    const result = Bun.spawnSync({
      cmd: [
        'bash',
        scriptPath,
        '--attempts',
        '2',
        '--delay-seconds',
        '0',
        '--status-file',
        statusPath,
        '--',
        'bash',
        '-lc',
        'exit 5',
      ],
      stdout: 'pipe',
      stderr: 'pipe',
    });

    expect(result.exitCode).toBe(5);
    expect(fs.readFileSync(statusPath, 'utf8')).toBe(
      'failed after 2/2 attempts\n'
    );
  });

  it('returns success when a later retry succeeds', () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'run-with-retry-'));
    const statePath = path.join(tempDir, 'attempt-count');
    const statusPath = path.join(tempDir, 'status.txt');
    const command = `count=0
if [ -f "${statePath}" ]; then
  count=$(cat "${statePath}")
fi
count=$((count + 1))
printf '%s' "$count" > "${statePath}"
if [ "$count" -eq 1 ]; then
  exit 5
fi
exit 0`;

    const result = Bun.spawnSync({
      cmd: [
        'bash',
        scriptPath,
        '--attempts',
        '2',
        '--delay-seconds',
        '0',
        '--status-file',
        statusPath,
        '--',
        'bash',
        '-lc',
        command,
      ],
      stdout: 'pipe',
      stderr: 'pipe',
    });

    expect(result.exitCode).toBe(0);
    expect(fs.readFileSync(statePath, 'utf8')).toBe('2');
    expect(fs.readFileSync(statusPath, 'utf8')).toBe(
      'recovered on attempt 2/2\n'
    );
  });

  it('records a first-attempt pass', () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'run-with-retry-'));
    const statusPath = path.join(tempDir, 'status.txt');
    const result = Bun.spawnSync({
      cmd: [
        'bash',
        scriptPath,
        '--attempts',
        '2',
        '--delay-seconds',
        '0',
        '--status-file',
        statusPath,
        '--',
        'bash',
        '-lc',
        'exit 0',
      ],
      stdout: 'pipe',
      stderr: 'pipe',
    });

    expect(result.exitCode).toBe(0);
    expect(fs.readFileSync(statusPath, 'utf8')).toBe('passed on attempt 1/2\n');
  });
});
