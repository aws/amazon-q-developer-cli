/**
 * Node-side stand-in for `bun:sqlite` for suites that run under Node and
 * transitively import the session content index without opening it. The
 * throw only guarantees the module graph loads; code that probes for
 * database support catches constructor failures and degrades to a
 * titles-only mode, so a test that does open the index will silently
 * exercise that fallback rather than error.
 */
export class Database {
  constructor() {
    throw new Error('bun:sqlite is unavailable under this runtime');
  }
}
