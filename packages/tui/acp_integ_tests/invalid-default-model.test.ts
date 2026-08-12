/**
 * ACP wire-level test for an unavailable `chat.defaultModel`.
 *
 * Repro: a user (e.g. social tier) configures a default model they can't
 * access. KAS optimistically accepts the id but it's absent from the
 * available model list. The model chip must show nothing — matching the
 * Rust engine — not a misleading "Auto" fallback.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

interface SetConfigOptionParams {
  sessionId: string;
  configId: string;
  value: unknown;
}

const MODEL_OPTIONS = [
  { value: 'auto', name: 'Auto' },
  { value: 'claude-sonnet-4', name: 'Claude Sonnet 4' },
];

function modelConfigOption(currentValue: string) {
  return {
    type: 'select' as const,
    id: 'model',
    name: 'Model',
    category: 'model',
    currentValue,
    options: MODEL_OPTIONS,
  };
}

function setupHandshake(tc: AcpTestCase, setModelCurrentValue: string): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  // session/new reports the user's real model list with `auto` selected.
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'default-model-session',
    modes: defaultKasModes(),
    configOptions: [modelConfigOption('auto')],
  }));

  // KAS echoes whatever model id it's told to set as currentValue, even when
  // it's not in `options` (mirrors agent.ts setSessionConfigOption).
  tc.mock.on('session/set_config_option', (params: SetConfigOptionParams) => {
    if (params.configId === 'model') {
      return { configOptions: [modelConfigOption(setModelCurrentValue)] };
    }
    return {};
  });
}

describe('unavailable chat.defaultModel', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('shows no model chip (not "Auto") when the default model is unavailable', async () => {
    /**
     * GIVEN  chat.defaultModel = an id absent from the available list
     * WHEN   the session is created (sets the model, KAS echoes the bad id)
     * THEN   store.currentModel is null — no misleading "Auto" chip
     */
    tc = new AcpTestCase({
      testName: 'invalid-default-model',
      settings: { 'chat.defaultModel': 'claude-opus-4.8' },
    });
    setupHandshake(tc, 'claude-opus-4.8');

    await tc.launch();
    await tc.mock.awaitConnection();

    const store = await tc.waitForStore((s) => s.isInitialized === true, 8000);

    // The bad default was sent to KAS...
    const modelReqs = tc.mock
      .receivedRequests('session/set_config_option')
      .filter((r) => (r.params as SetConfigOptionParams).configId === 'model');
    expect(modelReqs.length).toBeGreaterThanOrEqual(1);
    // ...but it resolves to no chip, not "Auto".
    expect(store.currentModel).toBeNull();
  });

  it('shows the model chip when the default model is available', async () => {
    /**
     * Companion to the above: a valid default model DOES surface a chip,
     * so the null result is specific to the unavailable case.
     */
    tc = new AcpTestCase({
      testName: 'valid-default-model',
      settings: { 'chat.defaultModel': 'claude-sonnet-4' },
    });
    setupHandshake(tc, 'claude-sonnet-4');

    await tc.launch();
    await tc.mock.awaitConnection();

    const store = await tc.waitForStore(
      (s) => s.currentModel?.id === 'claude-sonnet-4',
      8000
    );
    expect(store.currentModel).toEqual({
      id: 'claude-sonnet-4',
      name: 'Claude Sonnet 4',
    });
  });
});
