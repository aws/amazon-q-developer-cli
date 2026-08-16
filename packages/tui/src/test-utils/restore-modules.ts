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
  const snapshots: Array<
    [string, Promise<Record<string, unknown> | null> | Record<string, unknown>]
  > = [];
  for (const entry of modules) {
    const [specifier, namespace] =
      typeof entry === 'string' ? [entry, undefined] : entry;
    const resolved = Bun.resolveSync(specifier, callerDir);
    // Copy the exports: mock.module mutates the object it is given, so a live
    // reference would be overwritten by this file's own mocks. The require
    // path captures the instance already in use, so module-level singletons
    // survive; the namespace path re-registers a freshly evaluated instance,
    // which suits only modules that hold no state of their own.
    if (namespace) {
      snapshots.push([resolved, { ...namespace }]);
      continue;
    }
    try {
      snapshots.push([
        resolved,
        { ...(require(resolved) as Record<string, unknown>) },
      ]);
    } catch (requireError) {
      // require() rejects modules whose graph contains top-level await (and
      // anything else that fails evaluation). Fall back to a query-suffixed
      // dynamic import (bypasses the mock registry for THIS specifier only),
      // kicked off now and awaited in afterAll. Two limitations, both logged
      // so the caller is prompted toward the pre-awaited namespace form:
      // - the import settles after the caller's own mock.module calls run on
      //   this same synchronous tick, so the freshly evaluated module resolves
      //   its transitive deps against the caller's mocks — "real" only when
      //   none of its dependencies are mocked by this file;
      // - it re-registers a fresh instance, so module-level singletons do not
      //   survive. Still strictly better than throwing here, which would skip
      //   the restore entirely and leak this file's mocks into every later
      //   file. The rejection handler is attached NOW so a failure before
      //   afterAll is captured instead of surfacing as an unhandled rejection.
      console.warn(
        `restoreRealModulesAfterAll: require(${resolved}) failed (${String(
          requireError
        )}); falling back to a deferred ?real import — pass a pre-awaited ` +
          'namespace ([specifier, namespace]) if this module or its deps are mocked'
      );
      snapshots.push([
        resolved,
        import(`${resolved}?real`).then(
          (ns) => ({ ...ns }) as Record<string, unknown>,
          (importError) => {
            console.error(
              `restoreRealModulesAfterAll: ?real import of ${resolved} failed; ` +
                `the module stays mocked for the rest of the run. require error: ` +
                `${String(requireError)}; import error: ${String(importError)}`
            );
            return null;
          }
        ),
      ]);
    }
  }
  afterAll(async () => {
    // Each entry restores independently: one failed snapshot must not cancel
    // the restore of the entries behind it (mock.module is process-global, so
    // a skipped restore leaks into every later test file).
    for (const [resolved, exports] of snapshots) {
      const resolvedExports = await exports;
      if (resolvedExports) {
        mock.module(resolved, () => resolvedExports);
      }
    }
  });
}
