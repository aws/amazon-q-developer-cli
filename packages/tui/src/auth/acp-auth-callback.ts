/**
 * ACP capability for `_kiro/auth/getAccessToken`.
 *
 * KAS launches with `--auth=acp-callback` and calls back to this host
 * whenever it needs a fresh OIDC access token (first acquisition,
 * pre-expiry refresh, or hard-expired refresh). The host owns the
 * refresh token; KAS only ever sees access tokens.
 *
 * KAS sends an empty payload (`{}`); the host's behavior is identical
 * regardless of why the call was made, so KAS forwards no hints. The
 * handler shells out to the hidden `kiro-cli chat _ get-kas-token`
 * subcommand which:
 *   - acquires the cross-process refresh lock,
 *   - resolves the highest-priority cached token (External -> Builder -> Social),
 *   - refreshes it via OIDC if it has crossed expiry,
 *   - prints `{kind: "getKasToken", data: {accessToken, expiresAt, profileArn, authMethod?}}`.
 *
 * The handler returns `{accessToken, expiresAt, profileArn, authMethod?}`
 * to KAS. `authMethod` (e.g. `external_idp`) lets KAS apply the matching
 * `TokenType` header; absent for auth types that need none.
 *
 * # Failure mode
 *
 * Every failure path - spawn error, non-zero exit, parse failure,
 * `error` envelope from chat-cli, unexpected response kind - collapses
 * to a single user-facing message. Internal diagnostics go to
 * `logger` only. The throw becomes an ACP `Internal error` with the
 * user-facing string in `data.details`, which the TUI surfaces as a
 * readable sentence rather than the previous "No output from kiro-cli"
 * debug leak.
 */

import type {
  ClientCapability,
  GetAccessTokenResponse,
} from '@kiro/acp-type-covenant';

import {
  type AsyncSpawner,
  runChatInternalAsync,
} from '../utils/chat-internal-cli';
import { logger } from '../utils/logger';

export type { AsyncSpawner };

/**
 * Method name for the agent->client extension. Matches the typed entry
 * in `@kiro/acp-type-covenant/client-capabilities/index.ts`. Centralised
 * here so the registration site and the type covenant can't drift.
 */
export const GET_ACCESS_TOKEN_METHOD = '_kiro/auth/getAccessToken';

/** Wire request from KAS. Empty. */
export type GetAccessTokenRequest = Record<string, never>;

/** User-facing string surfaced when the host can't produce a token. */
export const AUTH_ERROR_USER_FACING =
  'Failed to verify authentication. Please log in again to continue.';

async function runGetKasToken(
  _request: GetAccessTokenRequest,
  spawner?: AsyncSpawner
): Promise<GetAccessTokenResponse> {
  const r = await runChatInternalAsync(['chat', '_', 'get-kas-token'], spawner);
  if (!r.ok) {
    logger.error('[auth-callback] host-side failure', { reason: r.message });
    throw new Error(AUTH_ERROR_USER_FACING);
  }
  if (r.output.kind === 'error') {
    logger.warn('[auth-callback] error envelope from chat-cli', r.output.data);
    throw new Error(AUTH_ERROR_USER_FACING);
  }
  if (r.output.kind !== 'getKasToken') {
    logger.error('[auth-callback] unexpected response kind', {
      kind: r.output.kind,
    });
    throw new Error(AUTH_ERROR_USER_FACING);
  }
  logger.debug('[auth-callback] token acquired', {
    expiresAt: r.output.data.expiresAt,
    profileArn: r.output.data.profileArn,
  });
  // Forward the optional `authMethod` (e.g. `external_idp`) so the KAS-side
  // `AcpCallbackAuthProvider` can apply the matching `TokenType` header.
  // Spread keeps the field optional on the wire; it is absent for auth
  // types (Builder ID / IdC / Social) where the subcommand sends nothing.
  const authMethod = r.output.data.authMethod
    ? { authMethod: r.output.data.authMethod }
    : {};
  // Forward the sign-in `provider` so KAS's GovernanceService can decide
  // enterprise status (only Enterprise / ExternalIdp are governed). Without
  // it KAS falls back to profileArn presence and misclassifies Builder ID /
  // social users as enterprise, fail-closing governance.
  const provider = r.output.data.provider
    ? { provider: r.output.data.provider }
    : {};
  return {
    accessToken: r.output.data.accessToken,
    expiresAt: r.output.data.expiresAt,
    profileArn: r.output.data.profileArn,
    ...authMethod,
    ...provider,
  };
}

/**
 * Build the `_kiro/auth/getAccessToken` capability for registration with
 * `KiroClient`. Pass to `KiroClient`'s `capabilities` array. The factory
 * accepts an optional spawner (used in tests) and otherwise wires the
 * production `Bun.spawn` path.
 */
export function createGetAccessTokenCapability(
  spawner?: AsyncSpawner
): ClientCapability<
  typeof GET_ACCESS_TOKEN_METHOD,
  GetAccessTokenRequest,
  GetAccessTokenResponse
> {
  return {
    type: 'other',
    key: 'authCallback',
    value: true,
    method: GET_ACCESS_TOKEN_METHOD,
    handler: (request: GetAccessTokenRequest) =>
      runGetKasToken(request, spawner),
  };
}
