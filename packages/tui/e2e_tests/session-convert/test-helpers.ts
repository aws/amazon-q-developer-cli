/**
 * Shared test helpers for session-convert e2e tests.
 *
 * - `setupAcpHandshake` registers the minimum mock handlers every
 *   --resume / --resume-id / /chat-load test needs to get past the
 *   ACP boot sequence: `initialize` + `session/load` +
 *   `session/set_config_option`. Tests that boot without
 *   `--resume*` (e.g. /chat picker, /chat load) opt in to a
 *   `session/new` handler by passing `bootSessionId`.
 * - `kasSessionIdPatternFor` builds the regex every flow test pins
 *   for the converted KAS session id, which is `cli_{sourceId}_{8
 *   alphanumeric}`. Using a builder avoids drift when the suffix
 *   length or alphabet changes.
 */

import type {
  InitializeRequest,
  InitializeResponse,
  LoadSessionRequest,
  LoadSessionResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import type { AcpTestCase } from '../../acp_integ_tests/shared/AcpTestCase';

export interface SetupAcpHandshakeOptions {
  /**
   * When set, registers a `session/new` handler returning this id.
   * Required for tests that boot without `--resume*` (the TUI's boot
   * path issues `session/new` to mint a fresh session before the
   * test exercises /chat or /chat load).
   */
  bootSessionId?: string;
}

export function setupAcpHandshake(
  tc: AcpTestCase,
  options: SetupAcpHandshakeOptions = {}
): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  if (options.bootSessionId !== undefined) {
    const bootSessionId = options.bootSessionId;
    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: bootSessionId,
      modes: {
        currentModeId: 'vibe',
        availableModes: [{ id: 'vibe', name: 'Default' }],
      },
    }));
  }
  tc.mock.on<LoadSessionRequest, LoadSessionResponse>('session/load', () => ({
    modes: {
      currentModeId: 'vibe',
      availableModes: [{ id: 'vibe', name: 'Default' }],
    },
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

export function kasSessionIdPatternFor(sourceId: string): RegExp {
  return new RegExp(`^cli_${sourceId}_[A-Za-z0-9]{8}$`);
}
