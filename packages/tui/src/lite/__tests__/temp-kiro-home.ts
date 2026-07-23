import { beforeAll, afterAll } from 'vitest';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { resetVerboseCache } from '../verbose.js';

/**
 * Redirect KIRO_HOME to a throwaway dir for the suite so verbose-config disk
 * writes don't clobber the developer's real ~/.kiro/settings/cli.json.
 * Registers its own beforeAll/afterAll; call once at the top of a describe-less
 * test module body.
 *
 * Also pins the Lite rollout on and drops the verbose-config cache at both
 * boundaries so neither environment nor cached settings leak between suites.
 */
export function useTempKiroHome(): void {
  let tmpHome: string | undefined;
  let originalKiroHome: string | undefined;
  let originalRollout: string | undefined;
  beforeAll(() => {
    originalKiroHome = process.env.KIRO_HOME;
    tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbose-test-'));
    process.env.KIRO_HOME = tmpHome;
    originalRollout = process.env.KIRO_LITE_ROLLOUT_ENABLED;
    process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
    resetVerboseCache();
  });
  afterAll(() => {
    resetVerboseCache();
    if (originalRollout === undefined) {
      delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
    } else {
      process.env.KIRO_LITE_ROLLOUT_ENABLED = originalRollout;
    }
    if (originalKiroHome === undefined) {
      delete process.env.KIRO_HOME;
    } else {
      process.env.KIRO_HOME = originalKiroHome;
    }
    if (tmpHome) {
      try {
        rmSync(tmpHome, { recursive: true, force: true });
      } catch {
        // best-effort cleanup
      }
    }
  });
}
