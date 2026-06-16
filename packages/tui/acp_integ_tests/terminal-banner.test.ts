/**
 * Verifies that ASBX_KIRO_TERMINAL_BANNER env var renders at session start.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

describe('ASBX_KIRO_TERMINAL_BANNER', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('displays banner text at session start when env var is set', async () => {
    tc = new AcpTestCase({
      testName: 'terminal-banner',
      extraEnv: {
        ASBX_KIRO_TERMINAL_BANNER:
          'You are running in a sandboxed environment.',
      },
    });

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-banner',
      modes: defaultKasModes(),
    }));

    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();

    await tc.waitForVisibleText('sandboxed environment', 3000);
    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('You are running in a sandboxed environment.');
  });

  it('does not display banner when env var is absent', async () => {
    tc = new AcpTestCase({
      testName: 'terminal-banner-absent',
      extraEnv: { ASBX_KIRO_TERMINAL_BANNER: '' },
    });

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-no-banner',
      modes: defaultKasModes(),
    }));

    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(500);

    const snap = tc.getSnapshotFormatted();
    expect(snap).not.toContain('sandboxed environment');
  });
});
