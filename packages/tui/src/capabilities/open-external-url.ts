/**
 * ACP capability for `_kiro/openExternalUrl`.
 *
 * KAS calls this when it needs the client to open a URL in the user's
 * browser — primarily for MCP OAuth flows where the agent starts a local
 * redirect server and needs the user to complete authentication in their
 * browser.
 */

import type { ClientCapability } from '@kiro/acp-type-covenant';
import { browserOpenCommand } from '../acp-client.js';
import { logger } from '../utils/logger.js';

export const OPEN_EXTERNAL_URL_METHOD = '_kiro/openExternalUrl';

export interface OpenExternalUrlRequest {
  url: string;
  [key: string]: unknown;
}

export interface OpenExternalUrlResponse {
  success: boolean;
  [key: string]: unknown;
}

function detectWsl(): boolean {
  try {
    return (
      process.platform === 'linux' &&
      require('fs').readFileSync('/proc/version', 'utf8').includes('microsoft')
    );
  } catch {
    return false;
  }
}

async function handleOpenExternalUrl(
  request: OpenExternalUrlRequest
): Promise<OpenExternalUrlResponse> {
  const { url } = request;
  logger.info('[open-external-url] Opening URL in browser', { url });

  try {
    const { execFileSync } = require('child_process');
    const { file, args } = browserOpenCommand(
      process.platform,
      url,
      detectWsl()
    );
    execFileSync(file, args, { stdio: 'ignore' });
    return { success: true };
  } catch (error) {
    logger.warn('[open-external-url] Failed to open browser', {
      error: error instanceof Error ? error.message : String(error),
    });
    return { success: false };
  }
}

/**
 * Build the `_kiro/openExternalUrl` capability for registration with
 * `KiroClient`. Enables KAS to open OAuth URLs in the user's browser.
 */
export function createOpenExternalUrlCapability(): ClientCapability<
  typeof OPEN_EXTERNAL_URL_METHOD,
  OpenExternalUrlRequest,
  OpenExternalUrlResponse
> {
  return {
    type: 'other',
    key: 'openExternalUrl',
    value: true,
    method: OPEN_EXTERNAL_URL_METHOD,
    handler: handleOpenExternalUrl,
  };
}
