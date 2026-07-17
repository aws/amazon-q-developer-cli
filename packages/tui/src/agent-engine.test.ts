import { describe, test, expect } from 'vitest';
import {
  engineSupportsMcpCommandActions,
  engineSupportsSubagentKill,
} from './agent-engine.js';

describe('engineSupportsSubagentKill', () => {
  test('true for the v2 (Rust) backend, which implements session/terminate', () => {
    expect(engineSupportsSubagentKill('v2')).toBe(true);
  });

  test('false for KAS (v3), where session/terminate is a no-op', () => {
    expect(engineSupportsSubagentKill('kas')).toBe(false);
  });
});

describe('engineSupportsMcpCommandActions', () => {
  test('true for V2, which implements command-backed MCP mutations', () => {
    expect(engineSupportsMcpCommandActions('v2')).toBe(true);
  });

  test('false for KAS, which only exposes snapshots and reset-server OAuth', () => {
    expect(engineSupportsMcpCommandActions('kas')).toBe(false);
  });
});
