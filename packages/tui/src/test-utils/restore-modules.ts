/**
 * mock.module is process-global in bun and is NOT undone by mock.restore():
 * a module mocked in one test file stays mocked for every later test file in
 * the same run. Any file that calls mock.module must therefore restore the
 * real exports when it finishes, or its mocks leak into unrelated files.
 *
 * Call this BEFORE the first mock.module statement so the real modules can
 * still be loaded and snapshotted:
 *
 *   restoreRealModulesAfterAll(import.meta.dir, ['../kiro', 'child_process']);
 *   mock.module('../kiro', () => ({ ... }));
 */
import { afterAll, mock } from 'bun:test';

export function restoreRealModulesAfterAll(
  callerDir: string,
  specifiers: readonly string[]
): void {
  const snapshots: Array<[string, Record<string, unknown>]> = [];
  for (const specifier of specifiers) {
    const resolved = Bun.resolveSync(specifier, callerDir);
    // Loads the real module (must run before it is mocked) and copies its
    // exports, because mock.module mutates existing namespace objects.
    snapshots.push([resolved, { ...require(resolved) }]);
  }
  afterAll(() => {
    for (const [resolved, exports] of snapshots) {
      mock.module(resolved, () => exports);
    }
  });
}
