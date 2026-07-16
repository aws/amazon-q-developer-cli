import { beforeAll, afterAll } from 'vitest';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import { resetVerboseCache } from '../verbose.js';

/**
 * Redirect KIRO_HOME to a throwaway dir for the suite so verbose-config disk
 * writes don't clobber the developer's real ~/.kiro/settings/lite_verbose.json.
 * Registers its own beforeAll/afterAll; call once at the top of a describe-less
 * test module body.
 *
 * Also drops the in-memory verbose-config cache at both boundaries: the cache
 * outlives the KIRO_HOME swap, so without the reset a config written by an
 * earlier test file leaks into this one (and this file's writes leak out).
 */
export function useTempKiroHome(): void {
  let tmpHome: string | undefined;
  let originalKiroHome: string | undefined;
  beforeAll(() => {
    originalKiroHome = process.env.KIRO_HOME;
    tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbose-test-'));
    process.env.KIRO_HOME = tmpHome;
    resetVerboseCache();
  });
  afterAll(() => {
    resetVerboseCache();
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
