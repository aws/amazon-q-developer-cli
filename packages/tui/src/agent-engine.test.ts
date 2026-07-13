import { describe, test, expect } from 'vitest';
import { engineSupportsSubagentKill } from './agent-engine.js';

describe('engineSupportsSubagentKill', () => {
  test('true for the v2 (Rust) backend, which implements session/terminate', () => {
    expect(engineSupportsSubagentKill('v2')).toBe(true);
  });

  test('false for KAS (v3), where session/terminate is a no-op', () => {
    expect(engineSupportsSubagentKill('kas')).toBe(false);
  });
});
