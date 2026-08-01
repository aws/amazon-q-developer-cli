/**
 * Classify a failed cloud-sandbox RPC into the bounded `cloud_error_kind`
 * enum backing `kiro_cli_cloud_error_total`, and map each kind to the
 * user-facing guidance line shown instead of the raw backend error.
 *
 * Classification is string-based because the relay flattens backend errors
 * into JSON-RPC `message`/`data` text (there is no structured error code on
 * this path today — see the ORR audit; when KAS grows structured codes,
 * prefer them here). The match set is the union of what the 07/16 live
 * scenario runs actually produced (`UnknownOperationException`,
 * "relayed stream ended without a done frame") and the standard AWS error
 * families (throttling, auth, timeouts).
 */

import { extractRpcErrorMessage } from './error-handling';

/** Mirrors the `cloud_error_kind` closed enum in the schema catalog. */
export type CloudErrorKind =
  | 'throttling'
  | 'auth'
  | 'version_skew'
  | 'not_found'
  | 'network'
  | 'timeout'
  | 'stream_truncated'
  | 'server_error'
  | 'other';

/** Mirrors the `cloud_op` closed enum in the schema catalog. */
export type CloudOp =
  | 'session_new'
  | 'session_load'
  | 'turn_stream'
  | 'source_providers_list'
  | 'source_providers_resources'
  | 'list_sessions'
  | 'delete_session';

export function classifyCloudError(error: unknown): CloudErrorKind {
  const msg = extractRpcErrorMessage(error, '').toLowerCase();
  if (!msg) return 'other';
  // KAS↔BFF deployment skew: the deployed BFF does not know the operation the
  // (newer/older) KAS called. Proven failure mode — every remote call fails
  // this way when versions diverge.
  if (
    msg.includes('unknownoperation') ||
    msg.includes('unknown operation') ||
    msg.includes('unknown ext method') ||
    msg.includes('unsupported operation') ||
    // An un-routed operation reaches us flattened to just "UnknownError" —
    // the exception class name is dropped in transit.
    msg.includes('unknownerror')
  ) {
    return 'version_skew';
  }
  if (
    msg.includes('throttl') ||
    msg.includes('quota') ||
    msg.includes('too many requests') ||
    msg.includes('429')
  ) {
    return 'throttling';
  }
  if (
    msg.includes('unauthorized') ||
    msg.includes('access denied') ||
    msg.includes('accessdenied') ||
    msg.includes('forbidden') ||
    msg.includes('expired token') ||
    msg.includes('token expired') ||
    // Compact AWS credential exception class names as the relay surfaces
    // them: ExpiredTokenException, InvalidClientTokenId.
    msg.includes('expiredtoken') ||
    msg.includes('invalidclient')
  ) {
    return 'auth';
  }
  if (
    msg.includes('without a done frame') ||
    // Require stream context for the bare word: "payload truncated by
    // gateway" etc. must not shadow the not_found/server_error branches.
    (msg.includes('truncated') && msg.includes('stream'))
  ) {
    return 'stream_truncated';
  }
  // DNS resolution failures FIRST: Node's canonical `getaddrinfo ENOTFOUND
  // host` contains "notfound" and would otherwise hit the not_found branch.
  if (msg.includes('enotfound')) {
    return 'network';
  }
  if (
    msg.includes('not found') ||
    msg.includes('notfound') ||
    msg.includes('does not exist')
  ) {
    return 'not_found';
  }
  if (msg.includes('timeout') || msg.includes('timed out')) {
    return 'timeout';
  }
  if (
    msg.includes('network') ||
    msg.includes('connection') ||
    msg.includes('econnrefused') ||
    msg.includes('econnreset') ||
    msg.includes('socket') ||
    msg.includes('dns')
  ) {
    return 'network';
  }
  if (
    msg.includes('internal error') ||
    msg.includes('internal server') ||
    msg.includes('service unavailable') ||
    msg.includes('500') ||
    msg.includes('503')
  ) {
    return 'server_error';
  }
  return 'other';
}

/**
 * User-facing guidance per failure class — shown alongside (not instead of)
 * the underlying error so support still gets the raw cause. `undefined`
 * means no extra guidance beyond the raw message.
 */
export function cloudErrorGuidance(kind: CloudErrorKind): string | undefined {
  switch (kind) {
    case 'throttling':
      return 'The cloud service is rate-limiting requests. Wait a moment and retry.';
    case 'auth':
      return 'Your session credentials were rejected. Run `kiro-cli login` in a new terminal, then retry.';
    case 'version_skew':
      return 'The cloud service and your CLI are out of sync. Update kiro (`kiro update`) or retry later — the service may be mid-deployment.';
    case 'stream_truncated':
      return 'The connection to the cloud session dropped mid-response. Your session is still running — resume it to reattach.';
    case 'network':
      return 'Could not reach the cloud service. Check your network connection and retry.';
    case 'timeout':
      return 'The cloud service took too long to respond. Retry; if it persists the service may be degraded.';
    default:
      return undefined;
  }
}
