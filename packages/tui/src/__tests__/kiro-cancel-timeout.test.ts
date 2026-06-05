/**
 * Regression test for KAS cancel hang bug.
 *
 * When KAS backend never responds to a prompt RPC after cancel(),
 * pendingPrompt never settles. Without a timeout, cancel() hangs forever,
 * blocking the entire session.
 */

import { describe, it, expect, mock, beforeEach } from 'bun:test';
import { AgentEventType } from '../types/agent-events';
import type { AgentStreamEvent } from '../types/agent-events';

// --- Mock logger ---
mock.module('../utils/logger', () => ({
  logger: {
    debug: () => {},
    error: () => {},
    warn: () => {},
    info: () => {},
  },
}));

// --- Mock AcpClient ---
let mockOnUpdateHandler: ((event: AgentStreamEvent) => void) | null = null;

const mockSessionClient = {
  sessionId: undefined as string | undefined,
  initialize: mock(() => Promise.resolve()),
  newSession: mock(() => {
    mockSessionClient.sessionId = 'session-1';
    return Promise.resolve({
      sessionId: 'session-1',
      currentModel: { id: 'model-1', name: 'Test Model' },
      currentAgent: { name: 'test-agent', welcomeMessage: 'Welcome!' },
    });
  }),
  loadSession: mock((id: string) =>
    Promise.resolve({
      sessionId: id,
      currentModel: { id: 'model-1', name: 'Test Model' },
      currentAgent: { name: 'test-agent' },
    })
  ),
  prompt: mock(() => Promise.resolve()),
  cancel: mock(() => Promise.resolve()),
  close: mock(() => {}),
  onUpdate: mock((handler: (event: AgentStreamEvent) => void) => {
    mockOnUpdateHandler = handler;
    return () => {
      mockOnUpdateHandler = null;
    };
  }),
  executeCommand: mock(() => Promise.resolve({ success: true, message: 'ok' })),
  getCommandOptions: mock(() => Promise.resolve({ options: [] })),
  setMode: mock(() => Promise.resolve()),
  listSettings: mock(() => Promise.resolve({ 'chat.theme': 'dark' })),
  setSetting: mock(() => Promise.resolve()),
  terminateSession: mock(() => Promise.resolve()),
  listSessions: mock(() => Promise.resolve({ sessions: [] })),
  resolveSpecSession: mock((req: { featureName: string }) =>
    Promise.resolve({ sessionId: `spec-${req.featureName}` })
  ),
  invokeSpec: mock((req: { sessionId: string }) =>
    Promise.resolve({ sessionId: req.sessionId, executionId: 'exec-1' })
  ),
};

const MockAcpClientClass = class MockAcpClient {
  sessionId = mockSessionClient.sessionId;
  initialize = mockSessionClient.initialize;
  newSession = (...args: any[]) => {
    const result = mockSessionClient.newSession(...(args as []));
    result.then((r: any) => {
      this.sessionId = r.sessionId;
    });
    return result;
  };
  loadSession = mockSessionClient.loadSession;
  prompt = mockSessionClient.prompt;
  cancel = mockSessionClient.cancel;
  close = mockSessionClient.close;
  onUpdate = mockSessionClient.onUpdate;
  executeCommand = mockSessionClient.executeCommand;
  getCommandOptions = mockSessionClient.getCommandOptions;
  setMode = mockSessionClient.setMode;
  listSettings = mockSessionClient.listSettings;
  setSetting = mockSessionClient.setSetting;
  terminateSession = mockSessionClient.terminateSession;
  listSessions = mockSessionClient.listSessions;
  resolveSpecSession = mockSessionClient.resolveSpecSession;
  invokeSpec = mockSessionClient.invokeSpec;
};

mock.module('../acp-client', () => ({
  AcpClient: MockAcpClientClass,
  createAcpClient: () => new MockAcpClientClass(),
}));

// Import after mocks — query-string specifier avoids collisions with other test files.
// @ts-expect-error — query-string specifier bypasses bun's mock registry
const { Kiro } = await import('../kiro?cancel-timeout');

describe('kiro.cancel() timeout (cancel hang bug)', () => {
  beforeEach(() => {
    mockSessionClient.cancel.mockClear();
    mockSessionClient.prompt.mockClear();
  });

  it('resolves within timeout when pendingPrompt never settles', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');

    // Simulate a pendingPrompt that never resolves (stuck KAS backend).
    // Access private field via cast.
    (kiro as any).pendingPrompt = new Promise<void>(() => {
      // intentionally never resolves
    });

    const start = Date.now();
    await kiro.cancel();
    const elapsed = Date.now() - start;

    // Should resolve within ~5s timeout, not hang. Allow generous margin.
    expect(elapsed).toBeLessThan(10_000);
    // pendingPrompt must be nulled out
    expect((kiro as any).pendingPrompt).toBeNull();
  }, 15_000);

  it('resolves immediately when pendingPrompt settles before timeout', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');

    // pendingPrompt that resolves quickly
    (kiro as any).pendingPrompt = Promise.resolve();

    const start = Date.now();
    await kiro.cancel();
    const elapsed = Date.now() - start;

    expect(elapsed).toBeLessThan(1000);
    expect((kiro as any).pendingPrompt).toBeNull();
  });

  it('nulls pendingPrompt even when it rejects', async () => {
    const kiro = new Kiro();
    await kiro.initialize('/path/to/agent');

    // pendingPrompt that rejects (swallowed by .then(() => {}, () => {}))
    (kiro as any).pendingPrompt = Promise.resolve();

    await kiro.cancel();

    expect((kiro as any).pendingPrompt).toBeNull();
  });
});
