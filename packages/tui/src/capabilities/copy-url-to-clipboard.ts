/**
 * ACP capability for `_kiro/openExternalUrl`.
 *
 * KAS calls this method when it wants the client to surface a URL to the
 * user — primarily MCP OAuth flows, where the agent starts a local redirect
 * server and needs the user to complete authentication in a browser.
 *
 * Rather than launching a browser automatically, the CLI copies the URL to
 * the system clipboard so the user can open it wherever they like. This
 * mirrors the V2 (Rust agent) OAuth flow (see `utils/mcp-oauth.ts`).
 *
 * NOTE: the wire method name stays `_kiro/openExternalUrl` — that is KAS's
 * protocol contract. Only the client-side behaviour (copy vs. open) and the
 * local function names reflect what we actually do.
 */

import type { ClientCapability } from '@kiro/acp-type-covenant';
import { copyToSystemClipboard } from '../commands/effects.js';
import { formatOAuthUrlFallback } from '../utils/mcp-oauth.js';
import { logger } from '../utils/logger.js';

/** Wire method name KAS uses; kept verbatim as the protocol contract. */
export const OPEN_EXTERNAL_URL_METHOD = '_kiro/openExternalUrl';

export interface CopyUrlToClipboardRequest {
  url: string;
  [key: string]: unknown;
}

export interface CopyUrlToClipboardResponse {
  success: boolean;
  [key: string]: unknown;
}

/**
 * Copy the requested URL to the system clipboard and report the outcome.
 *
 * The URL itself is never logged — OAuth authorization URLs carry sensitive
 * query parameters (state, client_id, PKCE challenge).
 *
 * `copyToClipboard` is injectable for testing; production uses the real
 * system clipboard.
 */
function handleCopyUrlToClipboard(
  request: CopyUrlToClipboardRequest,
  copyToClipboard: (text: string) => boolean,
  onCopyFailure: (message: string) => void
): CopyUrlToClipboardResponse {
  const success = copyToClipboard(request.url);
  if (success) {
    logger.info('[copy-url] Copied URL to clipboard');
  } else {
    logger.warn('[copy-url] Failed to copy URL — no clipboard tool found');
    onCopyFailure(formatOAuthUrlFallback(request.url));
  }
  return { success };
}

/**
 * Build the `_kiro/openExternalUrl` capability for registration with
 * `KiroClient`. When KAS asks the client to open a URL, the CLI copies it to
 * the clipboard instead of launching a browser.
 */
export function createCopyUrlToClipboardCapability(
  copyToClipboard: (text: string) => boolean = copyToSystemClipboard,
  onCopyFailure: (message: string) => void = () => {}
): ClientCapability<
  typeof OPEN_EXTERNAL_URL_METHOD,
  CopyUrlToClipboardRequest,
  CopyUrlToClipboardResponse
> {
  return {
    type: 'other',
    key: 'openExternalUrl',
    value: true,
    method: OPEN_EXTERNAL_URL_METHOD,
    handler: async (request: CopyUrlToClipboardRequest) =>
      handleCopyUrlToClipboard(request, copyToClipboard, onCopyFailure),
  };
}
