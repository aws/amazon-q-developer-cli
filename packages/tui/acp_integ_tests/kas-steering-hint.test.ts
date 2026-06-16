/**
 * ACP integ test: the prompt placeholder must NOT advertise mid-turn steering
 * (the "… to steer" Ctrl+S toggle hint) on KAS, which has no backend steering.
 * While the agent is working, KAS shows only "Type to queue".
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
  PromptRequest,
  PromptResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';

const SESSION_ID = 'steer-hint-session-1';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: SESSION_ID,
    modes: {
      currentModeId: 'vibe',
      availableModes: [{ id: 'vibe', name: 'Default' }],
    },
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

describe('KAS steering hint gating', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('omits the steer toggle hint from the working placeholder on KAS', async () => {
    tc = new AcpTestCase({ testName: 'kas-steering-hint' });
    setupHandshake(tc);
    // Hold the turn open so the TUI stays in the "working" state while we
    // inspect the prompt placeholder.
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await new Promise((r) => setTimeout(r, 5000));
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('do something');
    await tc.pressEnter();
    // Wait for the live screen to render the "working" placeholder. This both
    // proves the placeholder shows mid-turn and refreshes the PTY buffer before
    // we snapshot for the negative assertion below.
    await tc.waitForVisibleText('Kiro is working', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('Type to queue');
    // KAS has no steering, so the "… to steer" toggle hint must be absent.
    expect(snapshot).not.toContain('to steer');
  }, 25000);
});
