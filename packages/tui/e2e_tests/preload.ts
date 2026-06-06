/**
 * Preload for E2E tests — rebuilds dist/tui.js so tests always run against latest source.
 * Activated via bunfig.toml [test] preload when running tests from e2e_tests/.
 */
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

const TUI_ROOT = resolve(import.meta.dir, '..');

// The lite test suite requires the rollout gate to be enabled for /lite,
// /tui, and resolveUiMode() to behave as the tests expect. PR1's commit
// e4077111c (feat(rollout): gate lite TUI mode to internal + nightly only)
// added KIRO_LITE_ROLLOUT_ENABLED='1' as a precondition; without it, /lite
// is a no-op and KIRO_UI_MODE=lite silently falls back to 'tui'. Force it
// on for any lite-* test (integ or e2e).
const isLiteTest = process.argv.some(a => /\blite-/.test(a));
if (isLiteTest) {
  process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
}

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
