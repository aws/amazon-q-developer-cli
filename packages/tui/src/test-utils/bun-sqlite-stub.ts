/**
 * Node-side stand-in for `bun:sqlite`, aliased in vitest.config.ts. The
 * vitest suites (hooks/selectors under Node) transitively import the session
 * content index but never open it; this keeps the module graph loadable
 * while failing loudly if a test actually reaches for the database.
 */
export class Database {
  constructor() {
    throw new Error('bun:sqlite is unavailable under vitest (Node runtime)');
  }
}
