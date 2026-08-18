/**
 * Shim `node:sqlite` for `bun test`.
 *
 * From 0.46.0, `@kiro/agent`'s module graph carries a static top-level
 * `import { DatabaseSync } from "node:sqlite"` (the c2s symbolic store's
 * SQLite persistence, kiro-team/kiro-agent#2150). Bun does not implement
 * `node:sqlite` as of 1.3.x, so ANY import of the `@kiro/agent` barrel under
 * `bun test` — e.g. `computeWorkspaceHash` in
 * `acp_integ_tests/session-load-then-type.test.ts` — throws
 * `No such built-in module: node:sqlite` and takes the whole test file down.
 *
 * This preload registers a module mock so the barrel loads. The tests that
 * import the barrel only use pure helpers (path hashing); nothing constructs
 * the analysis store, so the throwing stub below is never reached at runtime —
 * it exists to fail loudly if a test ever DOES touch SQLite-backed KAS code
 * under Bun, instead of silently returning undefined behavior.
 *
 * Delete this shim when either side moves: Bun implements `node:sqlite`
 * (https://github.com/oven-sh/bun/issues — tracked upstream), or KAS makes the
 * sqlite import lazy so importing the barrel no longer requires the builtin.
 */
import { mock } from 'bun:test';

mock.module('node:sqlite', () => ({
  DatabaseSync: class DatabaseSync {
    constructor() {
      throw new Error(
        'node:sqlite is shimmed under bun test (see src/test-utils/node-sqlite-shim-preload.ts). ' +
          'A test reached real SQLite-backed @kiro/agent code, which Bun cannot run — ' +
          'run it under Node, or mock the store.'
      );
    }
  },
}));
