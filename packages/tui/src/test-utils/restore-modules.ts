/**
 * mock.module is process-global in bun and is NOT undone by mock.restore():
 * a module mocked in one test file stays mocked for every later test file in
 * the same run. Any file that calls mock.module must therefore restore the
 * real exports when it finishes, or its mocks leak into files that never
 * mocked anything — the hardest failure to attribute.
 *
 * Call this BEFORE the file's first mock.module so the real exports can still
 * be captured:
 *
 *   restoreRealModulesAfterAll(import.meta.dir, ['../kiro', 'child_process']);
 *   mock.module('../kiro', () => ({ ... }));
 *
 * A module whose graph contains top-level await cannot be captured this way —
 * it is not evaluated yet when the file runs on its own. Pass those as
 * `[specifier, namespace]`, loading the namespace with a query-suffixed
 * dynamic import (which bypasses the mock registry):
 *
 *   const realAcp = await import('../acp-client?real');
 *   restoreRealModulesAfterAll(import.meta.dir, [['../acp-client', realAcp]]);
 */
import { afterAll, mock } from 'bun:test';

type ModuleEntry = string | readonly [string, object];

export function restoreRealModulesAfterAll(
  callerDir: string,
  modules: readonly ModuleEntry[]
): void {
  const snapshots: Array<[string, Record<string, unknown>]> = [];
  for (const entry of modules) {
    const [specifier, namespace] =
      typeof entry === 'string' ? [entry, undefined] : entry;
    const resolved = Bun.resolveSync(specifier, callerDir);
    // Copy the exports: mock.module mutates the object it is given, so a live
    // reference would be overwritten by this file's own mocks. The require
    // path captures the instance already in use, so module-level singletons
    // survive; the namespace path re-registers a freshly evaluated instance,
    // which suits only modules that hold no state of their own.
    snapshots.push([
      resolved,
      { ...(namespace ?? (require(resolved) as Record<string, unknown>)) },
    ]);
  }
  afterAll(() => {
    for (const [resolved, exports] of snapshots) {
      mock.module(resolved, () => exports);
    }
  });
}
