/**
 * Preload for E2E tests — rebuilds dist/tui.js so tests always run against latest source.
 * Activated via bunfig.toml [test] preload when running tests from e2e_tests/.
 */
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

const TUI_ROOT = resolve(import.meta.dir, '..');

// Only build when running E2E tests
const isE2E = process.argv.some(a => a.includes('e2e_tests'));
if (isE2E) {
  console.log(`[e2e preload] Building TUI...`);
  const result = spawnSync('bun', ['run', 'build'], {
    cwd: TUI_ROOT,
    stdio: 'inherit',
    env: { ...process.env, NODE_ENV: 'production' },
  });
  if (result.status !== 0) {
    console.error('TUI build failed');
    process.exit(1);
  }
}
