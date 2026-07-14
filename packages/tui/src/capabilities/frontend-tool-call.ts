/**
 * ACP client capability for `_kiro/frontendToolCall`.
 *
 * Some tools of a relayed (cloud) session are meant to run in the client rather
 * than on the sandbox; KAS forwards such a call to this handler and pipes the
 * reply back. The CLI hosts no client-side tools yet, so every call is declined
 * with `{ outcome: 'cancelled' }` — the reply required so a relayed turn never
 * hangs. Inert for local sessions, which run their tools inline.
 *
 * To host a tool later, dispatch on `request.toolCallId` and return
 * `{ outcome: 'completed', output }` instead.
 */

import type { ClientCapability } from '@kiro/acp-type-covenant';
import { logger } from '../utils/logger.js';

/** Wire method name KAS uses; kept verbatim as the protocol contract. */
export const FRONTEND_TOOL_CALL_METHOD = '_kiro/frontendToolCall';

/**
 * TODO: request/response types are defined CLI-side only; extend
 * `@kiro/acp-type-covenant` with the `_kiro/frontendToolCall` contract and
 * import from there before the cloud-sandbox rollout ramps.
 */
export interface FrontendToolCallRequest {
  sessionId: string;
  toolCallId: string;
  title?: string;
  rawInput?: unknown;
  [key: string]: unknown;
}

export type FrontendToolCallResponse =
  | { outcome: 'completed'; output: unknown }
  | { outcome: 'cancelled' };

/** Build the `_kiro/frontendToolCall` capability for registration with `KiroClient`. */
export function createFrontendToolCallCapability(): ClientCapability<
  typeof FRONTEND_TOOL_CALL_METHOD,
  FrontendToolCallRequest,
  FrontendToolCallResponse
> {
  return {
    type: 'other',
    key: 'frontendToolCall',
    value: true,
    method: FRONTEND_TOOL_CALL_METHOD,
    handler: async (request: FrontendToolCallRequest) => {
      // No client-hosted tools yet — decline so the relayed turn never hangs.
      // (rawInput is intentionally not logged; it may carry tool arguments.)
      logger.debug(
        `[frontend-tool-call] declining unhosted client tool ` +
          `(toolCallId=${request.toolCallId}, title=${request.title ?? '<none>'})`
      );
      return { outcome: 'cancelled' };
    },
  };
}
